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

const props = withDefaults(
  defineProps<{
    modelValue: BlockDocument;
    disabled?: boolean;
  }>(),
  { disabled: false },
);

const emit = defineEmits<{
  "update:modelValue": [document: BlockDocument];
}>();

const editor = shallowRef<Editor | null>(null);
let lastEmittedSnapshot: string | null = null;

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
    },
  });
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

onMounted(() => {
  editor.value = createEditor();
});

onBeforeUnmount(() => {
  editor.value?.destroy();
  editor.value = null;
});

defineExpose({ getEditorJson, setSnapshot });
</script>

<template>
  <EditorContent
    :editor="editor ?? undefined"
    class="min-h-36 rounded border border-border bg-surface px-3 py-2 text-ui leading-6 outline-none focus-within:border-primary"
  />
</template>
