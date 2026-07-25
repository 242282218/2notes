import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { settingsUpdate } from "../services/settingsApi";
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

describe("settings store", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.mocked(settingsUpdate).mockReset();
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
});
