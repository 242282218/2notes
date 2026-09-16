<script setup lang="ts">
import { computed } from "vue";

const props = withDefaults(
  defineProps<{
    lines?: number;
  }>(),
  {
    lines: 3,
  },
);

const widths = [72, 92, 64, 84, 58];
const lineCount = computed(() => Math.max(1, Math.floor(props.lines)));
</script>

<template>
  <div data-skeleton-text class="grid gap-2 animate-fade-in" aria-hidden="true">
    <span
      v-for="index in lineCount"
      :key="index"
      data-skeleton-line
      class="h-3 rounded-sm bg-bg-inset skeleton-pulse"
      :style="{ width: `${widths[(index - 1) % widths.length]}%` }"
    />
  </div>
</template>
