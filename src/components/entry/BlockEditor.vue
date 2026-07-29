<script setup lang="ts">
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
import type { WikiLinkCompletion } from "../../composables/useWikiLinkCompletion";

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
let lastEmittedSnapshot: string | null = null;

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
      lastEmittedSnapshot = snapshotKey(document);
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
  const replacement = `[[${title}]]`;
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
  const key = snapshotKey(snapshot);
  if (key === lastEmittedSnapshot) {
    lastEmittedSnapshot = null;
    return;
  }
  lastEmittedSnapshot = null;
  activeEditor.commands.setContent(toTiptapDocument(snapshot), {
    emitUpdate: false,
    errorOnInvalidContent: true,
  });
}

function getEditorJson(): TiptapNode | null {
  return editor.value?.getJSON() as TiptapNode | null;
}

watch(
  () => props.modelValue,
  (snapshot) => setSnapshot(snapshot),
  { deep: true },
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
    class="min-h-36 rounded border border-border bg-surface px-3 py-2 text-ui leading-6 outline-none focus-within:border-primary"
  />
</template>
