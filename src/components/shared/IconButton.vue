<script setup lang="ts">
import type { Component } from "vue";

defineProps<{
  label: string;
  icon: Component;
  active?: boolean;
  danger?: boolean;
  disabled?: boolean;
}>();

defineEmits<{
  click: [];
}>();
</script>

<template>
  <button
    class="btn-icon"
    :class="{ 'border-brand text-brand bg-brand-subtle hover:bg-brand-subtle hover:text-brand': active, 'btn-icon-danger': danger }"
    :disabled="disabled"
    :title="label"
    type="button"
    @click="$emit('click')"
  >
    <component :is="icon" :size="18" />
    <span class="sr-only">{{ label }}</span>
  </button>
</template>
