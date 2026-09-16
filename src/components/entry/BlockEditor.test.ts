import { flushPromises, mount } from "@vue/test-utils";
import { nextTick } from "vue";
import { describe, expect, it } from "vitest";

import type { BlockDocument } from "../../types/generated";
import { BLOCK_ID_ATTR, fromTiptapDocument } from "../../editor/tiptapAdapter";
import BlockEditor from "./BlockEditor.vue";

const PARAGRAPH_ID = "550e8400-e29b-41d4-a716-446655440000";
const HEADING_ID = "6ba7b810-9dad-41d4-80b4-00c04fd430c8";
const LIST_ID = "6ba7b811-9dad-41d4-80b4-00c04fd430c8";
const ITEM_ID = "6ba7b812-9dad-41d4-80b4-00c04fd430c8";

type BlockEditorExposed = {
  completeWikiLink: (
    completion: { start: number; query: string },
    title: string,
  ) => boolean;
  getEditor: () => {
    commands: { setTextSelection: (position: number) => boolean };
    state: { selection: { from: number } };
  } | null;
  getEditorJson: () => Record<string, unknown> | null;
  setSnapshot: (snapshot: BlockDocument) => void;
  focusBlock: (id: string) => boolean;
};

function paragraphDocument(text: string): BlockDocument {
  return {
    schemaVersion: 1,
    blocks: [
      {
        id: PARAGRAPH_ID,
        kind: "paragraph",
        attrs: { level: null, language: null, start: null },
        content: text ? [{ type: "text", text, marks: [] }] : [],
        children: [],
      },
    ],
  };
}

function fixtureDocument(): BlockDocument {
  return {
    schemaVersion: 1,
    blocks: [
      {
        id: HEADING_ID,
        kind: "heading",
        attrs: { level: 2, language: null, start: null },
        content: [{ type: "text", text: "标题", marks: [] }],
        children: [],
      },
      {
        id: PARAGRAPH_ID,
        kind: "paragraph",
        attrs: { level: null, language: null, start: null },
        content: [
          { type: "text", text: "第一行", marks: [{ type: "bold" }] },
          { type: "hardBreak" },
          { type: "text", text: "第二行", marks: [] },
        ],
        children: [],
      },
      {
        id: LIST_ID,
        kind: "bulletList",
        attrs: { level: null, language: null, start: null },
        content: [],
        children: [
          {
            id: ITEM_ID,
            kind: "listItem",
            attrs: { level: null, language: null, start: null },
            content: [{ type: "text", text: "列表项", marks: [] }],
            children: [],
          },
        ],
      },
    ],
  };
}

describe("BlockEditor", () => {
  it("keeps domain block ids through the real Tiptap schema", async () => {
    const document = fixtureDocument();
    const wrapper = mount(BlockEditor, { props: { modelValue: document } });
    await flushPromises();

    const exposed = wrapper.vm as unknown as BlockEditorExposed;
    const json = exposed.getEditorJson();
    expect(json).not.toBeNull();

    const content = json?.content as Array<{ attrs?: Record<string, unknown> }>;
    expect(content[0].attrs?.[BLOCK_ID_ATTR]).toBe(HEADING_ID);
    expect(content[1].attrs?.[BLOCK_ID_ATTR]).toBe(PARAGRAPH_ID);
    expect(content[2].attrs?.[BLOCK_ID_ATTR]).toBe(LIST_ID);
    expect(
      (content[2] as { content: Array<{ attrs?: Record<string, unknown> }> })
        .content[0].attrs?.[BLOCK_ID_ATTR],
    ).toBe(ITEM_ID);
    expect(
      fromTiptapDocument(json as Parameters<typeof fromTiptapDocument>[0]),
    ).toEqual(document);

    wrapper.unmount();
  });

  it("hydrates external snapshots without emitting an update", async () => {
    const wrapper = mount(BlockEditor, {
      props: { modelValue: fixtureDocument() },
    });
    await flushPromises();

    const updateCountBeforeHydration =
      wrapper.emitted("update:modelValue")?.length ?? 0;
    const nextDocument = fixtureDocument();
    nextDocument.blocks[0] = {
      ...nextDocument.blocks[0],
      content: [{ type: "text", text: "外部更新", marks: [] }],
    };
    await wrapper.setProps({ modelValue: nextDocument });
    await nextTick();

    expect(wrapper.emitted("update:modelValue")?.length ?? 0).toBe(
      updateCountBeforeHydration,
    );
    const exposed = wrapper.vm as unknown as BlockEditorExposed;
    const snapshot = fromTiptapDocument(
      exposed.getEditorJson() as Parameters<typeof fromTiptapDocument>[0],
    );
    expect(snapshot).toEqual(nextDocument);

    wrapper.unmount();
  });

  it("hydrates an empty blockquote snapshot into an editable empty paragraph", async () => {
    const wrapper = mount(BlockEditor, {
      props: { modelValue: paragraphDocument("previous content") },
    });
    await flushPromises();

    const emptyQuote: BlockDocument = {
      schemaVersion: 1,
      blocks: [
        {
          id: HEADING_ID,
          kind: "blockquote",
          attrs: { level: null, language: null, start: null },
          content: [],
          children: [],
        },
      ],
    };
    await wrapper.setProps({ modelValue: emptyQuote });
    await nextTick();

    const exposed = wrapper.vm as unknown as BlockEditorExposed;
    const snapshot = fromTiptapDocument(
      exposed.getEditorJson() as Parameters<typeof fromTiptapDocument>[0],
    );
    expect(snapshot.blocks[0].kind).toBe("blockquote");
    expect(snapshot.blocks[0].children).toHaveLength(1);
    expect(snapshot.blocks[0].children[0].kind).toBe("paragraph");
    expect(snapshot.blocks[0].children[0].content).toEqual([]);

    wrapper.unmount();
  });

  it("does not rehydrate after its own model update is echoed back", async () => {
    const wrapper = mount(BlockEditor, {
      props: { modelValue: fixtureDocument() },
    });
    await flushPromises();

    const contenteditable = wrapper.find("[contenteditable='true']");
    contenteditable.element.textContent = "本地编辑";
    await contenteditable.trigger("input");
    await nextTick();

    const emitted = wrapper.emitted("update:modelValue");
    expect(emitted).toHaveLength(1);
    const localDocument = emitted?.[0][0] as BlockDocument;
    await wrapper.setProps({ modelValue: localDocument });
    await nextTick();

    expect(wrapper.emitted("update:modelValue")).toHaveLength(1);
    const exposed = wrapper.vm as unknown as BlockEditorExposed;
    expect(
      fromTiptapDocument(
        exposed.getEditorJson() as Parameters<typeof fromTiptapDocument>[0],
      ),
    ).toEqual(localDocument);

    wrapper.unmount();
  });

  it("replaces only the current text node wiki link query and restores the caret", async () => {
    const original = "Before [[leg after";
    const wrapper = mount(BlockEditor, {
      props: { modelValue: paragraphDocument(original) },
    });
    await flushPromises();

    const exposed = wrapper.vm as unknown as BlockEditorExposed;
    const editor = exposed.getEditor();
    expect(editor).not.toBeNull();
    editor?.commands.setTextSelection(1 + "Before [[leg".length);

    expect(
      exposed.completeWikiLink({ start: 9, query: "leg" }, "Canonical"),
    ).toBe(true);
    await nextTick();

    const text = fromTiptapDocument(
      exposed.getEditorJson() as Parameters<typeof fromTiptapDocument>[0],
    )
      .blocks[0].content.filter((node) => node.type === "text")
      .map((node) => node.text)
      .join("");
    expect(text).toBe("Before [[Canonical]] after");
    expect(editor?.state.selection.from).toBe(
      1 + "Before [[Canonical]]".length,
    );

    wrapper.unmount();
  });

  it("does not replace text when completion is not a current wiki link", async () => {
    const original = "Before leg after";
    const wrapper = mount(BlockEditor, {
      props: { modelValue: paragraphDocument(original) },
    });
    await flushPromises();

    const exposed = wrapper.vm as unknown as BlockEditorExposed;
    exposed.getEditor()?.commands.setTextSelection(1 + "Before leg".length);

    expect(
      exposed.completeWikiLink({ start: 7, query: "leg" }, "Canonical"),
    ).toBe(false);
    const text = fromTiptapDocument(
      exposed.getEditorJson() as Parameters<typeof fromTiptapDocument>[0],
    )
      .blocks[0].content.filter((node) => node.type === "text")
      .map((node) => node.text)
      .join("");
    expect(text).toBe(original);

    wrapper.unmount();
  });

  it("focuses a stable block id and rejects an unknown block", async () => {
    const wrapper = mount(BlockEditor, {
      props: { modelValue: fixtureDocument() },
    });
    await flushPromises();

    const exposed = wrapper.vm as unknown as BlockEditorExposed;
    expect(exposed.focusBlock(HEADING_ID)).toBe(true);
    expect(exposed.getEditor()?.state.selection.from).toBe(1);
    expect(exposed.focusBlock("missing-id")).toBe(false);

    wrapper.unmount();
  });

  it("updates editor editability when disabled changes", async () => {
    const wrapper = mount(BlockEditor, {
      props: { disabled: true, modelValue: fixtureDocument() },
    });
    await flushPromises();

    expect(wrapper.find("[contenteditable='false']").exists()).toBe(true);
    await wrapper.setProps({ disabled: false });
    await nextTick();
    expect(wrapper.find("[contenteditable='true']").exists()).toBe(true);

    wrapper.unmount();
  });
});
