<script setup lang="ts">
import type { EntryType } from "../../types/generated";
import { TYPE_LABELS } from "../../constants/labels";
import { ref } from "vue";

defineProps<{
  modelValue: EntryType;
  disabled?: boolean;
}>();

defineEmits<{
  "update:modelValue": [value: EntryType];
}>();

const selectRef = ref<HTMLSelectElement | null>(null);

const options: Array<{ value: EntryType; label: string }> = (
  Object.keys(TYPE_LABELS) as EntryType[]
).map((value) => ({ value, label: TYPE_LABELS[value] }));
</script>

<template>
  <select
    ref="selectRef"
    class="select-base entry-type-select"
    aria-label="类型"
    :value="modelValue"
    :disabled="disabled"
    @change="
      $emit('update:modelValue', (selectRef!.value as EntryType) ?? modelValue)
    "
  >
    <option v-for="option in options" :key="option.value" :value="option.value">
      {{ option.label }}
    </option>
  </select>
</template>
