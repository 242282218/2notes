<script setup lang="ts">
import { computed } from "vue";

import type {
  BlockDocument,
  BlockNode,
  OutlineItem,
} from "../../types/generated";

const props = defineProps<{
  document: BlockDocument;
}>();

const emit = defineEmits<{
  focus: [id: string];
}>();

function blockText(block: BlockNode): string {
  return block.content
    .map((node) => (node.type === "text" ? node.text : " "))
    .join("")
    .trim();
}

function collectHeadings(blocks: BlockNode[], result: OutlineItem[]) {
  for (const block of blocks) {
    if (block.kind === "heading") {
      result.push({
        id: block.id,
        level: block.attrs.level ?? 1,
        text: blockText(block),
      });
    }
    collectHeadings(block.children, result);
  }
}

const items = computed(() => {
  const headings: OutlineItem[] = [];
  collectHeadings(props.document.blocks, headings);
  return headings;
});
</script>

<template>
  <nav
    v-if="items.length"
    class="border-t border-border px-5 py-4 lg:border-l lg:border-t-0 lg:px-4"
    aria-label="文档大纲"
  >
    <p
      class="mb-2 text-micro font-semibold uppercase tracking-wide text-text-secondary"
    >
      大纲
    </p>
    <ol class="m-0 grid list-none gap-1 p-0">
      <li v-for="item in items" :key="item.id">
        <button
          class="block w-full truncate rounded-sm px-2 py-1 text-left text-ui text-text-secondary outline-none ring-focus hover:bg-bg-secondary hover:text-text-primary"
          type="button"
          :style="{ paddingLeft: `${Math.min(item.level, 6) * 8}px` }"
          @click="emit('focus', item.id)"
        >
          {{ item.text || "无标题" }}
        </button>
      </li>
    </ol>
  </nav>
</template>
