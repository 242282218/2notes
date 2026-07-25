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
  if (props.state === "failed") {
    return props.error || "保存失败";
  }
  return getSaveStateLabel(props.state);
});
</script>

<template>
  <span
    v-if="label"
    class="inline-flex items-center gap-1.5 rounded-full border border-border-subtle glass-panel px-3.5 py-1.5 text-[13px] font-medium text-text-secondary transition-all duration-150"
    :class="{
      'text-success bg-success-subtle border-success/20 animate-pulse-saved': state === 'saved',
      'text-danger bg-danger/10 border-danger/25': state === 'failed'
    }"
  >
    <Loader2 v-if="state === 'saving'" :size="14" class="animate-spin" />
    <AlertCircle v-else-if="state === 'failed'" :size="14" />
    <CheckCircle2 v-else-if="state === 'saved'" :size="14" />
    <span>{{ label }}</span>
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
