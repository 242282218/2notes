import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  markdownImportCommit,
  markdownImportPreview,
} from "../../services/importApi";
import { useEntriesStore } from "../../stores/entries";
import MarkdownImportPanel from "./MarkdownImportPanel.vue";
import ConfirmDialog from "../shared/ConfirmDialog.vue";

vi.mock("../../services/importApi", () => ({
  markdownImportPreview: vi.fn(),
  markdownImportCommit: vi.fn(),
}));

vi.mock("../../stores/entries", () => ({
  useEntriesStore: vi.fn(),
}));

describe("MarkdownImportPanel", () => {
  const entriesStore = {
    load: vi.fn(),
    refreshTags: vi.fn(),
    noteExternalChange: vi.fn(),
  };

  beforeEach(() => {
    vi.mocked(useEntriesStore).mockReturnValue(entriesStore as never);
    vi.mocked(markdownImportPreview).mockReset();
    vi.mocked(markdownImportCommit).mockReset();
    entriesStore.load.mockReset();
    entriesStore.refreshTags.mockReset();
    entriesStore.noteExternalChange.mockReset();
    entriesStore.load.mockResolvedValue(undefined);
    entriesStore.refreshTags.mockResolvedValue(undefined);
  });

  it("previews the selected files and states that import only creates new entries", async () => {
    vi.mocked(markdownImportPreview).mockResolvedValue(preview());
    const wrapper = mount(MarkdownImportPanel);

    await wrapper
      .get('button[aria-label="选择 Markdown 目录导入"]')
      .trigger("click");
    await flushPromises();

    expect(markdownImportPreview).toHaveBeenCalledWith();
    expect(wrapper.text()).toContain("将创建新条目，不会覆盖现有内容");
    expect(wrapper.text()).toContain("2 个文件");
    expect(wrapper.text()).toContain("3 KB");
    expect(wrapper.text()).toContain("notes/one.md");
    expect(wrapper.text()).toContain("忽略了 1 个空文件");
  });

  it("clears a consumed preview after commit fails", async () => {
    vi.mocked(markdownImportPreview).mockResolvedValue(preview());
    vi.mocked(markdownImportCommit).mockRejectedValue(new Error("会话已失效"));
    const wrapper = mount(MarkdownImportPanel, { attachTo: document.body });

    await wrapper
      .get('button[aria-label="选择 Markdown 目录导入"]')
      .trigger("click");
    await flushPromises();
    await wrapper.findComponent(ConfirmDialog).vm.$emit("confirm");
    await flushPromises();

    expect(wrapper.text()).toContain("会话已失效");
    expect(wrapper.text()).not.toContain("已选择 2 个文件");
    expect(wrapper.findComponent(ConfirmDialog).props("open")).toBe(false);
    expect(markdownImportCommit).toHaveBeenCalledOnce();

    wrapper.unmount();
    document.body.replaceChildren();
  });

  it("commits a preview once and refreshes entries after success", async () => {
    vi.mocked(markdownImportPreview).mockResolvedValue(preview());
    vi.mocked(markdownImportCommit).mockResolvedValue({
      importedCount: 2,
      skippedCount: 1,
      failedCount: 0,
      failures: [],
    });
    const wrapper = mount(MarkdownImportPanel, { attachTo: document.body });

    await wrapper
      .get('button[aria-label="选择 Markdown 目录导入"]')
      .trigger("click");
    await flushPromises();
    await wrapper.findComponent(ConfirmDialog).vm.$emit("confirm");
    await wrapper.findComponent(ConfirmDialog).vm.$emit("confirm");
    await flushPromises();
    await vi.waitFor(() => {
      expect(wrapper.text()).toContain("导入完成：成功 2，跳过 1，失败 0");
    });

    expect(markdownImportCommit).toHaveBeenCalledTimes(1);
    expect(markdownImportCommit).toHaveBeenCalledWith("session-1");
    expect(entriesStore.load).toHaveBeenCalledOnce();
    expect(entriesStore.refreshTags).toHaveBeenCalledOnce();
    expect(entriesStore.noteExternalChange).toHaveBeenCalledOnce();
    expect(wrapper.text()).toContain("导入完成：成功 2，跳过 1，失败 0");

    wrapper.unmount();
    document.body.replaceChildren();
  });
});

function preview() {
  return {
    sessionId: "session-1",
    fileCount: 2,
    totalBytes: 3 * 1024,
    paths: ["notes/one.md", "notes/two.md"],
    warnings: ["忽略了 1 个空文件"],
  };
}
