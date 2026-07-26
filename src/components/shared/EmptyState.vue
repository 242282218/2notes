<script setup lang="ts">
import type { Component } from "vue";

withDefaults(
  defineProps<{
    icon?: Component;
    title: string;
    description?: string;
    busy?: boolean;
    size?: "fill" | "compact";
  }>(),
  {
    icon: undefined,
    description: undefined,
    busy: false,
    size: "compact",
  },
);
</script>

<template>
  <div
    data-empty-state
    class="grid place-items-center justify-items-center gap-2 p-5 text-center text-text-tertiary"
    :class="size === 'fill' ? 'h-full content-center' : 'min-h-40'"
    :role="busy ? 'status' : undefined"
    :aria-busy="busy || undefined"
  >
    <component
      :is="icon"
      v-if="icon"
      :size="40"
      stroke-width="1.5"
      class="mb-1 text-border-hover"
      aria-hidden="true"
    />
    <strong class="text-heading font-medium text-text-secondary">{{ title }}</strong>
    <span v-if="description" class="max-w-65 text-ui">{{ description }}</span>
  </div>
</template>
