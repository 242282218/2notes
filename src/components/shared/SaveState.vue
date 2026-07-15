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
  <span v-if="label" class="save-state" :class="state">
    <Loader2 v-if="state === 'saving'" :size="14" class="spin" />
    <AlertCircle v-else-if="state === 'failed'" :size="14" />
    <CheckCircle2 v-else-if="state === 'saved'" :size="14" />
    {{ label }}
    <button
      v-if="state === 'failed'"
      type="button"
      class="save-retry"
      @click="$emit('retry')"
    >
      重试
    </button>
  </span>
</template>
