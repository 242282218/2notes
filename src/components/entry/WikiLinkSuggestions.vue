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
  <div :id="listboxId" class="wiki-link-suggestions" role="listbox">
    <button
      v-for="(suggestion, index) in suggestions"
      :id="`${listboxId}-option-${index}`"
      :key="suggestion.id"
      type="button"
      role="option"
      tabindex="-1"
      :aria-selected="index === activeIndex"
      @mousedown.prevent
      @click="emit('select', suggestion)"
    >
      <span>{{ suggestion.title }}</span>
      <small v-if="suggestion.matchedAlias">
        历史标题：{{ suggestion.matchedAlias }}
      </small>
    </button>
  </div>
</template>
