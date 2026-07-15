<script setup lang="ts">
import {
  KNOWLEDGE_LABEL,
  STATUS_LABELS,
  TYPE_LABELS,
} from "../../constants/labels";
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
    <span class="entry-title">{{
      item.title || item.summary || "untitled"
    }}</span>
    <span class="entry-summary">{{ item.summary }}</span>
    <span class="entry-meta">
      <span v-if="item.knowledgeState === 'knowledge'">{{
        KNOWLEDGE_LABEL
      }}</span>
      <span>{{ TYPE_LABELS[item.entryType] ?? item.entryType }}</span>
      <span>{{ STATUS_LABELS[item.status] ?? item.status }}</span>
      <span v-for="tag in item.tags.slice(0, 3)" :key="tag.id"
        >#{{ tag.name }}</span
      >
    </span>
  </button>
</template>
