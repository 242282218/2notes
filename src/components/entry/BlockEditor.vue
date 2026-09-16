<script setup lang="ts">
import { createDocument } from "@tiptap/core";
import { Editor, EditorContent } from "@tiptap/vue-3";
import StarterKit from "@tiptap/starter-kit";
import UniqueID from "@tiptap/extension-unique-id";
import { onBeforeUnmount, onMounted, shallowRef, watch } from "vue";

import type { BlockDocument } from "../../types/generated";
import { newBlockId } from "../../editor/blockDocument";
import {
  BLOCK_ID_ATTR,
  fromTiptapDocument,
  toTiptapDocument,
  type TiptapNode,
} from "../../editor/tiptapAdapter";
import {
  sanitizeWikiLinkTitle,
  type WikiLinkCompletion,
} from "../../composables/useWikiLinkCompletion";

export interface EditorTextContext {
  text: string;
  caret: number;
}

type TextContextEditor = {
  state: {
    selection: {
      empty: boolean;
      $from: {
        parent: { isTextblock: boolean };
        nodeBefore: { isText: boolean; text?: string; nodeSize: number } | null;
        nodeAfter: { isText: boolean; text?: string; nodeSize: number } | null;
        textOffset: number;
      };
    };
  };
};

const props = withDefaults(
  defineProps<{
    modelValue: BlockDocument;
    disabled?: boolean;
    wikiSuggestionsOpen?: boolean;
    wikiListboxId?: string;
    wikiActiveDescendant?: string;
  }>(),
  {
    disabled: false,
    wikiSuggestionsOpen: false,
    wikiListboxId: undefined,
    wikiActiveDescendant: undefined,
  },
);

const emit = defineEmits<{
  "update:modelValue": [document: BlockDocument];
  selectionChange: [context: EditorTextContext | null];
  editorKeydown: [event: KeyboardEvent];
  editorBlur: [];
}>();

const editor = shallowRef<Editor | null>(null);
// The document object most recently emitted by the editor. Reference equality
// short-circuits the echo round-trip (parent v-model returns the same object),
// so per-keystroke work stays O(1). A full digest is only computed when a
// DIFFERENT object arrives (entry switch / refresh), where it decides whether
// the content is semantically identical and the cursor may be kept.
let lastEmittedSnapshot: BlockDocument | null = null;

function editorAttributes(): Record<string, string> {
  const attributes: Record<string, string> = {
    "aria-expanded": props.wikiSuggestionsOpen ? "true" : "false",
    "aria-label": "正文",
    role: "combobox",
  };
  if (props.wikiSuggestionsOpen) {
    attributes["aria-autocomplete"] = "list";
    if (props.wikiListboxId) attributes["aria-controls"] = props.wikiListboxId;
    if (props.wikiActiveDescendant) {
      attributes["aria-activedescendant"] = props.wikiActiveDescendant;
    }
  }
  return attributes;
}

function snapshotKey(snapshot: BlockDocument): string {
  return JSON.stringify(snapshot);
}

function createEditor() {
  return new Editor({
    content: toTiptapDocument(props.modelValue),
    editable: !props.disabled,
    extensions: [
      StarterKit.configure({
        link: { openOnClick: false },
        trailingNode: false,
      }),
      UniqueID.configure({
        attributeName: BLOCK_ID_ATTR,
        types: [
          "paragraph",
          "heading",
          "bulletList",
          "orderedList",
          "listItem",
          "blockquote",
          "codeBlock",
          "horizontalRule",
        ],
        generateID: () => newBlockId(),
      }),
    ],
    onUpdate: ({ editor: activeEditor }) => {
      const document = fromTiptapDocument(activeEditor.getJSON());
      lastEmittedSnapshot = document;
      emit("update:modelValue", document);
      emit("selectionChange", getTextContext(activeEditor));
    },
    onSelectionUpdate: ({ editor: activeEditor }) => {
      emit("selectionChange", getTextContext(activeEditor));
    },
    onBlur: () => emit("editorBlur"),
    editorProps: {
      attributes: editorAttributes(),
      handleKeyDown: (_, event) => {
        emit("editorKeydown", event);
        return event.defaultPrevented;
      },
    },
  });
}

function getTextContext(
  activeEditor: TextContextEditor,
): EditorTextContext | null {
  const selection = activeEditor.state.selection;
  if (!selection.empty || !selection.$from.parent.isTextblock) return null;

  const before = selection.$from.nodeBefore;
  const after = selection.$from.nodeAfter;
  if (selection.$from.textOffset > 0 && before?.isText && after?.isText) {
    return {
      text: `${before.text ?? ""}${after.text ?? ""}`,
      caret: selection.$from.textOffset,
    };
  }
  if (before?.isText) {
    return { text: before.text ?? "", caret: before.nodeSize };
  }
  if (after?.isText) {
    return { text: after.text ?? "", caret: 0 };
  }
  return { text: "", caret: 0 };
}

function completeWikiLink(
  completion: WikiLinkCompletion,
  title: string,
): boolean {
  const activeEditor = editor.value;
  if (!activeEditor) return false;
  const context = getTextContext(activeEditor);
  if (
    !context ||
    completion.start < 2 ||
    completion.start + completion.query.length !== context.caret ||
    context.text.slice(completion.start - 2, completion.start) !== "[["
  ) {
    return false;
  }

  const from =
    activeEditor.state.selection.from - (context.caret - completion.start + 2);
  const safeTitle = sanitizeWikiLinkTitle(title);
  if (safeTitle === null) {
    return false;
  }
  const replacement = `[[${safeTitle}]]`;
  activeEditor
    .chain()
    .focus()
    .insertContentAt(
      { from, to: activeEditor.state.selection.from },
      replacement,
    )
    .setTextSelection(from + replacement.length)
    .run();
  return true;
}

function focusBlock(blockId: string): boolean {
  const activeEditor = editor.value;
  if (!activeEditor) return false;

  let position: number | null = null;
  activeEditor.state.doc.descendants((node, pos) => {
    if (node.attrs[BLOCK_ID_ATTR] === blockId) {
      position = pos + 1;
      return false;
    }
    return true;
  });
  if (position === null) return false;

  activeEditor
    .chain()
    .focus()
    .setTextSelection(position)
    .scrollIntoView()
    .run();
  return true;
}

function setSnapshot(snapshot: BlockDocument) {
  const activeEditor = editor.value;
  if (!activeEditor) return;
  if (snapshot === lastEmittedSnapshot) {
    return;
  }
  if (
    lastEmittedSnapshot !== null &&
    snapshotKey(snapshot) === snapshotKey(lastEmittedSnapshot)
  ) {
    lastEmittedSnapshot = snapshot;
    return;
  }
  const content = createDocument(
    toTiptapDocument(snapshot),
    activeEditor.schema,
    {},
    { errorOnInvalidContent: true },
  );
  const { tr } = activeEditor.state;
  tr.replaceWith(0, activeEditor.state.doc.content.size, content)
    .setMeta("preventUpdate", true)
    .setMeta("addToHistory", false);
  activeEditor.view.dispatch(tr);
  lastEmittedSnapshot = snapshot;
}

function getEditorJson(): TiptapNode | null {
  return editor.value?.getJSON() as TiptapNode | null;
}

// Shallow watch is sufficient: the parent replaces the whole document object on
// every snapshot (EntryDetail swaps document.value / emits a new object).
watch(
  () => props.modelValue,
  (snapshot) => setSnapshot(snapshot),
);

watch(
  () => props.disabled,
  (disabled) => editor.value?.setEditable(!disabled),
);

watch(
  [
    () => props.wikiSuggestionsOpen,
    () => props.wikiListboxId,
    () => props.wikiActiveDescendant,
  ],
  () =>
    editor.value?.setOptions({
      editorProps: {
        attributes: editorAttributes(),
        handleKeyDown: (_, event) => {
          emit("editorKeydown", event);
          return event.defaultPrevented;
        },
      },
    }),
);

onMounted(() => {
  editor.value = createEditor();
});

onBeforeUnmount(() => {
  editor.value?.destroy();
  editor.value = null;
});

defineExpose({
  completeWikiLink,
  focusBlock,
  getEditor: () => editor.value,
  getEditorJson,
  setSnapshot,
});
</script>

<template>
  <EditorContent
    :editor="editor ?? undefined"
    class="min-h-36 rounded border border-border bg-bg-elevated px-3 py-2 text-ui leading-6 outline-none focus-within:border-brand"
  />
</template>
