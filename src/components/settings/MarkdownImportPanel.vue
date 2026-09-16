<script setup lang="ts">
import { computed, ref } from "vue";

import {
  markdownImportCommit,
  markdownImportPreview,
} from "../../services/importApi";
import { useEntriesStore } from "../../stores/entries";
import { toErrorMessage } from "../../utils/errors";
import { trackPendingOperation } from "../../composables/usePendingOperations";
import type {
  MarkdownImportPreview,
  MarkdownImportReport,
} from "../../types/generated";
import ConfirmDialog from "../shared/ConfirmDialog.vue";
import SettingRow from "./SettingRow.vue";

const entriesStore = useEntriesStore();
const preview = ref<MarkdownImportPreview | null>(null);
const report = ref<MarkdownImportReport | null>(null);
const error = ref("");
const previewing = ref(false);
const confirming = ref(false);
const committing = ref(false);

const previewPaths = computed(() => preview.value?.paths.slice(0, 100) ?? []);
const hasMorePaths = computed(
  () => (preview.value?.paths.length ?? 0) > previewPaths.value.length,
);
const confirmMessage = computed(() => {
  if (!preview.value) return "";
  return `将导入 ${preview.value.fileCount} 个 Markdown 文件，创建新条目，不会覆盖现有内容。`;
});

async function selectDirectory() {
  previewing.value = true;
  error.value = "";
  report.value = null;
  try {
    preview.value = await markdownImportPreview();
    if (preview.value) {
      confirming.value = true;
    }
  } catch (cause) {
    error.value = toErrorMessage(cause, "预览导入失败");
  } finally {
    previewing.value = false;
  }
}

async function commit() {
  if (!preview.value || committing.value) return;

  committing.value = true;
  error.value = "";
  try {
    report.value = await trackPendingOperation(
      markdownImportCommit(preview.value.sessionId),
    );
    confirming.value = false;
    preview.value = null;
    entriesStore.noteExternalChange();
    await Promise.all([entriesStore.load(), entriesStore.refreshTags()]);
  } catch (cause) {
    // Commit consumes the server-side session before it starts work. A retry with
    // this preview would always fail, so reset it and require a fresh preview.
    confirming.value = false;
    preview.value = null;
    error.value = toErrorMessage(cause, "导入失败");
  } finally {
    committing.value = false;
  }
}

function cancelConfirm() {
  confirming.value = false;
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${formatNumber(bytes / 1024)} KB`;
  return `${formatNumber(bytes / (1024 * 1024))} MB`;
}

function formatNumber(value: number) {
  return Number.isInteger(value) ? String(value) : value.toFixed(1);
}
</script>

<template>
  <section id="import" class="mb-8 scroll-mt-4">
    <h2
      class="m-0 border-b border-border-strong pb-2 text-title text-text-primary"
    >
      导入
    </h2>
    <SettingRow
      title="Markdown 导入"
      description="选择 Markdown 目录后先预览导入内容"
    >
      <button
        type="button"
        class="btn-primary"
        aria-label="选择 Markdown 目录导入"
        :disabled="previewing || committing"
        @click="selectDirectory"
      >
        {{ previewing ? "正在预览…" : "选择目录" }}
      </button>
    </SettingRow>

    <div
      v-if="preview"
      class="border-b border-border px-3 py-3 text-ui text-text-secondary"
    >
      <p class="m-0">
        已选择 {{ preview.fileCount }} 个文件，共
        {{ formatBytes(preview.totalBytes) }}
      </p>
      <p class="mb-0 mt-1">将创建新条目，不会覆盖现有内容。</p>
      <ul
        v-if="previewPaths.length"
        class="mb-0 mt-2 max-h-48 overflow-auto pl-5"
      >
        <li v-for="path in previewPaths" :key="path">{{ path }}</li>
      </ul>
      <p v-if="hasMorePaths" class="mb-0 mt-2">仅显示前 100 个路径。</p>
      <ul v-if="preview.warnings.length" class="mb-0 mt-2 pl-5 text-warning">
        <li v-for="warning in preview.warnings" :key="warning">
          {{ warning }}
        </li>
      </ul>
    </div>

    <p
      v-if="report"
      class="mt-2 text-ui text-success"
      role="status"
      aria-live="polite"
    >
      导入完成：成功 {{ report.importedCount }}，跳过
      {{ report.skippedCount }}，失败
      {{ report.failedCount }}
    </p>
    <ul
      v-if="report?.failures.length"
      class="mt-2 pl-5 text-ui text-danger"
      role="alert"
    >
      <li v-for="failure in report.failures" :key="failure">{{ failure }}</li>
    </ul>
    <p v-if="error" class="mt-2 text-ui text-danger" role="alert">
      {{ error }}
    </p>

    <ConfirmDialog
      :open="confirming"
      title="确认导入 Markdown"
      :message="confirmMessage"
      confirm-label="确认导入"
      @cancel="cancelConfirm"
      @confirm="commit"
    />
  </section>
</template>
