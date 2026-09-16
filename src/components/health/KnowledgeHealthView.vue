<script setup lang="ts">
import { AlertCircle, RefreshCw } from "lucide-vue-next";
import { computed, ref, watch } from "vue";

import {
  knowledgeHealthIssuesGet,
  knowledgeHealthSummaryGet,
} from "../../services/healthApi";
import type {
  HealthIssueKind,
  HealthIssuePage,
  KnowledgeHealthSummary,
} from "../../types/generated";
import EmptyState from "../shared/EmptyState.vue";
import { PAGE_SIZE } from "../../constants/limits";
import { toErrorMessage } from "../../utils/errors";

const props = defineProps<{
  invalidatedToken: number;
}>();

const emit = defineEmits<{
  openEntry: [id: string];
}>();

const categories: Array<{
  kind: HealthIssueKind;
  label: string;
  description: string;
}> = [
  {
    kind: "unresolved_link",
    label: "未解析链接",
    description: "正文中存在尚未对应知识条目的 WikiLink",
  },
  {
    kind: "orphan_knowledge",
    label: "孤立知识",
    description: "没有链接关系和层级关系的知识条目",
  },
  {
    kind: "untagged_knowledge",
    label: "无标签知识",
    description: "尚未添加标签的知识条目",
  },
  {
    kind: "stale_capture",
    label: "陈旧收集",
    description: "超过 30 天仍待处理的收集条目",
  },
];

const summary = ref<KnowledgeHealthSummary | null>(null);
const selectedKind = ref<HealthIssueKind | null>(null);
const page = ref<HealthIssuePage | null>(null);
const summaryLoading = ref(false);
const issuesLoading = ref(false);
const error = ref<string | null>(null);
const stale = ref(false);
let summaryRequestId = 0;
let issuesRequestId = 0;

const selectedCategory = computed(() =>
  categories.find((category) => category.kind === selectedKind.value),
);

function categoryCount(kind: HealthIssueKind) {
  if (!summary.value) return 0;
  const counts: Record<HealthIssueKind, number> = {
    unresolved_link: summary.value.unresolvedLink,
    orphan_knowledge: summary.value.orphanKnowledge,
    untagged_knowledge: summary.value.untaggedKnowledge,
    stale_capture: summary.value.staleCapture,
  };
  return counts[kind];
}

async function loadSummary() {
  const requestId = ++summaryRequestId;
  summaryLoading.value = true;
  error.value = null;
  try {
    const next = await knowledgeHealthSummaryGet();
    if (requestId === summaryRequestId) {
      summary.value = next;
      stale.value = false;
    }
  } catch (cause) {
    if (requestId === summaryRequestId) {
      error.value = toErrorMessage(cause, "健康报告加载失败");
    }
  } finally {
    if (requestId === summaryRequestId) summaryLoading.value = false;
  }
}

async function loadIssues(offset = 0, append = false) {
  if (!selectedKind.value) return;
  const kind = selectedKind.value;
  const requestId = ++issuesRequestId;
  issuesLoading.value = true;
  error.value = null;
  try {
    const next = await knowledgeHealthIssuesGet(kind, {
      limit: PAGE_SIZE,
      offset,
    });
    if (requestId === issuesRequestId && selectedKind.value === kind) {
      page.value =
        append && page.value
          ? { ...next, items: [...page.value.items, ...next.items] }
          : next;
    }
  } catch (cause) {
    if (requestId === issuesRequestId && selectedKind.value === kind) {
      error.value = toErrorMessage(cause, "健康问题加载失败");
    }
  } finally {
    if (requestId === issuesRequestId) issuesLoading.value = false;
  }
}

function selectCategory(kind: HealthIssueKind) {
  selectedKind.value = kind;
  page.value = null;
  void loadIssues();
}

function refresh() {
  void loadSummary();
  if (selectedKind.value) void loadIssues(page.value?.offset ?? 0);
}

watch(
  () => props.invalidatedToken,
  () => {
    summaryRequestId += 1;
    summaryLoading.value = false;
    stale.value = true;
  },
);

void loadSummary();
</script>

<template>
  <section
    class="h-full overflow-auto bg-bg-secondary px-5 py-5 lg:px-8 lg:py-7"
  >
    <div class="mx-auto grid w-full max-w-[1080px] gap-5">
      <header class="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 class="m-0 text-display text-text-primary">知识健康</h1>
          <p class="mb-0 mt-1 text-ui text-text-secondary">
            发现需要整理、补全或连接的知识条目。
          </p>
        </div>
        <button class="btn-secondary text-ui" type="button" @click="refresh">
          <RefreshCw :size="15" aria-hidden="true" />
          刷新
        </button>
      </header>

      <p v-if="stale" class="m-0 text-caption text-text-secondary">
        数据已变化，点击刷新查看最新报告。
      </p>
      <p v-if="error" class="m-0 text-ui text-danger" role="alert">
        {{ error }}
      </p>

      <div
        v-if="summaryLoading && !summary"
        class="py-8 text-ui text-text-secondary"
        role="status"
        aria-busy="true"
      >
        加载健康报告
      </div>
      <div v-else class="grid gap-3 sm:grid-cols-2">
        <button
          v-for="category in categories"
          :key="category.kind"
          class="elevation-panel grid gap-1 rounded-lg bg-bg-elevated p-4 text-left outline-none ring-focus hover:bg-selected"
          :class="
            selectedKind === category.kind
              ? 'border border-brand/40'
              : 'border border-transparent'
          "
          :data-health-kind="category.kind"
          type="button"
          @click="selectCategory(category.kind)"
        >
          <span class="text-title text-text-primary">{{ category.label }}</span>
          <span class="text-display tabular-nums text-brand">{{
            categoryCount(category.kind)
          }}</span>
          <span class="text-caption text-text-secondary">{{
            category.description
          }}</span>
        </button>
      </div>

      <section
        v-if="selectedCategory"
        class="elevation-panel rounded-lg bg-bg-elevated"
      >
        <header class="border-b border-border px-5 py-4">
          <h2 class="m-0 text-title text-text-primary">
            {{ selectedCategory.label }}
          </h2>
          <p class="mb-0 mt-1 text-caption text-text-secondary">
            {{ selectedCategory.description }}
          </p>
        </header>
        <div
          v-if="issuesLoading && !page"
          class="px-5 py-6 text-ui text-text-secondary"
          role="status"
          aria-busy="true"
        >
          加载问题
        </div>
        <EmptyState
          v-else-if="page && !page.items.length"
          :icon="AlertCircle"
          title="暂无问题"
          description="这类知识状态良好。"
          size="compact"
        />
        <ul v-else-if="page" class="m-0 divide-y divide-border p-0">
          <li
            v-for="issue in page.items"
            :key="`${issue.entryId}-${issue.rawTarget ?? ''}`"
          >
            <button
              class="grid w-full gap-1 px-5 py-4 text-left outline-none ring-focus hover:bg-bg-secondary"
              type="button"
              @click="emit('openEntry', issue.entryId)"
            >
              <span class="text-ui font-medium text-text-primary">{{
                issue.title || "无标题"
              }}</span>
              <span
                v-if="issue.rawTarget"
                class="text-caption text-text-secondary"
              >
                [[{{ issue.rawTarget }}]] · {{ issue.occurrenceCount }} 处
              </span>
              <span v-else class="text-caption text-text-secondary"
                >更新于 {{ issue.updatedAt }}</span
              >
            </button>
          </li>
        </ul>
        <footer v-if="page?.hasMore" class="border-t border-border p-3">
          <button
            class="btn-secondary w-full text-ui"
            type="button"
            :disabled="issuesLoading"
            @click="loadIssues(page.offset + page.limit, true)"
          >
            {{ issuesLoading ? "加载中" : "加载更多" }}
          </button>
        </footer>
      </section>
    </div>
  </section>
</template>
