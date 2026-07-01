<script lang="ts">
export const typeLabels: Record<string, string> = Object.freeze({
  unclear: "未澄清",
  idea: "想法",
  task: "任务",
  material: "素材",
  question: "问题",
});

export const statusLabels: Record<string, string> = Object.freeze({
  pending: "待处理",
  done: "已完成",
  archived: "已归档",
});
</script>

<script setup lang="ts">
import type { EntryListItem } from "../../types/generated";

defineProps<{
  item: EntryListItem;
  active: boolean;
}>();

defineEmits<{
  select: [id: string];
}>();
</script>

<template>
  <button
    type="button"
    class="entry-list-item"
    :class="{ active }"
    @click="$emit('select', item.id)"
  >
    <span class="entry-title">{{ item.title || item.summary || "untitled" }}</span>
    <span class="entry-summary">{{ item.summary }}</span>
    <span class="entry-meta">
      <span>{{ typeLabels[item.entryType] }}</span>
      <span>{{ statusLabels[item.status] }}</span>
      <span
        v-for="tag in item.tags.slice(0, 3)"
        :key="tag.id"
      >#{{ tag.name }}</span>
    </span>
  </button>
</template>
