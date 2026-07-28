<script setup lang="ts">
import type { EntryType } from "../../types/generated";
import { ref } from "vue";

defineProps<{
  modelValue: EntryType;
  disabled?: boolean;
}>();

defineEmits<{
  "update:modelValue": [value: EntryType];
}>();

const selectRef = ref<HTMLSelectElement | null>(null);

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
