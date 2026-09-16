<script setup lang="ts">
import type { EntryStatus } from "../../types/generated";
import { STATUS_LABELS } from "../../constants/labels";
import { ref } from "vue";

defineProps<{
  modelValue: EntryStatus;
  disabled?: boolean;
}>();

defineEmits<{
  "update:modelValue": [value: EntryStatus];
}>();

const selectRef = ref<HTMLSelectElement | null>(null);

const options: Array<{ value: EntryStatus; label: string }> = (
  Object.keys(STATUS_LABELS) as EntryStatus[]
).map((value) => ({ value, label: STATUS_LABELS[value] }));
</script>

<template>
  <select
    ref="selectRef"
    class="select-base entry-status-select"
    aria-label="状态"
    :value="modelValue"
    :disabled="disabled"
    @change="
      $emit(
        'update:modelValue',
        (selectRef!.value as EntryStatus) ?? modelValue,
      )
    "
  >
    <option v-for="option in options" :key="option.value" :value="option.value">
      {{ option.label }}
    </option>
  </select>
</template>
