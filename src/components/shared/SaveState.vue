<script setup lang="ts">
import { AlertCircle, CheckCircle2, Loader2 } from "lucide-vue-next";
import { computed } from "vue";

import { getSaveStateLabel } from "../../composables/useSaveState";
import type { SaveState } from "../../composables/useAutosave";

const props = defineProps<{
  state: SaveState;
  error?: string | null;
}>();

defineEmits<{
  retry: [];
}>();

const label = computed(() => {
  if (props.state === "idle") {
    return "";
  }
  if (props.state === "failed") {
    return props.error || "保存失败";
  }
  return getSaveStateLabel(props.state);
});
</script>

<template>
  <span
    v-if="label"
    class="elevation-2 inline-flex items-center gap-1.5 rounded-full bg-bg-elevated px-3.5 py-1.5 text-ui font-medium text-text-secondary transition-[color,background-color,border-color,opacity] duration-fast ease-token animate-fade-in"
    :role="
      state === 'failed'
        ? 'alert'
        : state === 'saving' || state === 'saved'
          ? 'status'
          : undefined
    "
    :aria-live="state === 'saving' || state === 'saved' ? 'polite' : undefined"
    :class="{
      'text-success bg-success-subtle border-success/20':
        state === 'saved',
      'text-danger bg-danger/10 border-danger/25': state === 'failed',
    }"
  >
    <Loader2 v-if="state === 'saving'" :size="14" aria-hidden="true" />
    <AlertCircle
      v-else-if="state === 'failed'"
      :size="14"
      aria-hidden="true"
    />
    <CheckCircle2
      v-else-if="state === 'saved'"
      :size="14"
      aria-hidden="true"
    />
    <span class="hidden min-[1100px]:inline">{{ label }}</span>
    <button
      v-if="state === 'failed'"
      type="button"
      class="ml-1 font-semibold underline bg-transparent border-none p-0 cursor-pointer text-inherit"
      @click="$emit('retry')"
    >
      重试
    </button>
  </span>
</template>
