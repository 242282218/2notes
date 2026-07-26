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
    class="btn-icon size-[34px] shrink-0"
    :class="{
      'border-brand text-brand bg-brand-subtle hover:bg-brand-subtle hover:text-brand':
        active,
      'btn-icon-danger': danger,
    }"
    :disabled="disabled"
    :aria-label="label"
    :title="label"
    type="button"
    @click="$emit('click')"
  >
    <component :is="icon" :size="18" aria-hidden="true" />
  </button>
</template>
