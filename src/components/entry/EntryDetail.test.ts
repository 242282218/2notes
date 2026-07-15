import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { entriesUpdate } from "../../services/entryApi";
import { knowledgePromote } from "../../services/knowledgeApi";
import type { EntryDetail as EntryDetailType } from "../../types/generated";
import EntryDetail from "./EntryDetail.vue";

vi.mock("../../services/entryApi", () => ({
  entriesUpdate: vi.fn(),
}));

vi.mock("../../services/knowledgeApi", () => ({
  knowledgePromote: vi.fn(),
  knowledgeDemote: vi.fn(),
  knowledgeRelationsGet: vi.fn(),
  knowledgeSuggest: vi.fn().mockResolvedValue([]),
}));

vi.mock("../../services/tagApi", () => ({
  tagsSuggest: vi.fn().mockResolvedValue([]),
}));

describe("EntryDetail", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.mocked(entriesUpdate).mockReset();
    vi.mocked(knowledgePromote).mockReset();
  });

  it("does not convert an untouched automatic title into a user title", async () => {
    const detail = entry();
    vi.mocked(entriesUpdate).mockResolvedValue({
      ...detail,
      currentContent: "updated content",
      revision: 1,
    });
    const wrapper = mount(EntryDetail, {
      props: {
        detail,
        loading: false,
      },
    });
    await flushPromises();

    await wrapper.get("textarea.content-editor").setValue("updated content");
    await vi.advanceTimersByTimeAsync(500);
    await flushPromises();

    expect(entriesUpdate).toHaveBeenCalledWith(
      detail.id,
      expect.objectContaining({
        title: null,
        currentContent: "updated content",
      }),
      0,
    );
  });

  it("commits an unfinished tag draft before flushing for quit", async () => {
    const detail = entry();
    vi.mocked(entriesUpdate).mockResolvedValue({
      ...detail,
      tags: [
        {
          id: "tag-1",
          name: "work",
          normalizedName: "work",
          createdAt: "2026-07-15T00:00:00Z",
          entryCount: 1,
        },
      ],
      revision: 1,
    });
    const wrapper = mount(EntryDetail, {
      props: {
        detail,
        loading: false,
      },
    });
    await flushPromises();
    await wrapper.get('input[aria-label="标签"]').setValue("work");

    const exposed = wrapper.vm as unknown as {
      flushPendingSave: () => Promise<boolean>;
    };
    await exposed.flushPendingSave();

    expect(entriesUpdate).toHaveBeenCalledWith(
      detail.id,
      expect.objectContaining({ tags: ["work"] }),
      0,
    );
  });

  it("gives the core editor controls accessible names", async () => {
    const wrapper = mount(EntryDetail, {
      props: {
        detail: entry(),
        loading: false,
      },
    });
    await flushPromises();

    expect(wrapper.get("input.title-input").attributes("aria-label")).toBe(
      "标题",
    );
    expect(
      wrapper.get("textarea.content-editor").attributes("aria-label"),
    ).toBe("正文");
    expect(
      wrapper.get("select.entry-type-select").attributes("aria-label"),
    ).toBe("类型");
    expect(
      wrapper.get("select.entry-status-select").attributes("aria-label"),
    ).toBe("状态");
  });

  it("flushes edits before promoting the same entry", async () => {
    const detail = entry();
    vi.mocked(entriesUpdate).mockResolvedValue({
      ...detail,
      title: "确认后的标题",
      titleSource: "user",
      revision: 1,
    });
    vi.mocked(knowledgePromote).mockResolvedValue({
      ...detail,
      title: "确认后的标题",
      titleSource: "user",
      knowledgeState: "knowledge",
      knowledgePromotedAt: "2026-07-15T00:00:00Z",
      revision: 2,
    });
    const wrapper = mount(EntryDetail, { props: { detail, loading: false } });
    await flushPromises();
    await wrapper.get("input.title-input").setValue("确认后的标题");
    await wrapper.get('button[aria-label="沉淀为知识"]').trigger("click");
    await flushPromises();
    expect(entriesUpdate).toHaveBeenCalled();
    expect(knowledgePromote).toHaveBeenCalledWith(detail.id, 1);
  });

  it("disables promotion for an empty title", async () => {
    const wrapper = mount(EntryDetail, {
      props: { detail: { ...entry(), title: null }, loading: false },
    });
    await flushPromises();
    expect(
      wrapper.get('button[aria-label="沉淀为知识"]').attributes(),
    ).toHaveProperty("disabled");
  });
});

function entry(): EntryDetailType {
  return {
    id: "entry-1",
    title: "automatic title",
    titleSource: "auto",
    originalContent: "original",
    currentContent: "content",
    entryType: "unclear",
    status: "pending",
    knowledgeState: "capture",
    knowledgePromotedAt: null,
    knowledgeAliases: [],
    tags: [],
    revision: 0,
    createdAt: "2026-07-15T00:00:00Z",
    updatedAt: "2026-07-15T00:00:00Z",
    deletedAt: null,
  };
}
