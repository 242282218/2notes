<script setup lang="ts">
import { computed, ref, watch } from "vue";

import { knowledgeBreadcrumbsGet } from "../../services/knowledgeApi";
import type { EntryBreadcrumb } from "../../types/generated";

const props = defineProps<{
  entryId: string | null;
  enabled: boolean;
}>();

const emit = defineEmits<{
  open: [id: string];
}>();

const breadcrumbs = ref<EntryBreadcrumb[]>([]);
const loading = ref(false);
let requestId = 0;

const ancestors = computed(() => breadcrumbs.value.slice(0, -1));

watch(
  [() => props.entryId, () => props.enabled],
  async ([entryId, enabled]) => {
    const currentRequest = ++requestId;
    breadcrumbs.value = [];
    if (!entryId || !enabled) return;

    loading.value = true;
    try {
      const next = await knowledgeBreadcrumbsGet(entryId);
      if (currentRequest === requestId) breadcrumbs.value = next;
    } catch {
      if (currentRequest === requestId) breadcrumbs.value = [];
    } finally {
      if (currentRequest === requestId) loading.value = false;
    }
  },
  { immediate: true },
);
</script>

<template>
  <nav
    v-if="!loading && ancestors.length"
    class="flex flex-wrap items-center gap-x-1 gap-y-1 px-5 pt-4 text-caption text-text-secondary"
    aria-label="知识路径"
  >
    <template v-for="(breadcrumb, index) in ancestors" :key="breadcrumb.id">
      <span v-if="index > 0" aria-hidden="true">/</span>
      <button
        class="max-w-[180px] truncate rounded-sm px-1 py-0.5 outline-none ring-focus hover:text-text-primary"
        type="button"
        @click="emit('open', breadcrumb.id)"
      >
        {{ breadcrumb.title || "无标题" }}
      </button>
    </template>
  </nav>
</template>
