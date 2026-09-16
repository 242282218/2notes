<script setup lang="ts">
import { X } from "lucide-vue-next";
import { computed, onUnmounted, ref, watch } from "vue";

import { normalizeTagNames } from "../../composables/useEntryFilters";
import { tagsSuggest } from "../../services/tagApi";
import type { Tag } from "../../types/generated";

const SUGGEST_DEBOUNCE_MS = 150;

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
const listboxId = `tag-suggestions-${crypto.randomUUID()}`;
const selected = computed(() => normalizeTagNames(props.modelValue));
const suggestionsOpen = computed(() => suggestions.value.length > 0);
const activeSuggestionId = computed(() =>
  suggestionsOpen.value
    ? `${listboxId}-option-${activeSuggestionIndex.value}`
    : undefined,
);
let suggestionRequestId = 0;
let suggestTimer: number | undefined;

watch(suggestions, () => {
  activeSuggestionIndex.value = 0;
});

// Debounce network suggestions: every keystroke otherwise fires tagsSuggest.
function cancelPendingSuggest() {
  if (suggestTimer !== undefined) {
    window.clearTimeout(suggestTimer);
    suggestTimer = undefined;
  }
  suggestionRequestId += 1;
}

watch(draft, (value) => {
  if (props.disabled) {
    suggestions.value = [];
    cancelPendingSuggest();
    return;
  }
  cancelPendingSuggest();
  suggestTimer = window.setTimeout(() => {
    suggestTimer = undefined;
    void loadSuggestions(value);
  }, SUGGEST_DEBOUNCE_MS);
});

async function loadSuggestions(value: string) {
  const requestId = ++suggestionRequestId;
  const query = value.trim();
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
}

onUnmounted(cancelPendingSuggest);

function addTag(value = draft.value) {
  if (props.disabled) {
    return;
  }
  const trimmed = value.trim();
  if (!trimmed) {
    draft.value = "";
    suggestions.value = [];
    return;
  }
  // Compute the dedupe key against the *display* view so case-insensitive
  // duplicates are still prevented, but emit the parent's original array plus
  // the new tag — never overwrite the parent's casing/whitespace intent with
  // the normalized projection we render.
  const dedupeKey = trimmed.toLocaleLowerCase();
  const alreadyPresent = props.modelValue.some(
    (existing) => existing.trim().toLocaleLowerCase() === dedupeKey,
  );
  if (!alreadyPresent) {
    emit("update:modelValue", [...props.modelValue, trimmed]);
  }
  draft.value = "";
  suggestions.value = [];
}

function removeTag(tag: string) {
  if (props.disabled) {
    return;
  }
  // `tag` comes from the normalized display view; match it against the parent's
  // original array case-insensitively and drop exactly one equivalent entry so
  // we never silently coerce siblings the parent stored differently.
  const key = tag.trim().toLocaleLowerCase();
  let removed = false;
  const next = props.modelValue.filter((existing) => {
    if (removed) {
      return true;
    }
    if (existing.trim().toLocaleLowerCase() === key) {
      removed = true;
      return false;
    }
    return true;
  });
  emit("update:modelValue", next);
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
      cancelPendingSuggest();
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

function onBlur() {
  if (props.disabled) {
    return;
  }
  commitDraft();
}

defineExpose({ commitDraft });
</script>

<template>
  <div
    class="relative flex min-h-[42px] flex-wrap items-center gap-2 rounded-md border border-border-strong bg-bg-elevated px-2 py-1 transition-colors duration-fast ease-token focus-within:border-brand"
  >
    <span
      v-for="tag in selected"
      :key="tag"
      class="inline-flex h-[26px] items-center gap-1 rounded-full bg-brand-subtle px-[9px] text-ui font-medium text-brand"
    >
      {{ tag }}
      <button
        type="button"
        :title="`移除 ${tag}`"
        :aria-label="`移除标签 ${tag}`"
        :disabled="disabled"
        class="inline-flex size-4 items-center justify-center rounded-sm bg-transparent p-0 text-inherit hover:bg-brand/10"
        @click="removeTag(tag)"
      >
        <X :size="13" aria-hidden="true" />
      </button>
    </span>
    <input
      v-model="draft"
      data-tag-input
      type="text"
      placeholder="添加标签"
      role="combobox"
      aria-label="标签"
      aria-autocomplete="list"
      :aria-expanded="suggestionsOpen"
      :aria-controls="suggestionsOpen ? listboxId : undefined"
      :aria-activedescendant="activeSuggestionId"
      :disabled="disabled"
      class="min-w-[60px] flex-1 border-none bg-transparent ring-focus disabled:opacity-55"
      @keydown="onKeydown"
      @blur="onBlur"
    />
    <div
      v-if="suggestions.length"
      :id="listboxId"
      class="elevation-2 absolute left-0 top-[calc(100%+4px)] z-20 grid min-w-[180px] max-w-full gap-0.5 rounded-md bg-bg-elevated p-1"
      role="listbox"
      aria-label="标签建议"
    >
      <button
        v-for="(tag, index) in suggestions"
        :id="`${listboxId}-option-${index}`"
        :key="tag.id"
        type="button"
        role="option"
        tabindex="-1"
        :aria-selected="index === activeSuggestionIndex"
        :disabled="disabled"
        class="flex h-8 items-center rounded-sm border-none bg-transparent px-2.5 text-left text-ui text-text-primary hover:bg-bg-hover aria-selected:bg-bg-hover"
        @mousedown.prevent
        @click="addTag(tag.name)"
      >
        {{ tag.name }}
      </button>
    </div>
  </div>
</template>
