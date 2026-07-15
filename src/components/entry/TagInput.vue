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
const selected = computed(() => normalizeTagNames(props.modelValue));
let suggestionRequestId = 0;

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
  <div class="tag-input">
    <span v-for="tag in selected" :key="tag" class="tag-pill">
      {{ tag }}
      <button
        type="button"
        :title="`移除 ${tag}`"
        :disabled="disabled"
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
      @keydown="onKeydown"
      @blur="onBlur"
    />
    <div v-if="suggestions.length" class="suggestions">
      <button
        v-for="tag in suggestions"
        :key="tag.id"
        type="button"
        :disabled="disabled"
        @click="addTag(tag.name)"
      >
        {{ tag.name }}
      </button>
    </div>
  </div>
</template>
