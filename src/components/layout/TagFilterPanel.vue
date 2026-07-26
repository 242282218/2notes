<script setup lang="ts">
import { X } from "lucide-vue-next";
import { onMounted, onUnmounted } from "vue";

import type { Tag } from "../../types/generated";

defineProps<{
  tags: Tag[];
  currentTag: string;
  tagsError: string | null;
}>();

const emit = defineEmits<{
  select: [tag: string];
  close: [returnFocus?: boolean];
  retry: [];
}>();

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    emit("close", true);
  }
}

onMounted(() => {
  window.addEventListener("keydown", onKeydown);
});

onUnmounted(() => {
  window.removeEventListener("keydown", onKeydown);
});
</script>

<template>
  <aside
    id="tag-panel"
    class="elevation-2 absolute inset-y-0 left-0 z-40 flex w-[236px] flex-col bg-bg-elevated"
    aria-label="标签筛选"
  >
    <header
      class="flex h-[48px] items-center justify-between border-b border-border px-3"
    >
      <strong class="text-ui font-semibold">全部标签</strong>
      <button
        type="button"
        class="btn-icon"
        aria-label="关闭标签面板"
        @click="emit('close', true)"
      >
        <X :size="16" aria-hidden="true" />
      </button>
    </header>
    <div class="min-h-0 flex-1 overflow-auto p-2">
      <div
        v-if="tagsError"
        class="mb-2 rounded-md border border-border bg-bg-hover px-3 py-2 text-micro text-text-secondary"
        role="alert"
      >
        <p class="mb-2">{{ tagsError }}</p>
        <button
          type="button"
          class="rounded-md border border-border bg-bg-elevated px-2 py-1 text-micro text-text-primary ring-focus"
          @click="emit('retry')"
        >
          重试
        </button>
      </div>
      <button
        type="button"
        class="mb-1 flex h-[36px] w-full items-center justify-between rounded-md border-none bg-transparent px-3 text-ui text-text-secondary transition-colors duration-fast ease-token ring-focus hover:bg-bg-hover hover:text-text-primary"
        :class="{
          'bg-selected font-medium text-text-primary hover:bg-selected':
            !currentTag,
        }"
        @click="emit('select', '')"
      >
        <span>全部标签</span>
        <span class="text-micro text-text-tertiary">{{ tags.length }}</span>
      </button>
      <button
        v-for="tag in tags"
        :key="tag.id"
        type="button"
        class="flex h-[36px] w-full items-center justify-between rounded-md border-none bg-transparent px-3 text-ui text-text-secondary transition-colors duration-fast ease-token ring-focus hover:bg-bg-hover hover:text-text-primary"
        :class="{
          'bg-selected font-medium text-text-primary hover:bg-selected':
            currentTag === tag.name,
        }"
        @click="emit('select', tag.name)"
      >
        <span class="overflow-hidden text-ellipsis whitespace-nowrap"
          >#{{ tag.name }}</span
        >
        <span class="text-micro text-text-tertiary">{{ tag.entryCount }}</span>
      </button>
    </div>
  </aside>
</template>
