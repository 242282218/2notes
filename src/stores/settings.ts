import { defineStore } from "pinia";
import { ref } from "vue";

import { settingsGet, settingsUpdate } from "../services/settingsApi";
import { useTheme, type ThemeMode } from "../composables/useTheme";
import type { AppSettings } from "../types/generated";

export const useSettingsStore = defineStore("settings", () => {
  const settings = ref<AppSettings | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function load() {
    loading.value = true;
    error.value = null;
    try {
      const next = await settingsGet();
      settings.value = next;
      // Sync local UI theme with persisted backend value (handles cross-device
      // or reset scenarios where localStorage may diverge from the DB).
      useTheme().setMode(next.themeMode as ThemeMode);
    } catch (loadError) {
      error.value =
        loadError instanceof Error ? loadError.message : "设置加载失败";
    } finally {
      loading.value = false;
    }
  }

  async function setAutostart(enabled: boolean) {
    const previous = settings.value;
    error.value = null;
    if (settings.value) {
      settings.value = { ...settings.value, autostartEnabled: enabled };
    }
    try {
      settings.value = await settingsUpdate({
        autostartEnabled: enabled,
        themeMode: null,
      });
    } catch (updateError) {
      settings.value = previous;
      error.value =
        updateError instanceof Error
          ? updateError.message
          : "开机自启动设置失败";
      throw updateError;
    }
  }

  async function setThemeMode(themeMode: ThemeMode) {
    const previous = settings.value;
    error.value = null;
    if (settings.value) {
      settings.value = { ...settings.value, themeMode };
    }
    // Apply to local UI immediately for responsive feedback.
    useTheme().setMode(themeMode);
    try {
      settings.value = await settingsUpdate({
        autostartEnabled: null,
        themeMode,
      });
    } catch (updateError) {
      settings.value = previous;
      // Rollback UI to previous persisted mode.
      if (previous) {
        useTheme().setMode(previous.themeMode as ThemeMode);
      }
      error.value =
        updateError instanceof Error ? updateError.message : "主题设置失败";
      throw updateError;
    }
  }

  return {
    settings,
    loading,
    error,
    load,
    setAutostart,
    setThemeMode,
  };
});
