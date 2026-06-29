<script setup lang="ts">
import type { EntryStatus } from "../../types/generated";

defineProps<{
  modelValue: EntryStatus;
}>();

defineEmits<{
  "update:modelValue": [value: EntryStatus];
}>();

const options: Array<{ value: EntryStatus; label: string }> = [
  { value: "pending", label: "待处理" },
  { value: "done", label: "已完成" },
  { value: "archived", label: "已归档" },
];
</script>

<template>
  <select
    class="field-select"
    :value="modelValue"
    @change="$emit('update:modelValue', ($event.target as HTMLSelectElement).value as EntryStatus)"
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
