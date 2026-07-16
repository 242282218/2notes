import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { backupsList } from "../../services/backupApi";
import { knowledgeRebuildIndex } from "../../services/knowledgeApi";
import { settingsGet } from "../../services/settingsApi";
import type { AppSettings, KnowledgeIndexReport } from "../../types/generated";
import SettingsView from "./SettingsView.vue";

vi.mock("../../services/backupApi", () => ({
  backupsList: vi.fn(),
  backupsCreate: vi.fn(),
  backupsRestore: vi.fn(),
}));

vi.mock("../../services/knowledgeApi", () => ({
  knowledgeRebuildIndex: vi.fn(),
}));

vi.mock("../../services/settingsApi", () => ({
  settingsGet: vi.fn(),
  settingsUpdate: vi.fn(),
}));

vi.mock("../../services/exportApi", () => ({
  exportMarkdown: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn() }));

describe("SettingsView", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.mocked(settingsGet).mockResolvedValue(baseSettings);
    vi.mocked(backupsList).mockResolvedValue([]);
    vi.mocked(knowledgeRebuildIndex).mockReset();
  });

  it("disables rebuild while the index operation is busy", async () => {
    const pending = deferred<KnowledgeIndexReport>();
    vi.mocked(knowledgeRebuildIndex).mockReturnValue(pending.promise);
    const wrapper = mount(SettingsView);
    await flushPromises();

    const button = wrapper.get('button[aria-label="重建知识索引"]');
    await button.trigger("click");

    expect(button.attributes("disabled")).toBeDefined();
    pending.resolve(report(true));
    await flushPromises();
    expect(button.attributes("disabled")).toBeUndefined();
  });

  it("shows all rebuild statistics and the no-content-change explanation", async () => {
    vi.mocked(knowledgeRebuildIndex).mockResolvedValue(report(true));
    const wrapper = mount(SettingsView);
    await flushPromises();

    await wrapper.get('button[aria-label="重建知识索引"]').trigger("click");
    await flushPromises();

    expect(wrapper.text()).toContain("只重建搜索和关联索引，不修改条目正文");
    expect(wrapper.text()).toContain("来源 2");
    expect(wrapper.text()).toContain("链接 5");
    expect(wrapper.text()).toContain("未解析 1");
    expect(wrapper.text()).toContain("搜索索引可用");
  });

  it("reports when the search index is unavailable", async () => {
    vi.mocked(knowledgeRebuildIndex).mockResolvedValue(report(false));
    const wrapper = mount(SettingsView);
    await flushPromises();

    await wrapper.get('button[aria-label="重建知识索引"]').trigger("click");
    await flushPromises();

    expect(wrapper.text()).toContain("搜索索引不可用");
  });

  it("shows rebuild failures as an alert", async () => {
    vi.mocked(knowledgeRebuildIndex).mockRejectedValue(new Error("索引失败"));
    const wrapper = mount(SettingsView);
    await flushPromises();

    await wrapper.get('button[aria-label="重建知识索引"]').trigger("click");
    await flushPromises();

    expect(wrapper.get('[role="alert"]').text()).toBe("索引失败");
  });
});

const baseSettings: AppSettings = {
  dataDir: "data",
  logDir: "logs",
  backupDir: "backups",
  shortcut: "Ctrl+Alt+N",
  shortcutRegistered: true,
  shortcutError: null,
  autostartEnabled: false,
};

function report(searchIndexAvailable: boolean): KnowledgeIndexReport {
  return {
    indexedSources: 2,
    linkOccurrences: 5,
    unresolvedOccurrences: 1,
    searchIndexAvailable,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}
