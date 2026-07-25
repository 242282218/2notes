import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { entriesUpdate } from "../../services/entryApi";
import {
  knowledgePromote,
  knowledgeSuggest,
} from "../../services/knowledgeApi";
import type {
  EntryDetail as EntryDetailType,
  KnowledgeSuggestion,
} from "../../types/generated";
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
    vi.mocked(knowledgeSuggest).mockReset().mockResolvedValue([]);
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

    await wrapper.get("textarea").setValue("updated content");
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

    expect(wrapper.get('input[placeholder="标题"]').attributes("aria-label")).toBe(
      "标题",
    );
    expect(
      wrapper.get("textarea").attributes("aria-label"),
    ).toBe("正文");
    expect(wrapper.get("textarea").attributes("role")).toBe(
      "combobox",
    );
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
    await wrapper.get('input[placeholder="标题"]').setValue("确认后的标题");
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

  it("navigates open wiki link suggestions and exposes the active option", async () => {
    const wrapper = mount(EntryDetail, {
      props: { detail: entry(), loading: false },
    });
    await flushPromises();
    await openSuggestions(wrapper, "[[ca", [
      suggestion("one", "Canonical One"),
      suggestion("two", "Canonical Two"),
    ]);
    const editor = wrapper.get("textarea");

    expect(editor.attributes()).toMatchObject({
      "aria-autocomplete": "list",
      "aria-expanded": "true",
      "aria-controls": "entry-wiki-link-suggestions",
      "aria-activedescendant": "entry-wiki-link-suggestions-option-0",
    });

    await editor.trigger("keydown", { key: "ArrowDown" });
    expect(editor.attributes("aria-activedescendant")).toBe(
      "entry-wiki-link-suggestions-option-1",
    );
    expect(
      wrapper.findAll('[role="option"]')[1].attributes("aria-selected"),
    ).toBe("true");

    await editor.trigger("keydown", { key: "ArrowUp" });
    expect(editor.attributes("aria-activedescendant")).toBe(
      "entry-wiki-link-suggestions-option-0",
    );

    await editor.trigger("keydown", { key: "Escape" });
    expect(wrapper.find('[role="listbox"]').exists()).toBe(false);
    expect(editor.attributes("aria-expanded")).toBeUndefined();

    const closedArrow = new KeyboardEvent("keydown", {
      key: "ArrowDown",
      bubbles: true,
      cancelable: true,
    });
    editor.element.dispatchEvent(closedArrow);
    expect(closedArrow.defaultPrevented).toBe(false);
  });

  it("closes and invalidates wiki link suggestions when the editor blurs", async () => {
    const wrapper = mount(EntryDetail, {
      props: { detail: entry(), loading: false },
      attachTo: document.body,
    });
    await flushPromises();
    await openSuggestions(wrapper, "[[ca", [suggestion("one", "Canonical")]);
    const editor = wrapper.get("textarea");
    const title = wrapper.get('input[placeholder="标题"]');
    (editor.element as HTMLTextAreaElement).focus();

    (title.element as HTMLInputElement).focus();
    await flushPromises();

    expect(wrapper.find('[role="listbox"]').exists()).toBe(false);
    (editor.element as HTMLTextAreaElement).focus();
    await editor.trigger("keydown", { key: "Enter" });
    expect((editor.element as HTMLTextAreaElement).value).toBe("[[ca");
    wrapper.unmount();
  });

  it("does not consume selection keys while an IME composition is active", async () => {
    const wrapper = mount(EntryDetail, {
      props: { detail: entry(), loading: false },
    });
    await flushPromises();
    await openSuggestions(wrapper, "[[ca", [suggestion("one", "Canonical")]);
    const editor = wrapper.get("textarea");
    const enter = new KeyboardEvent("keydown", {
      key: "Enter",
      isComposing: true,
      bubbles: true,
      cancelable: true,
    });

    editor.element.dispatchEvent(enter);
    await flushPromises();

    expect(enter.defaultPrevented).toBe(false);
    expect((editor.element as HTMLTextAreaElement).value).toBe("[[ca");
    expect(wrapper.find('[role="listbox"]').exists()).toBe(true);
  });

  it("recomputes completion on arrow keyup while suggestions are closed", async () => {
    const wrapper = mount(EntryDetail, {
      props: { detail: entry(), loading: false },
    });
    await flushPromises();
    const value = "[[first]]\n[[second";
    await setEditor(wrapper, value, 4);
    const editor = wrapper.get("textarea");
    const element = editor.element as HTMLTextAreaElement;
    element.setSelectionRange(value.length, value.length);

    await editor.trigger("keydown", { key: "ArrowDown" });
    await editor.trigger("keyup", { key: "ArrowDown" });
    await vi.advanceTimersByTimeAsync(120);
    await flushPromises();

    expect(knowledgeSuggest).toHaveBeenCalledTimes(1);
    expect(knowledgeSuggest).toHaveBeenCalledWith("second");
  });

  it("discards stale wiki link suggestion responses", async () => {
    const first = deferred<KnowledgeSuggestion[]>();
    const second = deferred<KnowledgeSuggestion[]>();
    vi.mocked(knowledgeSuggest)
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const wrapper = mount(EntryDetail, {
      props: { detail: entry(), loading: false },
    });
    await flushPromises();

    await setEditor(wrapper, "[[a");
    await vi.advanceTimersByTimeAsync(120);
    await setEditor(wrapper, "[[ab");
    await vi.advanceTimersByTimeAsync(120);

    second.resolve([suggestion("new", "New Result")]);
    await flushPromises();
    first.resolve([suggestion("old", "Old Result")]);
    await flushPromises();

    expect(wrapper.text()).toContain("New Result");
    expect(wrapper.text()).not.toContain("Old Result");
  });

  it("inserts the canonical title, restores the caret, and autosaves", async () => {
    const detail = entry();
    vi.mocked(entriesUpdate).mockResolvedValue({
      ...detail,
      currentContent: "Before [[Canonical]] after",
      revision: 1,
    });
    const wrapper = mount(EntryDetail, {
      props: { detail, loading: false },
      attachTo: document.body,
    });
    await flushPromises();
    await openSuggestions(
      wrapper,
      "Before [[leg after",
      [suggestion("canonical", "Canonical", "Legacy")],
      12,
    );
    const editor = wrapper.get("textarea");

    await wrapper.get('[role="option"]').trigger("click");
    await flushPromises();

    expect((editor.element as HTMLTextAreaElement).value).toBe(
      "Before [[Canonical]] after",
    );
    expect((editor.element as HTMLTextAreaElement).selectionStart).toBe(20);
    expect(document.activeElement).toBe(editor.element);

    await vi.advanceTimersByTimeAsync(500);
    await flushPromises();
    expect(entriesUpdate).toHaveBeenCalledWith(
      detail.id,
      expect.objectContaining({
        currentContent: "Before [[Canonical]] after",
      }),
      0,
    );
  });
});

async function openSuggestions(
  wrapper: ReturnType<typeof mount>,
  value: string,
  items: KnowledgeSuggestion[],
  caret = value.length,
) {
  vi.mocked(knowledgeSuggest).mockResolvedValueOnce(items);
  await setEditor(wrapper, value, caret);
  await vi.advanceTimersByTimeAsync(120);
  await flushPromises();
}

async function setEditor(
  wrapper: ReturnType<typeof mount>,
  value: string,
  caret = value.length,
) {
  const editor = wrapper.get("textarea");
  const element = editor.element as HTMLTextAreaElement;
  element.value = value;
  element.setSelectionRange(caret, caret);
  await editor.trigger("input");
}

function suggestion(
  id: string,
  title: string,
  matchedAlias: string | null = null,
): KnowledgeSuggestion {
  return { id, title, matchedAlias };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

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
