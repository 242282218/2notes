import { defineStore } from "pinia";
import { ref } from "vue";

import { settingsGet, settingsUpdate } from "../services/settingsApi";
import { useTheme } from "../composables/useTheme";
import type { AppSettings, SettingsPatch, ThemeMode } from "../types/generated";

type MutableField = "autostartEnabled" | "themeMode";

interface QueuedWrite {
  field: MutableField;
  target: boolean | ThemeMode;
  /** Last known persisted value for this field at enqueue time. */
  lastSuccess: boolean | ThemeMode | null;
  resolve: () => void;
  reject: (error: unknown) => void;
}

const READONLY_PATH_FIELDS = [
  "dataDir",
  "logDir",
  "backupDir",
  "shortcut",
  "shortcutRegistered",
  "shortcutError",
] as const satisfies ReadonlyArray<keyof AppSettings>;

function pickChangedFields(
  serverSettings: AppSettings,
  patch: SettingsPatch,
): Partial<AppSettings> {
  const next: Partial<AppSettings> = {};
  if (patch.autostartEnabled != null) {
    next.autostartEnabled = serverSettings.autostartEnabled;
  }
  if (patch.themeMode != null) {
    next.themeMode = serverSettings.themeMode;
  }
  return next;
}

function pickReadonlyPathFields(
  serverSettings: AppSettings,
): Partial<AppSettings> {
  const next: Partial<AppSettings> = {};
  for (const field of READONLY_PATH_FIELDS) {
    switch (field) {
      case "dataDir":
        next.dataDir = serverSettings.dataDir;
        break;
      case "logDir":
        next.logDir = serverSettings.logDir;
        break;
      case "backupDir":
        next.backupDir = serverSettings.backupDir;
        break;
      case "shortcut":
        next.shortcut = serverSettings.shortcut;
        break;
      case "shortcutRegistered":
        next.shortcutRegistered = serverSettings.shortcutRegistered;
        break;
      case "shortcutError":
        next.shortcutError = serverSettings.shortcutError;
        break;
    }
  }
  return next;
}

export const useSettingsStore = defineStore("settings", () => {
  const settings = ref<AppSettings | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const autostartSaving = ref(false);
  const themeSaving = ref(false);

  let loadPromise: Promise<void> | null = null;
  let mutationGeneration = 0;

  /** Last successfully persisted mutable values (not optimistic). */
  let lastPersistedAutostart: boolean | null = null;
  let lastPersistedTheme: ThemeMode | null = null;

  const writeQueue: QueuedWrite[] = [];
  let draining = false;

  function bumpMutationGeneration() {
    mutationGeneration += 1;
  }

  function refreshBusyFlags() {
    autostartSaving.value = writeQueue.some(
      (item) => item.field === "autostartEnabled",
    );
    themeSaving.value = writeQueue.some((item) => item.field === "themeMode");
  }

  function rememberPersistedFromSettings(next: AppSettings) {
    lastPersistedAutostart = next.autostartEnabled;
    lastPersistedTheme = next.themeMode;
  }

  function captureLastSuccess(field: MutableField): boolean | ThemeMode | null {
    // Prefer explicitly tracked persisted values.
    if (field === "autostartEnabled" && lastPersistedAutostart != null) {
      return lastPersistedAutostart;
    }
    if (field === "themeMode" && lastPersistedTheme != null) {
      return lastPersistedTheme;
    }

    // If earlier writes for this field are already queued, reuse the first
    // item's lastSuccess (the last truly persisted baseline).
    for (const item of writeQueue) {
      if (item.field === field) {
        return item.lastSuccess;
      }
    }

    // No pending writes and no tracked baseline yet: current store value is
    // the last known persisted snapshot (e.g. tests/direct assignment).
    if (field === "autostartEnabled") {
      return settings.value?.autostartEnabled ?? null;
    }
    return settings.value?.themeMode ?? null;
  }

  function applyOptimistic(field: MutableField, target: boolean | ThemeMode) {
    if (!settings.value) {
      if (field === "themeMode") {
        useTheme().setMode(target as ThemeMode);
      }
      return;
    }
    if (field === "autostartEnabled") {
      settings.value = {
        ...settings.value,
        autostartEnabled: target as boolean,
      };
      return;
    }
    settings.value = {
      ...settings.value,
      themeMode: target as ThemeMode,
    };
    useTheme().setMode(target as ThemeMode);
  }

  function rollbackField(
    field: MutableField,
    lastSuccess: boolean | ThemeMode | null,
  ) {
    if (lastSuccess == null) {
      return;
    }
    if (field === "autostartEnabled") {
      if (settings.value) {
        settings.value = {
          ...settings.value,
          autostartEnabled: lastSuccess as boolean,
        };
      }
      return;
    }
    if (settings.value) {
      settings.value = {
        ...settings.value,
        themeMode: lastSuccess as ThemeMode,
      };
    }
    useTheme().setMode(lastSuccess as ThemeMode);
  }

  function mergeServerResponse(
    serverSettings: AppSettings,
    patch: SettingsPatch,
  ) {
    settings.value = settings.value
      ? { ...settings.value, ...pickChangedFields(serverSettings, patch) }
      : serverSettings;
  }

  async function drainWriteQueue() {
    if (draining) {
      return;
    }
    draining = true;
    try {
      while (writeQueue.length > 0) {
        const item = writeQueue[0];
        const patch: SettingsPatch =
          item.field === "autostartEnabled"
            ? {
                autostartEnabled: item.target as boolean,
                themeMode: null,
              }
            : {
                autostartEnabled: null,
                themeMode: item.target as ThemeMode,
              };

        try {
          const serverSettings = await settingsUpdate(patch);
          mergeServerResponse(serverSettings, patch);

          // Keep the user intent for the field we just wrote, even if the
          // server snapshot is stale relative to other optimistic fields.
          if (item.field === "autostartEnabled") {
            if (settings.value) {
              settings.value = {
                ...settings.value,
                autostartEnabled: item.target as boolean,
              };
            }
            lastPersistedAutostart = item.target as boolean;
          } else {
            if (settings.value) {
              settings.value = {
                ...settings.value,
                themeMode: item.target as ThemeMode,
              };
            }
            lastPersistedTheme = item.target as ThemeMode;
            useTheme().setMode(item.target as ThemeMode);
          }

          // Re-apply later queued optimistic values so a completed write cannot
          // clobber a newer intent already waiting in the queue.
          for (const pending of writeQueue.slice(1)) {
            applyOptimistic(pending.field, pending.target);
          }

          error.value = null;
          item.resolve();
        } catch (updateError) {
          const laterSameField = writeQueue
            .slice(1)
            .filter((pending) => pending.field === item.field);
          if (laterSameField.length === 0) {
            rollbackField(item.field, item.lastSuccess);
          } else {
            const latest = laterSameField[laterSameField.length - 1];
            applyOptimistic(latest.field, latest.target);
          }
          error.value =
            updateError instanceof Error
              ? updateError.message
              : item.field === "autostartEnabled"
                ? "开机自启动设置失败"
                : "主题设置失败";
          item.reject(updateError);
        } finally {
          writeQueue.shift();
          refreshBusyFlags();
        }
      }
    } finally {
      draining = false;
      if (writeQueue.length > 0) {
        void drainWriteQueue();
      }
    }
  }

  function enqueueWrite(
    field: MutableField,
    target: boolean | ThemeMode,
  ): Promise<void> {
    bumpMutationGeneration();
    // Capture baseline before optimistic mutation.
    const lastSuccess = captureLastSuccess(field);
    applyOptimistic(field, target);

    return new Promise<void>((resolve, reject) => {
      writeQueue.push({
        field,
        target,
        lastSuccess,
        resolve,
        reject,
      });
      refreshBusyFlags();
      void drainWriteQueue();
    });
  }

  async function load() {
    if (loadPromise) {
      return loadPromise;
    }

    const generationAtStart = mutationGeneration;
    const hadPendingWritesAtStart = writeQueue.length > 0;
    loading.value = true;
    if (!hadPendingWritesAtStart) {
      error.value = null;
    }

    loadPromise = (async () => {
      try {
        const next = await settingsGet();
        const generationChanged = mutationGeneration !== generationAtStart;
        const hasPendingWrites = writeQueue.length > 0;

        if (generationChanged || hasPendingWrites) {
          // Stale relative to user writes: only fill missing settings or
          // refresh readonly path fields; never overwrite mutable fields,
          // theme UI, or error state.
          if (!settings.value) {
            settings.value = next;
            // Seed persisted baselines only for fields without pending writes.
            if (!writeQueue.some((item) => item.field === "autostartEnabled")) {
              lastPersistedAutostart = next.autostartEnabled;
            }
            if (!writeQueue.some((item) => item.field === "themeMode")) {
              lastPersistedTheme = next.themeMode;
            }
            const themeWrites = writeQueue.filter(
              (item) => item.field === "themeMode",
            );
            const latestThemeWrite = themeWrites[themeWrites.length - 1];
            if (latestThemeWrite) {
              settings.value = {
                ...settings.value,
                themeMode: latestThemeWrite.target as ThemeMode,
              };
              useTheme().setMode(latestThemeWrite.target as ThemeMode);
            } else {
              useTheme().setMode(next.themeMode);
            }
            const autostartWrites = writeQueue.filter(
              (item) => item.field === "autostartEnabled",
            );
            const latestAutostartWrite =
              autostartWrites[autostartWrites.length - 1];
            if (latestAutostartWrite) {
              settings.value = {
                ...settings.value,
                autostartEnabled: latestAutostartWrite.target as boolean,
              };
            }
          } else {
            settings.value = {
              ...settings.value,
              ...pickReadonlyPathFields(next),
            };
          }
          return;
        }

        settings.value = next;
        rememberPersistedFromSettings(next);
        useTheme().setMode(next.themeMode);
        error.value = null;
      } catch (loadError) {
        if (
          mutationGeneration === generationAtStart &&
          writeQueue.length === 0
        ) {
          error.value =
            loadError instanceof Error ? loadError.message : "设置加载失败";
        }
      } finally {
        loading.value = false;
        loadPromise = null;
      }
    })();

    return loadPromise;
  }

  async function ensureLoaded() {
    if (settings.value) {
      return;
    }
    return load();
  }

  async function setAutostart(enabled: boolean) {
    return enqueueWrite("autostartEnabled", enabled);
  }

  async function setThemeMode(themeMode: ThemeMode) {
    return enqueueWrite("themeMode", themeMode);
  }

  return {
    settings,
    loading,
    error,
    autostartSaving,
    themeSaving,
    load,
    ensureLoaded,
    setAutostart,
    setThemeMode,
  };
});
