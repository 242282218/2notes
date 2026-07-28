import { config, flushPromises, mount } from "@vue/test-utils";
import { defineComponent, h } from "vue";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { entriesUpdate } from "../../services/entryApi";
import {
  knowledgeDemote,
  knowledgePromote,
  knowledgeSuggest,
} from "../../services/knowledgeApi";
import type {
  BlockDocument,
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

const BlockEditorStub = defineComponent({
  name: "BlockEditor",
  props: {
    modelValue: { type: Object, required: true },
    disabled: Boolean,
    wikiSuggestionsOpen: Boolean,
    wikiListboxId: { type: String, default: undefined },
    wikiActiveDescendant: { type: String, default: undefined },
  },
  emits: [
    "update:modelValue",
    "selectionChange",
    "editorKeydown",
    "editorBlur",
  ],
  setup(props, { emit, expose }) {
    expose({
      completeWikiLink: (
        completion: { start: number; query: string },
        title: string,
      ) => {
        const block = props.modelValue.blocks[0];
        const text =
          block?.content
            .filter((node: { type: string }) => node.type === "text")
            .map((node: { text: string }) => node.text)
            .join("") ?? "";
        const value = `${text.slice(0, completion.start - 2)}[[${title}]]${text.slice(
          completion.start + completion.query.length,
        )}`;
        emit("update:modelValue", documentWithText(value));
        emit("selectionChange", {
          text: value,
          caret: completion.start + title.length + 4,
        });
        return true;
      },
    });
    return () =>
      h("div", {
        contenteditable: !props.disabled,
        role: "combobox",
        "aria-activedescendant": props.wikiSuggestionsOpen
          ? props.wikiActiveDescendant
          : undefined,
        "aria-autocomplete": props.wikiSuggestionsOpen ? "list" : undefined,
        "aria-controls": props.wikiSuggestionsOpen
          ? props.wikiListboxId
          : undefined,
        "aria-expanded": props.wikiSuggestionsOpen ? "true" : "false",
        "aria-label": "正文",
        onKeydown: (event: KeyboardEvent) => emit("editorKeydown", event),
        onBlur: () => emit("editorBlur"),
      });
  },
});

config.global.stubs.BlockEditor = BlockEditorStub;

describe("EntryDetail", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.mocked(entriesUpdate).mockReset();
    vi.mocked(knowledgePromote).mockReset();
    vi.mocked(knowledgeDemote).mockReset();
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

    await setEditor(wrapper, "updated content");
    await vi.advanceTimersByTimeAsync(500);
    await flushPromises();

    expect(entriesUpdate).toHaveBeenCalledWith(
      detail.id,
      expect.objectContaining({
        title: null,
        document: documentContainingText("updated content"),
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

    expect(
      wrapper.get('input[placeholder="标题"]').attributes("aria-label"),
    ).toBe("标题");
    expect(contentEditor(wrapper).attributes("aria-label")).toBe("正文");
    expect(contentEditor(wrapper).attributes("role")).toBe("combobox");
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
    const exposed = wrapper.vm as unknown as {
      promoteToKnowledge: () => Promise<void>;
    };
    await exposed.promoteToKnowledge();
    await flushPromises();
    expect(entriesUpdate).toHaveBeenCalled();
    expect(knowledgePromote).toHaveBeenCalledWith(detail.id, 1);
  });

  it("does not emit a stale promotion after selection changes", async () => {
    const pendingPromotion = deferred<EntryDetailType>();
    const first = entry();
    const second = { ...entry(), id: "entry-2", title: "second" };
    vi.mocked(knowledgePromote).mockReturnValue(pendingPromotion.promise);
    const wrapper = mount(EntryDetail, {
      props: { detail: first, loading: false },
    });
    await flushPromises();

    const exposed = wrapper.vm as unknown as {
      promoteToKnowledge: () => Promise<void>;
    };
    const promoting = exposed.promoteToKnowledge();
    await flushPromises();
    await wrapper.setProps({ detail: second });
    await flushPromises();
    await wrapper.setProps({ detail: first });
    await flushPromises();
    pendingPromotion.resolve({
      ...first,
      knowledgeState: "knowledge",
      revision: 1,
    });
    await promoting;

    expect(wrapper.emitted("saved")).toBeUndefined();
    expect(wrapper.emitted("entryUpdated")?.[0]).toEqual([
      expect.objectContaining({ id: first.id, revision: 1 }),
    ]);
  });

  it("uses the same stale guard for demotion ABA responses", async () => {
    const pendingDemotion = deferred<EntryDetailType>();
    const first = { ...entry(), knowledgeState: "knowledge" as const };
    const second = { ...entry(), id: "entry-2", title: "second" };
    vi.mocked(knowledgeDemote).mockReturnValue(pendingDemotion.promise);
    const wrapper = mount(EntryDetail, {
      props: { detail: first, loading: false },
    });
    await flushPromises();

    const exposed = wrapper.vm as unknown as {
      demoteFromKnowledge: () => Promise<void>;
    };
    const demoting = exposed.demoteFromKnowledge();
    await flushPromises();
    await wrapper.setProps({ detail: second });
    await flushPromises();
    await wrapper.setProps({ detail: first });
    await flushPromises();
    pendingDemotion.resolve({
      ...first,
      knowledgeState: "capture",
      revision: 1,
    });
    await demoting;

    expect(wrapper.emitted("saved")).toBeUndefined();
    expect(wrapper.emitted("entryUpdated")?.[0]).toEqual([
      expect.objectContaining({ id: first.id, revision: 1 }),
    ]);
  });

  it.each([
    ["promotion", "capture", knowledgePromote, "promoteToKnowledge"],
    ["demotion", "knowledge", knowledgeDemote, "demoteFromKnowledge"],
  ] as const)(
    "discards stale %s when only the selection token changes",
    async (_, knowledgeState, api, action) => {
      const pending = deferred<EntryDetailType>();
      const current = { ...entry(), knowledgeState };
      vi.mocked(api).mockReturnValue(pending.promise);
      const wrapper = mount(EntryDetail, {
        props: {
          detail: current,
          loading: false,
          selectionGeneration: 1,
        },
      });
      await flushPromises();

      const exposed = wrapper.vm as unknown as Record<
        typeof action,
        () => Promise<void>
      >;
      const operation = exposed[action]();
      await flushPromises();
      await wrapper.setProps({ selectionGeneration: 2 });
      pending.resolve({
        ...current,
        knowledgeState: knowledgeState === "capture" ? "knowledge" : "capture",
        revision: 1,
      });
      await operation;

      expect(wrapper.emitted("saved")).toBeUndefined();
      expect(wrapper.emitted("entryUpdated")?.[0]).toEqual([
        expect.objectContaining({ id: current.id, revision: 1 }),
      ]);
    },
  );

  it("advances expected revision across continuous edits", async () => {
    const detail = entry();
    const firstSave = deferred<EntryDetailType>();
    vi.mocked(entriesUpdate)
      .mockImplementationOnce(() => firstSave.promise)
      .mockResolvedValueOnce({
        ...detail,
        currentContent: "second draft",
        revision: 2,
      });

    const wrapper = mount(EntryDetail, {
      props: { detail, loading: false },
    });
    await flushPromises();

    await setEditor(wrapper, "first draft");
    await vi.advanceTimersByTimeAsync(500);
    await flushPromises();
    expect(entriesUpdate).toHaveBeenCalledTimes(1);
    expect(entriesUpdate).toHaveBeenLastCalledWith(
      detail.id,
      expect.objectContaining({
        document: documentContainingText("first draft"),
      }),
      0,
    );

    await setEditor(wrapper, "second draft");
    firstSave.resolve({
      ...detail,
      currentContent: "first draft",
      revision: 1,
    });
    await flushPromises();
    await vi.advanceTimersByTimeAsync(500);
    await flushPromises();

    expect(entriesUpdate).toHaveBeenCalledTimes(2);
    expect(entriesUpdate).toHaveBeenLastCalledWith(
      detail.id,
      expect.objectContaining({
        document: documentContainingText("second draft"),
      }),
      1,
    );
    expect(editorText(wrapper)).toBe("second draft");
  });

  it("keeps local fields when a newer same-id detail arrives while dirty", async () => {
    const detail = entry();
    const pendingSave = deferred<EntryDetailType>();
    vi.mocked(entriesUpdate).mockReturnValue(pendingSave.promise);
    const wrapper = mount(EntryDetail, {
      props: { detail, loading: false },
    });
    await flushPromises();

    await setEditor(wrapper, "local dirty content");
    await wrapper.get('input[placeholder="标题"]').setValue("local title");
    await vi.advanceTimersByTimeAsync(500);
    await flushPromises();

    await wrapper.setProps({
      detail: {
        ...detail,
        title: "server title",
        currentContent: "server content",
        revision: 5,
        tags: [
          {
            id: "tag-server",
            name: "server-tag",
            normalizedName: "server-tag",
            createdAt: "2026-07-15T00:00:00Z",
            entryCount: 1,
          },
        ],
      },
    });
    await flushPromises();

    expect(editorText(wrapper)).toBe("local dirty content");
    expect(
      (wrapper.get('input[placeholder="标题"]').element as HTMLInputElement)
        .value,
    ).toBe("local title");
    expect(wrapper.text()).not.toContain("server-tag");

    pendingSave.resolve({
      ...detail,
      title: "local title",
      titleSource: "user",
      currentContent: "local dirty content",
      revision: 1,
    });
    await flushPromises();

    // After the in-flight save settles, same-id prop replacement still must not
    // clobber the local editor with the external server snapshot.
    expect(editorText(wrapper)).toBe("local dirty content");
    expect(
      (wrapper.get('input[placeholder="标题"]').element as HTMLInputElement)
        .value,
    ).toBe("local title");
    expect(wrapper.text()).not.toContain("server-tag");
  });

  it("keeps local fields for same-id replacement while dirty before save starts", async () => {
    const detail = entry();
    const wrapper = mount(EntryDetail, {
      props: { detail, loading: false },
    });
    await flushPromises();

    await setEditor(wrapper, "unsaved local body");
    await wrapper.get('input[placeholder="标题"]').setValue("unsaved title");

    await wrapper.setProps({
      detail: {
        ...detail,
        title: "remote title",
        currentContent: "remote body",
        revision: 4,
      },
    });
    await flushPromises();

    expect(editorText(wrapper)).toBe("unsaved local body");
    expect(
      (wrapper.get('input[placeholder="标题"]').element as HTMLInputElement)
        .value,
    ).toBe("unsaved title");
  });

  it("fully hydrates a newer same-id detail when local state is clean", async () => {
    const detail = entry();
    const wrapper = mount(EntryDetail, {
      props: { detail, loading: false },
    });
    await flushPromises();

    await wrapper.setProps({
      detail: {
        ...detail,
        title: "external title",
        currentContent: "external content",
        document: documentWithText("external content"),
        entryType: "idea",
        status: "done",
        revision: 3,
        tags: [
          {
            id: "tag-ext",
            name: "external",
            normalizedName: "external",
            createdAt: "2026-07-15T00:00:00Z",
            entryCount: 1,
          },
        ],
      },
    });
    await flushPromises();

    expect(
      (wrapper.get('input[placeholder="标题"]').element as HTMLInputElement)
        .value,
    ).toBe("external title");
    expect(editorText(wrapper)).toBe("external content");
    expect(
      (wrapper.get("select.entry-type-select").element as HTMLSelectElement)
        .value,
    ).toBe("idea");
    expect(
      (wrapper.get("select.entry-status-select").element as HTMLSelectElement)
        .value,
    ).toBe("done");
    expect(wrapper.text()).toContain("external");
  });

  it("does not reset local fields when save success replaces same-id detail", async () => {
    const detail = entry();
    vi.mocked(entriesUpdate).mockResolvedValue({
      ...detail,
      currentContent: "saved content",
      revision: 1,
    });
    const wrapper = mount(EntryDetail, {
      props: { detail, loading: false },
    });
    await flushPromises();

    await setEditor(wrapper, "saved content");
    await vi.advanceTimersByTimeAsync(500);
    await flushPromises();

    await wrapper.setProps({
      detail: {
        ...detail,
        currentContent: "saved content",
        revision: 1,
      },
    });
    await flushPromises();

    expect(editorText(wrapper)).toBe("saved content");
    await setEditor(wrapper, "follow-up edit");
    await vi.advanceTimersByTimeAsync(500);
    await flushPromises();

    expect(entriesUpdate).toHaveBeenLastCalledWith(
      detail.id,
      expect.objectContaining({
        document: documentContainingText("follow-up edit"),
      }),
      1,
    );
  });

  it("cancels pending autosave when unmounted", async () => {
    const wrapper = mount(EntryDetail, {
      props: { detail: entry(), loading: false },
    });
    await flushPromises();
    await setEditor(wrapper, "dirty content");

    wrapper.unmount();
    await vi.advanceTimersByTimeAsync(500);

    expect(entriesUpdate).not.toHaveBeenCalled();
  });

  it("disables promotion for an empty title", async () => {
    const wrapper = mount(EntryDetail, {
      props: { detail: { ...entry(), title: null }, loading: false },
    });
    await flushPromises();
    const events = wrapper.emitted("toolbarChange") ?? [];
    expect(events[events.length - 1]?.[0]).toMatchObject({
      showPromote: true,
      canPromote: false,
    });
  });

  it("hides promotion outside an active capture entry", async () => {
    const knowledgeWrapper = mount(EntryDetail, {
      props: {
        detail: { ...entry(), knowledgeState: "knowledge" },
        loading: false,
      },
    });
    await flushPromises();
    const knowledgeEvents = knowledgeWrapper.emitted("toolbarChange") ?? [];
    expect(knowledgeEvents[knowledgeEvents.length - 1]?.[0]).toMatchObject({
      showPromote: false,
    });

    const deletedWrapper = mount(EntryDetail, {
      props: {
        detail: { ...entry(), deletedAt: "2026-07-25T00:00:00Z" },
        loading: false,
      },
    });
    await flushPromises();
    const deletedEvents = deletedWrapper.emitted("toolbarChange") ?? [];
    expect(deletedEvents[deletedEvents.length - 1]?.[0]).toMatchObject({
      showPromote: false,
    });
  });

  it("reports toolbar state without rendering its own header or fixed save state", async () => {
    const wrapper = mount(EntryDetail, {
      props: { detail: entry(), loading: false },
    });
    await flushPromises();

    expect(wrapper.find("header").exists()).toBe(false);
    const article = wrapper.get("article");
    expect(article.classes()).toContain("elevation-panel");
    expect(article.classes()).not.toContain("border");
    expect(wrapper.find("[data-testid='save-state']").exists()).toBe(false);
    expect(wrapper.html()).not.toContain("fixed bottom-4 right-4");
    const events = wrapper.emitted("toolbarChange") ?? [];
    expect(events[events.length - 1]?.[0]).toMatchObject({
      canPromote: true,
      canDemote: false,
      deleted: false,
    });
  });

  it("exposes loading as a busy status region", () => {
    const wrapper = mount(EntryDetail, {
      props: { detail: null, loading: true },
    });

    const status = wrapper.get('[role="status"]');
    expect(status.attributes("aria-busy")).toBe("true");
    expect(status.text()).toContain("加载中");
  });

  it("keeps wiki combobox collapsed without controls when suggestions are closed", async () => {
    const wrapper = mount(EntryDetail, {
      props: { detail: entry(), loading: false },
    });
    await flushPromises();

    const editor = contentEditor(wrapper);
    expect(editor.attributes("role")).toBe("combobox");
    expect(editor.attributes("aria-expanded")).toBe("false");
    expect(editor.attributes("aria-controls")).toBeUndefined();
    expect(editor.attributes("aria-activedescendant")).toBeUndefined();
    expect(wrapper.find('[role="listbox"]').exists()).toBe(false);
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
    const editor = contentEditor(wrapper);
    const listbox = wrapper.get('[role="listbox"]');

    expect(listbox.attributes("id")).toBe("entry-wiki-link-suggestions");
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
    expect(editor.attributes("aria-expanded")).toBe("false");
    expect(editor.attributes("aria-controls")).toBeUndefined();
    expect(editor.attributes("aria-activedescendant")).toBeUndefined();

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
    const editor = contentEditor(wrapper);
    await editor.trigger("blur");
    await flushPromises();

    expect(wrapper.find('[role="listbox"]').exists()).toBe(false);
    await editor.trigger("keydown", { key: "Enter" });
    expect(editorText(wrapper)).toBe("[[ca");
    wrapper.unmount();
  });

  it("does not consume selection keys while an IME composition is active", async () => {
    const wrapper = mount(EntryDetail, {
      props: { detail: entry(), loading: false },
    });
    await flushPromises();
    await openSuggestions(wrapper, "[[ca", [suggestion("one", "Canonical")]);
    const enter = new KeyboardEvent("keydown", {
      key: "Enter",
      isComposing: true,
      bubbles: true,
      cancelable: true,
    });

    await emitEditorKeydown(wrapper, enter);
    await flushPromises();

    expect(enter.defaultPrevented).toBe(false);
    expect(editorText(wrapper)).toBe("[[ca");
    expect(wrapper.find('[role="listbox"]').exists()).toBe(true);
  });

  it("recomputes completion from the current text node selection", async () => {
    const wrapper = mount(EntryDetail, {
      props: { detail: entry(), loading: false },
    });
    await flushPromises();
    const editor = wrapper.findComponent({ name: "BlockEditor" });
    await editor.vm.$emit("selectionChange", {
      text: "[[first]] [[second",
      caret: "[[first]] [[second".length,
    });
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
    await wrapper.get('[role="option"]').trigger("click");
    await flushPromises();

    expect(editorText(wrapper)).toBe("Before [[Canonical]] after");

    await vi.advanceTimersByTimeAsync(500);
    await flushPromises();
    expect(entriesUpdate).toHaveBeenCalledWith(
      detail.id,
      expect.objectContaining({
        document: documentContainingText("Before [[Canonical]] after"),
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

function contentEditor(wrapper: ReturnType<typeof mount>) {
  return wrapper.get("[contenteditable]");
}

function editorDocument(wrapper: ReturnType<typeof mount>): BlockDocument {
  return wrapper
    .findComponent(EntryDetail)
    .findComponent({ name: "BlockEditor" })
    .props("modelValue") as BlockDocument;
}

function editorText(wrapper: ReturnType<typeof mount>): string {
  return editorDocument(wrapper)
    .blocks.flatMap((block) => block.content)
    .filter((node) => node.type === "text")
    .map((node) => node.text)
    .join("");
}

async function setEditor(
  wrapper: ReturnType<typeof mount>,
  value: string,
  caret = value.length,
) {
  const editor = wrapper.findComponent({ name: "BlockEditor" });
  await editor.vm.$emit("update:modelValue", documentWithText(value));
  await editor.vm.$emit("selectionChange", { text: value, caret });
}

async function emitEditorKeydown(
  wrapper: ReturnType<typeof mount>,
  event: KeyboardEvent,
) {
  await wrapper
    .findComponent({ name: "BlockEditor" })
    .vm.$emit("editorKeydown", event);
}

function documentContainingText(text: string) {
  return expect.objectContaining({
    blocks: [
      expect.objectContaining({
        kind: "paragraph",
        content: [expect.objectContaining({ type: "text", text })],
      }),
    ],
  });
}

function documentWithText(text: string): BlockDocument {
  return {
    schemaVersion: 1,
    blocks: [
      {
        id: "550e8400-e29b-41d4-a716-446655440000",
        kind: "paragraph",
        attrs: { level: null, language: null, start: null },
        content: text ? [{ type: "text", text, marks: [] }] : [],
        children: [],
      },
    ],
  };
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

function emptyDocument() {
  return {
    schemaVersion: 1,
    blocks: [
      {
        id: "550e8400-e29b-41d4-a716-446655440000",
        kind: "paragraph" as const,
        attrs: { level: null, language: null, start: null },
        content: [],
        children: [],
      },
    ],
  };
}

function entry(): EntryDetailType {
  return {
    id: "entry-1",
    title: "automatic title",
    titleSource: "auto",
    originalContent: "original",
    currentContent: "content",
    document: emptyDocument(),
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
