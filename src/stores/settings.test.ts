import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { useTheme } from "../composables/useTheme";
import { settingsGet, settingsUpdate } from "../services/settingsApi";
import type { AppSettings } from "../types/generated";
import { useSettingsStore } from "./settings";

vi.mock("../services/settingsApi", () => ({
  settingsGet: vi.fn(),
  settingsUpdate: vi.fn(),
}));

const baseSettings: AppSettings = {
  dataDir: "data",
  logDir: "logs",
  backupDir: "backups",
  shortcut: "Ctrl+Alt+N",
  shortcutRegistered: true,
  shortcutError: null,
  autostartEnabled: false,
  themeMode: "system",
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

describe("settings store", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.mocked(settingsGet).mockReset();
    vi.mocked(settingsUpdate).mockReset();
    useTheme().setMode("system");
  });

  it("rolls back autostart when update fails", async () => {
    const store = useSettingsStore();
    store.settings = { ...baseSettings };
    vi.mocked(settingsUpdate).mockRejectedValue(new Error("denied"));

    await expect(store.setAutostart(true)).rejects.toThrow("denied");

    expect(store.settings?.autostartEnabled).toBe(false);
    expect(store.error).toBe("denied");
  });

  it("keeps server settings when autostart update succeeds", async () => {
    const store = useSettingsStore();
    store.settings = { ...baseSettings };
    vi.mocked(settingsUpdate).mockResolvedValue({
      ...baseSettings,
      autostartEnabled: true,
    });

    await store.setAutostart(true);

    expect(store.settings?.autostartEnabled).toBe(true);
    expect(store.error).toBeNull();
  });

  it("serializes concurrent theme and autostart updates with delayed responses", async () => {
    const store = useSettingsStore();
    store.settings = { ...baseSettings };

    const themeUpdate = deferred<AppSettings>();
    const autostartUpdate = deferred<AppSettings>();
    const callOrder: string[] = [];
    let inFlight = 0;
    let maxInFlight = 0;

    vi.mocked(settingsUpdate).mockImplementation((patch) => {
      inFlight += 1;
      maxInFlight = Math.max(maxInFlight, inFlight);
      const pending =
        patch.themeMode != null ? themeUpdate.promise : autostartUpdate.promise;
      callOrder.push(patch.themeMode != null ? "theme" : "autostart");
      return pending.finally(() => {
        inFlight -= 1;
      });
    });

    const themePromise = store.setThemeMode("dark");
    const autostartPromise = store.setAutostart(true);

    // Allow microtasks to enqueue the first write.
    await Promise.resolve();
    await Promise.resolve();

    expect(callOrder).toEqual(["theme"]);
    expect(store.themeSaving).toBe(true);
    expect(store.autostartSaving).toBe(true);

    // Respond to theme first with a partial/old snapshot that still has autostart=false.
    themeUpdate.resolve({
      ...baseSettings,
      themeMode: "dark",
      autostartEnabled: false,
    });
    await themePromise;
    await Promise.resolve();
    await Promise.resolve();

    expect(callOrder).toEqual(["theme", "autostart"]);
    expect(store.settings?.themeMode).toBe("dark");

    // Autostart response still has old theme=system; merge must keep dark.
    autostartUpdate.resolve({
      ...baseSettings,
      themeMode: "system",
      autostartEnabled: true,
    });
    await autostartPromise;

    expect(maxInFlight).toBe(1);
    expect(callOrder).toEqual(["theme", "autostart"]);
    expect(store.settings?.themeMode).toBe("dark");
    expect(store.settings?.autostartEnabled).toBe(true);
    expect(useTheme().mode.value).toBe("dark");
    expect(store.themeSaving).toBe(false);
    expect(store.autostartSaving).toBe(false);
  });

  it("does not let a stale load overwrite a newer theme write", async () => {
    const store = useSettingsStore();
    const loadResponse = deferred<AppSettings>();
    vi.mocked(settingsGet).mockReturnValue(loadResponse.promise);
    vi.mocked(settingsUpdate).mockResolvedValue({
      ...baseSettings,
      themeMode: "dark",
    });

    const loadPromise = store.load();
    await Promise.resolve();

    await store.setThemeMode("dark");
    expect(store.settings?.themeMode).toBe("dark");
    expect(useTheme().mode.value).toBe("dark");

    loadResponse.resolve({ ...baseSettings, themeMode: "system" });
    await loadPromise;

    expect(store.settings?.themeMode).toBe("dark");
    expect(useTheme().mode.value).toBe("dark");
    // Stale load must not wipe a successful write error channel either.
    expect(store.error).toBeNull();
  });

  it("reuses a single settingsGet when ensureLoaded is called concurrently", async () => {
    const store = useSettingsStore();
    const loadResponse = deferred<AppSettings>();
    vi.mocked(settingsGet).mockReturnValue(loadResponse.promise);

    const first = store.ensureLoaded();
    const second = store.ensureLoaded();
    expect(settingsGet).toHaveBeenCalledTimes(1);

    loadResponse.resolve({ ...baseSettings });
    await Promise.all([first, second]);

    expect(settingsGet).toHaveBeenCalledTimes(1);
    expect(store.settings).toEqual(baseSettings);

    await store.ensureLoaded();
    expect(settingsGet).toHaveBeenCalledTimes(1);
  });

  it("rolls back only the failed field after a successful autostart write", async () => {
    const store = useSettingsStore();
    store.settings = { ...baseSettings };

    vi.mocked(settingsUpdate).mockImplementation(async (patch) => {
      if (patch.autostartEnabled != null) {
        return {
          ...baseSettings,
          autostartEnabled: true,
          themeMode: "system",
        };
      }
      throw new Error("theme denied");
    });

    await store.setAutostart(true);
    expect(store.settings?.autostartEnabled).toBe(true);

    await expect(store.setThemeMode("dark")).rejects.toThrow("theme denied");

    expect(store.settings?.autostartEnabled).toBe(true);
    expect(store.settings?.themeMode).toBe("system");
    expect(useTheme().mode.value).toBe("system");
    expect(store.error).toBe("theme denied");
  });

  it("keeps last successful theme when a later theme update fails after another field write", async () => {
    const store = useSettingsStore();
    store.settings = { ...baseSettings };

    vi.mocked(settingsUpdate)
      .mockResolvedValueOnce({
        ...baseSettings,
        themeMode: "dark",
      })
      .mockResolvedValueOnce({
        ...baseSettings,
        themeMode: "dark",
        autostartEnabled: true,
      })
      .mockRejectedValueOnce(new Error("theme denied"));

    await store.setThemeMode("dark");
    await store.setAutostart(true);
    await expect(store.setThemeMode("light")).rejects.toThrow("theme denied");

    expect(store.settings?.autostartEnabled).toBe(true);
    expect(store.settings?.themeMode).toBe("dark");
    expect(useTheme().mode.value).toBe("dark");
  });
});
