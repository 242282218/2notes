<script setup lang="ts">
import type { KnowledgeSuggestion } from "../../types/generated";

defineProps<{
  suggestions: KnowledgeSuggestion[];
  activeIndex: number;
  listboxId: string;
}>();

const emit = defineEmits<{
  select: [suggestion: KnowledgeSuggestion];
}>();
</script>

<template>
  <div
    :id="listboxId"
    class="wiki-link-suggestions elevation-2 absolute left-0 top-[calc(100%+4px)] z-20 grid min-w-[220px] max-w-full gap-0.5 rounded-md bg-bg-elevated p-1"
    role="listbox"
    aria-label="知识条目建议"
  >
    <button
      v-for="(suggestion, index) in suggestions"
      :id="`${listboxId}-option-${index}`"
      :key="suggestion.id"
      type="button"
      role="option"
      tabindex="-1"
      :aria-selected="index === activeIndex"
      class="grid min-h-8 w-full gap-0.5 rounded-sm border-none bg-transparent px-2.5 py-1.5 text-left text-ui text-text-primary hover:bg-bg-hover aria-selected:bg-bg-hover"
      @mousedown.prevent
      @click="emit('select', suggestion)"
    >
      <span>{{ suggestion.title }}</span>
      <small
        v-if="suggestion.matchedAlias"
        class="text-caption text-text-secondary"
      >
        历史标题：{{ suggestion.matchedAlias }}
      </small>
    </button>
  </div>
</template>
