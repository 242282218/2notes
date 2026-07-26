<script setup lang="ts">
import { Monitor, Moon, Sun } from "lucide-vue-next";

import { useTheme } from "../../composables/useTheme";
import type { ThemeMode } from "../../types/generated";

const { mode, options } = useTheme();

const icons: Record<ThemeMode, typeof Monitor> = {
  system: Monitor,
  light: Sun,
  dark: Moon,
};

const emit = defineEmits<{
  change: [mode: ThemeMode];
}>();

function isThemeMode(value: string): value is ThemeMode {
  return value === "system" || value === "light" || value === "dark";
}

function onChange(event: Event) {
  const value = (event.target as HTMLSelectElement).value;
  if (!isThemeMode(value)) {
    return;
  }
  emit("change", value);
}
</script>

<template>
  <div
    class="inline-flex h-[34px] items-center gap-2 rounded-md border border-border bg-bg-elevated px-2.5 text-text-secondary"
  >
    <component :is="icons[mode]" :size="16" aria-hidden="true" />
    <select
      class="ring-focus cursor-pointer border-0 bg-transparent text-inherit outline-0"
      :value="mode"
      aria-label="主题"
      @change="onChange"
    >
      <option
        v-for="option in options"
        :key="option.value"
        :value="option.value"
      >
        {{ option.label }}
      </option>
    </select>
  </div>
</template>
