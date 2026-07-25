<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { Link2 } from "lucide-vue-next";

import { knowledgeRelationsGet } from "../../services/knowledgeApi";
import type { KnowledgeRelations, RelatedEntry } from "../../types/generated";
import EmptyState from "../shared/EmptyState.vue";

const props = defineProps<{
  entryId: string;
  revision: number;
  refreshToken: number;
}>();

const emit = defineEmits<{
  openRelated: [id: string];
}>();

const relations = ref<KnowledgeRelations | null>(null);
const loading = ref(false);
const error = ref("");
const isEmpty = computed(
  () =>
    relations.value !== null &&
    relations.value.outgoing.length === 0 &&
    relations.value.backlinks.length === 0 &&
    relations.value.unresolved.length === 0,
);
let requestId = 0;

watch(
  () => [props.entryId, props.revision, props.refreshToken] as const,
  loadRelations,
  { immediate: true },
);

onBeforeUnmount(() => {
  requestId += 1;
});

async function loadRelations() {
  const currentRequest = ++requestId;
  loading.value = true;
  error.value = "";
  relations.value = null;
  try {
    const result = await knowledgeRelationsGet(props.entryId);
    if (currentRequest !== requestId) return;
    if (!result) {
      throw new Error("关联数据格式无效");
    }
    relations.value = result;
  } catch (loadError) {
    if (currentRequest !== requestId) return;
    error.value =
      loadError instanceof Error ? loadError.message : "加载关联失败";
  } finally {
    if (currentRequest === requestId) {
      loading.value = false;
    }
  }
}

function entryTitle(entry: RelatedEntry) {
  return entry.title?.trim() || "无标题";
}
</script>

<template>
  <section class="grid gap-4 rounded-xl border border-border bg-bg-elevated p-4 shadow-sm" aria-label="关联">
    <h3 class="m-0 text-[15px] font-semibold text-text-primary">关联</h3>
    <EmptyState v-if="loading" title="加载关联中" />
    <p v-else-if="error" class="text-danger m-0" role="alert">{{ error }}</p>
    <EmptyState
      v-else-if="isEmpty"
      :icon="Link2"
      title="暂无关联"
      description="在正文中使用 [[标题]] 创建知识链接"
    />
    <template v-else-if="relations">
      <section v-if="relations.outgoing.length" class="grid gap-2">
        <h4 class="m-0 text-[13px] font-medium text-text-secondary">出链</h4>
        <ul class="m-0 grid gap-2 p-0 list-none">
          <li v-for="entry in relations.outgoing" :key="entry.id">
            <button
              type="button"
              class="grid w-full gap-0.5 rounded-md border border-border bg-bg-secondary p-3 text-left text-text-primary transition-colors duration-150 ring-focus hover:border-border-hover hover:bg-bg-hover"
              @click="emit('openRelated', entry.id)"
            >
              <span class="font-medium text-[14px]">{{ entryTitle(entry) }}</span>
              <small class="text-[12px] text-text-tertiary truncate">{{ entry.summary }}</small>
              <small class="text-[12px] text-text-tertiary">{{ entry.occurrenceCount }} 次</small>
              <small v-if="entry.deletedAt" class="text-[12px] text-danger">已在回收站</small>
            </button>
          </li>
        </ul>
      </section>

      <section v-if="relations.backlinks.length" class="grid gap-2">
        <h4 class="m-0 text-[13px] font-medium text-text-secondary">反向链接</h4>
        <ul class="m-0 grid gap-2 p-0 list-none">
          <li v-for="entry in relations.backlinks" :key="entry.id">
            <button
              type="button"
              class="grid w-full gap-0.5 rounded-md border border-border bg-bg-secondary p-3 text-left text-text-primary transition-colors duration-150 ring-focus hover:border-border-hover hover:bg-bg-hover"
              @click="emit('openRelated', entry.id)"
            >
              <span class="font-medium text-[14px]">{{ entryTitle(entry) }}</span>
              <small class="text-[12px] text-text-tertiary truncate">{{ entry.summary }}</small>
              <small class="text-[12px] text-text-tertiary">{{ entry.occurrenceCount }} 次</small>
              <small v-if="entry.deletedAt" class="text-[12px] text-danger">已在回收站</small>
            </button>
          </li>
        </ul>
      </section>

      <section v-if="relations.unresolved.length" class="grid gap-2">
        <h4 class="m-0 text-[13px] font-medium text-text-secondary">未解析链接</h4>
        <ul class="m-0 grid gap-2 p-0 list-none">
          <li
            v-for="entry in relations.unresolved"
            :key="`${entry.rawTarget}-${entry.occurrenceCount}`"
            class="grid w-full gap-0.5 rounded-md border border-border bg-bg-secondary p-3 text-left text-text-primary transition-colors duration-150"
          >
            <span class="font-medium text-[14px]">{{ entry.rawTarget }}</span>
            <small class="text-[12px] text-text-tertiary">{{ entry.occurrenceCount }} 次</small>
          </li>
        </ul>
      </section>
    </template>
  </section>
</template>
