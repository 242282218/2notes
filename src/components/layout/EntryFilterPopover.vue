<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref, watch } from "vue";

import type { EntryStatus, EntryType } from "../../types/generated";

const props = defineProps<{
  open: boolean;
  entryType: EntryType | "";
  status: EntryStatus | "";
}>();

const emit = defineEmits<{
  typeChange: [value: EntryType | ""];
  statusChange: [value: EntryStatus | ""];
  close: [returnFocus?: boolean];
}>();

const typeOptions: Array<{ value: EntryType | ""; label: string }> = [
  { value: "", label: "全部类型" },
  { value: "unclear", label: "未澄清" },
  { value: "idea", label: "想法" },
  { value: "task", label: "任务" },
  { value: "material", label: "素材" },
  { value: "question", label: "问题" },
];

const statusOptions: Array<{ value: EntryStatus | ""; label: string }> = [
  { value: "", label: "全部状态" },
  { value: "pending", label: "待处理" },
  { value: "done", label: "已完成" },
  { value: "archived", label: "已归档" },
];

const typeSelectRef = ref<HTMLSelectElement | null>(null);

function onKeydown(event: KeyboardEvent) {
  if (!props.open) return;
  if (event.key === "Escape") {
    event.preventDefault();
    emit("close", true);
  }
}

watch(
  () => props.open,
  (open) => {
    if (open) {
      void nextTick(() => typeSelectRef.value?.focus());
    }
  },
);

onMounted(() => {
  window.addEventListener("keydown", onKeydown);
});

onUnmounted(() => {
  window.removeEventListener("keydown", onKeydown);
});
</script>

<template>
  <Transition name="filter-panel">
    <div
      v-if="open"
      id="entry-filter-popover"
      class="elevation-2 absolute right-0 top-[42px] z-50 grid w-[220px] gap-3 rounded-lg bg-bg-elevated p-3"
      aria-label="条目筛选"
    >
      <label class="grid gap-1 text-micro text-text-tertiary">
        类型
        <select
          ref="typeSelectRef"
          class="select-base w-full text-ui"
          aria-label="类型筛选"
          :value="entryType"
          @change="
            emit(
              'typeChange',
              ($event.target as HTMLSelectElement).value as EntryType | '',
            )
          "
        >
          <option
            v-for="option in typeOptions"
            :key="option.value || 'all'"
            :value="option.value"
          >
            {{ option.label }}
          </option>
        </select>
      </label>
      <label class="grid gap-1 text-micro text-text-tertiary">
        状态
        <select
          class="select-base w-full text-ui"
          aria-label="状态筛选"
          :value="status"
          @change="
            emit(
              'statusChange',
              ($event.target as HTMLSelectElement).value as EntryStatus | '',
            )
          "
        >
          <option
            v-for="option in statusOptions"
            :key="option.value || 'all'"
            :value="option.value"
          >
            {{ option.label }}
          </option>
        </select>
      </label>
    </div>
  </Transition>
</template>

<style scoped>
.filter-panel-enter-active,
.filter-panel-leave-active {
  transition:
    opacity var(--duration-base) var(--ease-out),
    transform var(--duration-base) var(--ease-out);
}

.filter-panel-enter-from,
.filter-panel-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
