<script setup lang="ts">
import type { EntryType } from "../../types/generated";

defineProps<{
  modelValue: EntryType;
}>();

defineEmits<{
  "update:modelValue": [value: EntryType];
}>();

const options: Array<{ value: EntryType; label: string }> = [
  { value: "unclear", label: "未澄清" },
  { value: "idea", label: "想法" },
  { value: "task", label: "任务" },
  { value: "material", label: "素材" },
  { value: "question", label: "问题" },
];
</script>

<template>
  <select
    class="field-select"
    :value="modelValue"
    @change="$emit('update:modelValue', ($event.target as HTMLSelectElement).value as EntryType)"
  >
    <option
      v-for="option in options"
      :key="option.value"
      :value="option.value"
    >
      {{ option.label }}
    </option>
  </select>
</template>
