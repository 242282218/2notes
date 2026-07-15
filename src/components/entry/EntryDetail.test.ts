import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { entriesUpdate } from "../../services/entryApi";
import type { EntryDetail as EntryDetailType } from "../../types/generated";
import EntryDetail from "./EntryDetail.vue";

vi.mock("../../services/entryApi", () => ({
  entriesUpdate: vi.fn(),
}));

vi.mock("../../services/tagApi", () => ({
  tagsSuggest: vi.fn().mockResolvedValue([]),
}));

describe("EntryDetail", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.mocked(entriesUpdate).mockReset();
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
    tags: [],
    revision: 0,
    createdAt: "2026-07-15T00:00:00Z",
    updatedAt: "2026-07-15T00:00:00Z",
    deletedAt: null,
  };
}
