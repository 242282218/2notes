<script setup lang="ts">
import { X } from "lucide-vue-next";
import { computed, ref, watch } from "vue";

import { normalizeTagNames } from "../../composables/useEntryFilters";
import { tagsSuggest } from "../../services/tagApi";
import type { Tag } from "../../types/generated";

const props = defineProps<{
  modelValue: string[];
  disabled?: boolean;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: string[]];
}>();

const draft = ref("");
const suggestions = ref<Tag[]>([]);
const activeSuggestionIndex = ref(0);
const selected = computed(() => normalizeTagNames(props.modelValue));
let suggestionRequestId = 0;

watch(suggestions, () => {
  activeSuggestionIndex.value = 0;
});

watch(draft, async (value) => {
  const requestId = ++suggestionRequestId;
  const query = value.trim();
  if (props.disabled) {
    suggestions.value = [];
    return;
  }
  try {
    const nextSuggestions = query ? await tagsSuggest(query) : [];
    if (requestId === suggestionRequestId) {
      suggestions.value = nextSuggestions;
    }
  } catch {
    if (requestId === suggestionRequestId) {
      suggestions.value = [];
    }
  }
});

function addTag(value = draft.value) {
  if (props.disabled) {
    return;
  }
  const next = normalizeTagNames([...selected.value, value]);
  emit("update:modelValue", next);
  draft.value = "";
  suggestions.value = [];
}

function removeTag(tag: string) {
  if (props.disabled) {
    return;
  }
  emit(
    "update:modelValue",
    selected.value.filter((item) => item !== tag),
  );
}

function onKeydown(event: KeyboardEvent) {
  if (props.disabled || event.isComposing) {
    return;
  }

  if (suggestions.value.length > 0) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      activeSuggestionIndex.value =
        (activeSuggestionIndex.value + 1) % suggestions.value.length;
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      activeSuggestionIndex.value =
        (activeSuggestionIndex.value - 1 + suggestions.value.length) %
        suggestions.value.length;
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      suggestions.value = [];
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      addTag(suggestions.value[activeSuggestionIndex.value].name);
      return;
    }
  }

  if (event.key === "Enter" || event.key === ",") {
    event.preventDefault();
    addTag();
  }
  if (event.key === "Backspace" && !draft.value && selected.value.length) {
    removeTag(selected.value[selected.value.length - 1]);
  }
}

function commitDraft() {
  if (!props.disabled && draft.value.trim()) {
    addTag();
  }
}

function onBlur(event: FocusEvent) {
  if (props.disabled) {
    return;
  }
  if (
    event.relatedTarget instanceof HTMLElement &&
    event.relatedTarget.closest(".suggestions")
  ) {
    return;
  }
  commitDraft();
}

defineExpose({ commitDraft });
</script>

<template>
  <div class="relative flex min-h-[42px] flex-wrap items-center gap-2 rounded-md border border-border-strong bg-bg-elevated px-2 py-1 transition-colors duration-150 focus-within:border-brand focus-within:shadow-[0_0_0_3px_var(--color-focus-ring-bg)]">
    <span
      v-for="tag in selected"
      :key="tag"
      class="inline-flex h-[26px] items-center gap-1 rounded-full bg-brand-subtle px-[9px] text-[13px] font-medium text-brand"
    >
      {{ tag }}
      <button
        type="button"
        :title="`移除 ${tag}`"
        :disabled="disabled"
        class="inline-flex size-4 items-center justify-center rounded-sm bg-transparent p-0 text-inherit hover:bg-brand/10 dark:hover:bg-brand/12"
        @click="removeTag(tag)"
      >
        <X :size="13" />
      </button>
    </span>
    <input
      v-model="draft"
      type="text"
      placeholder="添加标签"
      aria-label="标签"
      :disabled="disabled"
      class="min-w-[60px] flex-1 bg-transparent outline-none border-none disabled:opacity-55"
      @keydown="onKeydown"
      @blur="onBlur"
    />
    <div
      v-if="suggestions.length"
      class="absolute left-0 top-[calc(100%+5px)] z-[5] grid min-w-[180px] gap-1 rounded-lg border border-border-strong bg-bg-elevated p-2 shadow-lg"
      role="listbox"
      aria-label="标签建议"
    >
      <button
        v-for="(tag, index) in suggestions"
        :id="`tag-suggestion-${index}`"
        :key="tag.id"
        type="button"
        role="option"
        tabindex="-1"
        :aria-selected="index === activeSuggestionIndex"
        :disabled="disabled"
        class="group relative flex h-[34px] items-center rounded-md border-none bg-transparent px-3 text-left text-text-primary hover:bg-bg-hover aria-selected:bg-bg-hover"
        @click="addTag(tag.name)"
      >
        <div class="absolute bottom-1.5 left-0 top-1.5 w-[3px] rounded-r-full bg-brand opacity-0 transition-opacity duration-150 group-aria-selected:opacity-100"></div>
        {{ tag.name }}
      </button>
    </div>
  </div>
</template>
