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

function allTagsTitle(tags: EntryListItem["tags"]) {
  return tags.map((tag) => `#${tag.name}`).join(", ");
}

// Formatters are locale-independent of the value; build once instead of per render.
const sameDayFormatter = new Intl.DateTimeFormat("zh-CN", {
  hour: "2-digit",
  minute: "2-digit",
});
const otherDayFormatter = new Intl.DateTimeFormat("zh-CN", {
  month: "numeric",
  day: "numeric",
});

function formatUpdatedAt(value: string) {
  const date = new Date(value);
  const today = new Date();
  const sameDay = date.toDateString() === today.toDateString();

  return (sameDay ? sameDayFormatter : otherDayFormatter).format(date);
}
</script>

<template>
  <button
    type="button"
    class="entry-list-item group relative grid w-full gap-1.5 rounded-md border-l-2 bg-transparent px-3 py-3 text-left transition-colors duration-fast ease-token ring-focus hover:bg-selected"
    :class="{
      'border-l-brand bg-selected hover:bg-selected': active,
      'border-l-transparent': !active,
    }"
    :data-entry-id="item.id"
    :aria-current="active ? 'true' : undefined"
    @click="$emit('select', item.id)"
  >
    <span class="flex min-w-0 items-baseline justify-between gap-3">
      <strong
        class="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-ui font-semibold text-text-primary"
      >
        {{ item.title || item.summary || "无标题" }}
      </strong>
      <time
        :datetime="item.updatedAt"
        class="shrink-0 text-micro tabular-nums text-text-tertiary"
      >
        {{ formatUpdatedAt(item.updatedAt) }}
      </time>
    </span>
    <span
      v-if="item.searchSnippet"
      class="search-snippet line-clamp-2 overflow-hidden text-caption text-text-secondary"
    >
      <template v-for="(part, index) in item.searchSnippet.parts" :key="index">
        <mark
          v-if="part.highlighted"
          class="rounded-sm bg-brand-subtle px-0.5 font-medium text-text-primary"
          >{{ part.text }}</mark
        >
        <template v-else>{{ part.text }}</template>
      </template>
    </span>
    <span
      v-else
      class="line-clamp-2 overflow-hidden text-caption text-text-secondary"
      >{{ item.summary }}</span
    >
    <span
      class="mt-0.5 flex min-w-0 items-center gap-1.5 overflow-hidden text-micro text-text-tertiary"
    >
      <span
        v-if="item.knowledgeState === 'knowledge'"
        class="shrink-0 rounded bg-brand-subtle px-1.5 py-0.5 font-medium text-brand"
      >
        {{ KNOWLEDGE_LABEL }}
      </span>
      <span v-if="item.knowledgeState === 'knowledge'" aria-hidden="true"
        >·</span
      >
      <span class="shrink-0 font-medium text-text-secondary">{{
        TYPE_LABELS[item.entryType] ?? item.entryType
      }}</span>
      <span aria-hidden="true">·</span>
      <span class="shrink-0">{{
        STATUS_LABELS[item.status] ?? item.status
      }}</span>
      <span
        v-for="tag in item.tags.slice(0, 2)"
        :key="tag.id"
        data-entry-tag
        :title="`#${tag.name}`"
        class="max-w-[72px] overflow-hidden text-ellipsis whitespace-nowrap rounded bg-bg-active px-1.5 py-0.5"
      >
        #{{ tag.name }}
      </span>
      <span
        v-if="item.tags.length > 2"
        data-tag-overflow
        class="shrink-0"
        :title="allTagsTitle(item.tags)"
        >+{{ item.tags.length - 2 }}</span
      >
    </span>
  </button>
</template>
