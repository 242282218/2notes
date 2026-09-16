import { mount, flushPromises } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { settingsGet } from "../services/settingsApi";
import type { AppSettings } from "../types/generated";
import App from "./App.vue";

vi.mock("@tauri-apps/api/core", () => ({
  isTauri: vi.fn(() => false),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({ label: "main" })),
}));

vi.mock("../services/settingsApi", () => ({
  settingsGet: vi.fn(),
  settingsUpdate: vi.fn(),
}));

vi.mock("../components/layout/AppShell.vue", () => ({
  default: { name: "AppShell", template: "<div data-testid='app-shell' />" },
}));

vi.mock("../components/quick-capture/QuickCapture.vue", () => ({
  default: {
    name: "QuickCapture",
    template: "<div data-testid='quick-capture' />",
  },
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
  backupRetentionCount: 10,
};

describe("App.vue settings bootstrap", () => {
  const originalSearch = window.location.search;

  beforeEach(() => {
    setActivePinia(createPinia());
    vi.mocked(settingsGet).mockReset();
    vi.mocked(settingsGet).mockResolvedValue(baseSettings);
  });

  afterEach(() => {
    window.history.replaceState({}, "", `/${originalSearch}`);
  });

  it("calls settingsStore.ensureLoaded for the main window", async () => {
    window.history.replaceState({}, "", "/");
    const wrapper = mount(App, {
      global: { plugins: [createPinia()] },
    });
    await flushPromises();

    expect(settingsGet).toHaveBeenCalledTimes(1);
    expect(wrapper.find("[data-testid='app-shell']").exists()).toBe(true);
    wrapper.unmount();
  });

  it("does not call main-only settings_get for quick-capture view", async () => {
    window.history.replaceState({}, "", "/?view=quick-capture");
    const wrapper = mount(App, {
      global: { plugins: [createPinia()] },
    });
    await flushPromises();

    expect(settingsGet).not.toHaveBeenCalled();
    expect(wrapper.find("[data-testid='quick-capture']").exists()).toBe(true);
    wrapper.unmount();
  });
});
