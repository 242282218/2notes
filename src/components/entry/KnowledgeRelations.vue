<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";

import { knowledgeRelationsGet } from "../../services/knowledgeApi";
import type { KnowledgeRelations, RelatedEntry } from "../../types/generated";

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
  <section class="knowledge-relations" aria-label="关联">
    <h3>关联</h3>
    <p v-if="loading" class="knowledge-relations-state">加载关联中</p>
    <p v-else-if="error" class="error-text" role="alert">{{ error }}</p>
    <p v-else-if="isEmpty" class="knowledge-relations-state">暂无关联</p>
    <template v-else-if="relations">
      <section v-if="relations.outgoing.length" class="relation-group">
        <h4>出链</h4>
        <ul>
          <li v-for="entry in relations.outgoing" :key="entry.id">
            <button type="button" @click="emit('openRelated', entry.id)">
              <span>{{ entryTitle(entry) }}</span>
              <small>{{ entry.summary }}</small>
              <small>{{ entry.occurrenceCount }} 次</small>
              <small v-if="entry.deletedAt">已在回收站</small>
            </button>
          </li>
        </ul>
      </section>

      <section v-if="relations.backlinks.length" class="relation-group">
        <h4>反向链接</h4>
        <ul>
          <li v-for="entry in relations.backlinks" :key="entry.id">
            <button type="button" @click="emit('openRelated', entry.id)">
              <span>{{ entryTitle(entry) }}</span>
              <small>{{ entry.summary }}</small>
              <small>{{ entry.occurrenceCount }} 次</small>
              <small v-if="entry.deletedAt">已在回收站</small>
            </button>
          </li>
        </ul>
      </section>

      <section v-if="relations.unresolved.length" class="relation-group">
        <h4>未解析链接</h4>
        <ul>
          <li
            v-for="entry in relations.unresolved"
            :key="`${entry.rawTarget}-${entry.occurrenceCount}`"
            class="unresolved-relation"
          >
            <span>{{ entry.rawTarget }}</span>
            <small>{{ entry.occurrenceCount }} 次</small>
          </li>
        </ul>
      </section>
    </template>
  </section>
</template>
