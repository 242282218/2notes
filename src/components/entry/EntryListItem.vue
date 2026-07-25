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
    class="entry-list-item group relative grid w-full gap-1 border-b border-border-subtle bg-bg-elevated px-4 py-3.5 text-left transition-colors duration-150 focus-visible:z-10 ring-focus hover:bg-neutral-50 dark:hover:bg-neutral-800/50"
    :class="{ 'bg-neutral-100 dark:bg-neutral-800 hover:bg-neutral-100 dark:hover:bg-neutral-800 shadow-[inset_3px_0_0_var(--color-brand)]': active }"
    :data-entry-id="item.id"
    @click="$emit('select', item.id)"
  >
    <span class="overflow-hidden text-ellipsis whitespace-nowrap text-[14px] font-semibold tracking-tight text-text-primary">
      {{ item.title || item.summary || "untitled" }}
    </span>
    <span v-if="item.searchSnippet" class="search-snippet line-clamp-2 overflow-hidden text-[13px] leading-relaxed text-text-secondary">
      <template v-for="(part, index) in item.searchSnippet.parts" :key="index">
        <mark v-if="part.highlighted" class="rounded-[3px] bg-brand-subtle px-[3px] py-[1px] font-medium text-text-primary">{{ part.text }}</mark>
        <template v-else>{{ part.text }}</template>
      </template>
    </span>
    <span v-else class="line-clamp-2 overflow-hidden text-[13px] leading-relaxed text-text-secondary">{{ item.summary }}</span>
    <span class="mt-[2px] flex min-w-0 flex-wrap gap-2 text-[12px] tabular-nums text-text-tertiary">
      <span
        v-if="item.knowledgeState === 'knowledge'"
        class="inline-flex max-w-full items-center overflow-hidden text-ellipsis whitespace-nowrap rounded-full bg-brand/10 px-2 min-h-[22px] text-[12px] text-brand"
      >
        {{ KNOWLEDGE_LABEL }}
      </span>
      <span class="inline-flex max-w-full items-center overflow-hidden text-ellipsis whitespace-nowrap rounded-full bg-bg-active px-2 min-h-[22px] text-[12px]">
        {{ TYPE_LABELS[item.entryType] ?? item.entryType }}
      </span>
      <span class="inline-flex max-w-full items-center overflow-hidden text-ellipsis whitespace-nowrap rounded-full bg-bg-active px-2 min-h-[22px] text-[12px]">
        {{ STATUS_LABELS[item.status] ?? item.status }}
      </span>
      <span
        v-for="tag in item.tags.slice(0, 3)"
        :key="tag.id"
        :title="`#${tag.name}`"
        class="inline-flex max-w-full items-center overflow-hidden text-ellipsis whitespace-nowrap rounded-full bg-bg-active px-2 min-h-[22px] text-[12px]"
      >
        #{{ tag.name }}
      </span>
    </span>
  </button>
</template>
