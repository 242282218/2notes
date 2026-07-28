import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { backupsList, backupsRestore } from "../../services/backupApi";
import { exportMarkdown } from "../../services/exportApi";
import { knowledgeRebuildIndex } from "../../services/knowledgeApi";
import { settingsGet } from "../../services/settingsApi";
import { useEntriesStore } from "../../stores/entries";
import type {
  AppSettings,
  BackupInfo,
  KnowledgeIndexReport,
} from "../../types/generated";
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

vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn() }));

describe("SettingsView", () => {
  let observerCallback: IntersectionObserverCallback;
  const observe = vi.fn();
  const disconnect = vi.fn();
  const createObserver = vi.fn();

  beforeEach(() => {
    setActivePinia(createPinia());
    vi.mocked(settingsGet).mockResolvedValue(baseSettings);
    vi.mocked(backupsList).mockResolvedValue([]);
    vi.mocked(backupsRestore).mockReset();
    vi.mocked(knowledgeRebuildIndex).mockReset();
    vi.mocked(exportMarkdown).mockReset();
    observe.mockReset();
    disconnect.mockReset();
    createObserver.mockReset();
    class IntersectionObserverStub {
      observe = observe;
      disconnect = disconnect;

      constructor(callback: IntersectionObserverCallback) {
        createObserver();
        observerCallback = callback;
      }
    }
    vi.stubGlobal("IntersectionObserver", IntersectionObserverStub);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("exposes settings content as a labeled section instead of a nested main", async () => {
    const wrapper = mount(SettingsView);
    await flushPromises();

    expect(wrapper.findAll("main")).toHaveLength(0);
    expect(wrapper.find('section[aria-label="设置内容"]').exists()).toBe(true);
  });

  it("tracks the visible category and disconnects its observer", async () => {
    const wrapper = mount(SettingsView);
    await flushPromises();

    const appearanceButton = wrapper.get('button[data-category="appearance"]');
    const backupButton = wrapper.get('button[data-category="backup"]');
    expect(appearanceButton.attributes("aria-current")).toBe("location");
    expect(observe).toHaveBeenCalledTimes(6);

    observerCallback(
      [intersectionEntry(wrapper.get("#backup").element, true, 0.8)],
      {} as IntersectionObserver,
    );
    await wrapper.vm.$nextTick();

    expect(appearanceButton.attributes("aria-current")).toBeUndefined();
    expect(backupButton.attributes("aria-current")).toBe("location");

    wrapper.unmount();
    expect(disconnect).toHaveBeenCalledOnce();
  });

  it("does not create an observer after unmounting during settings load", async () => {
    const pendingSettings = deferred<AppSettings>();
    vi.mocked(settingsGet).mockReturnValue(pendingSettings.promise);
    const wrapper = mount(SettingsView);

    wrapper.unmount();
    pendingSettings.resolve(baseSettings);
    await flushPromises();

    expect(createObserver).not.toHaveBeenCalled();
    expect(observe).not.toHaveBeenCalled();
  });

  it("activates and scrolls to a category when its navigation button is clicked", async () => {
    vi.stubGlobal("IntersectionObserver", undefined);
    const scrollIntoView = vi.fn();
    Element.prototype.scrollIntoView = scrollIntoView;
    const wrapper = mount(SettingsView);
    await flushPromises();

    const exportButton = wrapper.get('button[data-category="export"]');
    await exportButton.trigger("click");

    expect(exportButton.attributes("aria-current")).toBe("location");
    expect(scrollIntoView).toHaveBeenCalledWith({
      behavior: "smooth",
      block: "start",
    });
  });

  it("keeps the clicked category active while the scroll settles", async () => {
    vi.useFakeTimers();
    try {
      const wrapper = mount(SettingsView);
      await flushPromises();

      const exportButton = wrapper.get('button[data-category="export"]');
      const appearanceButton = wrapper.get(
        'button[data-category="appearance"]',
      );
      await exportButton.trigger("click");

      observerCallback(
        [intersectionEntry(wrapper.get("#appearance").element, true, 0.9)],
        {} as IntersectionObserver,
      );
      await wrapper.vm.$nextTick();

      expect(exportButton.attributes("aria-current")).toBe("location");
      expect(appearanceButton.attributes("aria-current")).toBeUndefined();

      await vi.advanceTimersByTimeAsync(300);
      observerCallback(
        [intersectionEntry(wrapper.get("#appearance").element, true, 0.9)],
        {} as IntersectionObserver,
      );
      await wrapper.vm.$nextTick();

      expect(appearanceButton.attributes("aria-current")).toBe("location");
      expect(exportButton.attributes("aria-current")).toBeUndefined();
    } finally {
      vi.useRealTimers();
    }
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

  it("exports without a frontend target directory argument", async () => {
    vi.mocked(exportMarkdown).mockResolvedValue({
      exportedCount: 3,
      targetDir: "C:/exports",
    });
    const wrapper = mount(SettingsView);
    await flushPromises();

    const exportButtons = wrapper
      .findAll("button")
      .filter((button) => button.text().includes("导出"));
    const action = exportButtons[exportButtons.length - 1];
    await action.trigger("click");
    await flushPromises();

    expect(exportMarkdown).toHaveBeenCalledWith();
    expect(exportMarkdown).toHaveBeenCalledTimes(1);
    expect(wrapper.text()).toContain("已导出 3 个文件");
  });

  it("treats a cancelled export as a silent no-op", async () => {
    vi.mocked(exportMarkdown).mockResolvedValue(null);
    const wrapper = mount(SettingsView);
    await flushPromises();

    const exportButtons = wrapper
      .findAll("button")
      .filter((button) => button.text().includes("导出"));
    await exportButtons[exportButtons.length - 1].trigger("click");
    await flushPromises();

    expect(exportMarkdown).toHaveBeenCalledWith();
    expect(wrapper.text()).not.toContain("已导出");
    expect(
      wrapper
        .findAll('[role="alert"]')
        .every((node) => !node.text().includes("导出失败")),
    ).toBe(true);
  });

  it("keeps restored data invalidated when interface refresh fails", async () => {
    vi.mocked(backupsList).mockResolvedValue([backup]);
    vi.mocked(backupsRestore).mockResolvedValue(backup);
    const entriesStore = useEntriesStore();
    const load = vi.spyOn(entriesStore, "load").mockImplementation(async () => {
      entriesStore.error = "加载失败";
    });
    const refreshTags = vi
      .spyOn(entriesStore, "refreshTags")
      .mockResolvedValue(undefined);
    const noteExternalChange = vi.spyOn(entriesStore, "noteExternalChange");
    const wrapper = mount(SettingsView);
    try {
      await flushPromises();

      await wrapper
        .findAll("button")
        .find((button) => button.text() === "恢复")!
        .trigger("click");
      const modalActions = document.body.querySelector(".modal-actions");
      expect(modalActions).not.toBeNull();
      const confirmButton = Array.from(
        modalActions!.querySelectorAll<HTMLButtonElement>("button"),
      ).find((button) => button.textContent?.trim() === "确认恢复");
      expect(confirmButton).toBeDefined();
      confirmButton?.click();
      await flushPromises();

      expect(backupsRestore).toHaveBeenCalledWith(backup.path);
      expect(noteExternalChange).toHaveBeenCalledOnce();
      expect(load).toHaveBeenCalledOnce();
      expect(refreshTags).toHaveBeenCalledOnce();
      expect(wrapper.text()).toContain(`已恢复备份：${backup.fileName}`);
      expect(wrapper.text()).toContain("恢复成功，但界面刷新失败");
    } finally {
      wrapper.unmount();
      document.body.replaceChildren();
    }
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
  themeMode: "system",
};

const backup: BackupInfo = {
  path: "backups/backup.sqlite",
  fileName: "backup.sqlite",
  kind: "manual",
  createdAt: "2026-07-16T00:00:00Z",
  sizeBytes: 1024,
};

function report(searchIndexAvailable: boolean): KnowledgeIndexReport {
  return {
    indexedSources: 2,
    linkOccurrences: 5,
    unresolvedOccurrences: 1,
    searchIndexAvailable,
    projectedBlocks: 17,
    repairedDocuments: 1,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

function intersectionEntry(
  target: Element,
  isIntersecting: boolean,
  intersectionRatio: number,
): IntersectionObserverEntry {
  return {
    target,
    isIntersecting,
    intersectionRatio,
  } as IntersectionObserverEntry;
}
