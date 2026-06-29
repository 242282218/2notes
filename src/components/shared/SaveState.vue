<script setup lang="ts">
import { AlertCircle, CheckCircle2, Loader2 } from "lucide-vue-next";
import { computed } from "vue";

import type { SaveState } from "../../composables/useAutosave";

const props = defineProps<{
  state: SaveState;
  error?: string | null;
}>();

const label = computed(() => {
  switch (props.state) {
    case "dirty":
      return "未保存";
    case "saving":
      return "保存中";
    case "saved":
      return "已保存";
    case "failed":
      return props.error || "保存失败";
    case "idle":
    default:
      return "";
  }
});
</script>

<template>
  <span
    v-if="label"
    class="save-state"
    :class="state"
  >
    <Loader2
      v-if="state === 'saving'"
      :size="14"
      class="spin"
    />
    <AlertCircle
      v-else-if="state === 'failed'"
      :size="14"
    />
    <CheckCircle2
      v-else-if="state === 'saved'"
      :size="14"
    />
    {{ label }}
  </span>
</template>
