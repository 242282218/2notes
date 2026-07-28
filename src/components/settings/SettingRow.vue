<script setup lang="ts">
withDefaults(
  defineProps<{
    title: string;
    description?: string;
    descriptionTitle?: string;
    as?: "div" | "label";
  }>(),
  {
    description: "",
    descriptionTitle: undefined,
    as: "div",
  },
);
</script>

<template>
  <component
    :is="as"
    class="grid min-h-17 grid-cols-[minmax(0,1fr)_minmax(112px,auto)] items-center gap-5 border-b border-border py-3 max-[520px]:grid-cols-1 max-[520px]:gap-3"
  >
    <div class="grid min-w-0 gap-1">
      <strong class="text-heading text-text-primary">{{ title }}</strong>
      <span
        class="min-w-0 text-ui text-text-tertiary"
        :title="descriptionTitle"
      >
        <slot name="description">{{ description }}</slot>
      </span>
    </div>
    <div class="min-w-0 justify-self-end max-[520px]:justify-self-start">
      <slot />
    </div>
  </component>
</template>
