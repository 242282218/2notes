<script setup lang="ts">
import { Download, ExternalLink, FolderOpen } from "lucide-vue-next";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { computed, onMounted, ref } from "vue";

import {
  backupsCreate,
  backupsList,
  backupsRestore,
} from "../../services/backupApi";
import { exportMarkdown } from "../../services/exportApi";
import { knowledgeRebuildIndex } from "../../services/knowledgeApi";
import { useEntriesStore } from "../../stores/entries";
import { useSettingsStore } from "../../stores/settings";
import type { BackupInfo, KnowledgeIndexReport } from "../../types/generated";
import AppearanceSettings from "./AppearanceSettings.vue";
import ConfirmDialog from "../shared/ConfirmDialog.vue";
import IconButton from "../shared/IconButton.vue";

const settingsStore = useSettingsStore();

const categories = [
  { id: "appearance", label: "外观" },
  { id: "shortcut", label: "快捷键" },
  { id: "directories", label: "目录" },
  { id: "backup", label: "备份" },
  { id: "knowledge", label: "知识索引" },
  { id: "export", label: "导出" },
];

function scrollToSection(id: string) {
  const element = document.getElementById(id);
  if (element) {
    element.scrollIntoView({ behavior: "smooth", block: "start" });
  }
}
const entriesStore = useEntriesStore();
const exportMessage = ref("");
const exportError = ref("");
const backupMessage = ref("");
const backupError = ref("");
const backupBusy = ref(false);
const backups = ref<BackupInfo[]>([]);
const restoreConfirm = ref(false);
const pendingRestoreBackup = ref<BackupInfo | null>(null);
const indexBusy = ref(false);
const indexReport = ref<KnowledgeIndexReport | null>(null);
const indexError = ref("");

const restoreMessage = computed(() =>
  pendingRestoreBackup.value
    ? `恢复备份 ${pendingRestoreBackup.value.fileName}？当前数据库会先创建恢复前快照。`
    : "",
);

onMounted(() => {
  void settingsStore.load();
  void loadBackups();
});

async function chooseExportDir() {
  exportMessage.value = "";
  exportError.value = "";
  const selected = await openDialog({
    directory: true,
    multiple: false,
    title: "选择 Markdown 导出目录",
  });
  if (typeof selected !== "string") {
    return;
  }
  try {
    const result = await exportMarkdown(selected);
    exportMessage.value = `已导出 ${result.exportedCount} 个文件`;
  } catch (error) {
    exportError.value = error instanceof Error ? error.message : "导出失败";
  }
}

async function openDir(path: string) {
  try {
    await openPath(path);
  } catch (error) {
    backupError.value = error instanceof Error ? error.message : "打开目录失败";
  }
}

async function loadBackups() {
  try {
    backups.value = await backupsList();
  } catch (error) {
    backupError.value =
      error instanceof Error ? error.message : "备份列表加载失败";
  }
}

async function createManualBackup() {
  backupBusy.value = true;
  backupMessage.value = "";
  backupError.value = "";
  try {
    const backup = await backupsCreate("manual");
    backupMessage.value = `已创建备份：${backup.fileName}`;
    await loadBackups();
  } catch (error) {
    backupError.value = error instanceof Error ? error.message : "创建备份失败";
  } finally {
    backupBusy.value = false;
  }
}

function promptRestore(backup: BackupInfo) {
  pendingRestoreBackup.value = backup;
  restoreConfirm.value = true;
}

async function confirmRestore() {
  if (!pendingRestoreBackup.value) return;
  const backup = pendingRestoreBackup.value;
  restoreConfirm.value = false;
  backupBusy.value = true;
  backupMessage.value = "";
  backupError.value = "";
  try {
    try {
      await backupsRestore(backup.path);
    } catch (error) {
      backupError.value =
        error instanceof Error ? error.message : "恢复备份失败";
      return;
    }
    backupMessage.value = `已恢复备份：${backup.fileName}`;
    entriesStore.noteExternalChange();
    try {
      await Promise.all([
        loadBackups(),
        entriesStore.load(),
        entriesStore.refreshTags(),
      ]);
      if (entriesStore.error) {
        throw new Error(entriesStore.error);
      }
    } catch {
      backupError.value = "恢复成功，但界面刷新失败";
    }
  } finally {
    backupBusy.value = false;
    pendingRestoreBackup.value = null;
  }
}

async function rebuildKnowledgeIndexes() {
  indexBusy.value = true;
  indexReport.value = null;
  indexError.value = "";
  try {
    indexReport.value = await knowledgeRebuildIndex();
    entriesStore.noteExternalChange();
  } catch (error) {
    indexError.value = error instanceof Error ? error.message : "索引重建失败";
  } finally {
    indexBusy.value = false;
  }
}

async function updateAutostart(event: Event) {
  const target = event.target as HTMLInputElement;
  try {
    await settingsStore.setAutostart(target.checked);
  } catch {
    target.checked = settingsStore.settings?.autostartEnabled ?? false;
  }
}
</script>

<template>
  <section class="grid max-w-[1100px] gap-6 p-6 md:grid-cols-[180px_minmax(0,860px)]">
    <nav class="flex flex-row flex-wrap gap-1 rounded-lg border border-border bg-bg-elevated p-2 shadow-sm md:sticky md:top-4 md:h-fit md:flex-col" aria-label="设置分类">
      <button
        v-for="category in categories"
        :key="category.id"
        type="button"
        class="flex h-[34px] items-center rounded-md border-none bg-transparent px-3 text-left text-[14px] text-text-secondary transition-colors duration-150 ring-focus hover:bg-bg-hover hover:text-text-primary"
        @click="scrollToSection(category.id)"
      >
        {{ category.label }}
      </button>
    </nav>

    <div>
      <h1 class="mb-5 text-[26px] font-bold tracking-tight text-text-primary">设置</h1>
      <div v-if="settingsStore.error" class="text-danger mb-4">
        {{ settingsStore.error }}
      </div>
      <template v-if="settingsStore.settings">
        <AppearanceSettings id="appearance" />

        <section id="shortcut" class="mb-6">
          <h2 class="mb-4 text-[18px] font-semibold text-text-primary">快捷键</h2>
          <div class="flex min-h-[64px] items-center justify-between gap-4 border-b border-border py-2">
            <div class="grid min-w-0 gap-1">
              <strong class="font-medium text-text-primary">全局快捷键</strong>
              <span class="break-words text-[13px] leading-relaxed text-text-tertiary">{{ settingsStore.settings.shortcut }}</span>
            </div>
            <span
              class="inline-flex min-h-[22px] items-center overflow-hidden text-ellipsis whitespace-nowrap rounded-full px-2 text-[12px]"
              :class="settingsStore.settings.shortcutRegistered ? 'bg-success-subtle text-success' : 'bg-danger/10 text-danger'"
            >
              {{
                settingsStore.settings.shortcutRegistered
                  ? "已注册"
                  : "注册失败"
              }}
            </span>
          </div>
          <p v-if="settingsStore.settings.shortcutError" class="text-danger mt-2">
            {{ settingsStore.settings.shortcutError }}
          </p>

          <label class="flex min-h-[64px] cursor-pointer items-center justify-between gap-4 border-b border-border py-2">
            <div class="grid min-w-0 gap-1">
              <strong class="font-medium text-text-primary">开机自启动</strong>
              <span class="break-words text-[13px] leading-relaxed text-text-tertiary">随 Windows 会话启动</span>
            </div>
            <input
              type="checkbox"
              class="m-0 size-5 cursor-pointer rounded-sm border border-border-strong bg-bg-elevated accent-brand ring-focus"
              :checked="settingsStore.settings?.autostartEnabled ?? false"
              @change="updateAutostart"
            />
          </label>
        </section>

        <section id="directories" class="mb-6">
          <h2 class="mb-4 text-[18px] font-semibold text-text-primary">目录</h2>
          <div class="flex min-h-[64px] items-center justify-between gap-4 border-b border-border py-2">
            <div class="grid min-w-0 gap-1">
              <strong class="font-medium text-text-primary">数据目录</strong>
              <span class="break-words text-[13px] leading-relaxed text-text-tertiary">{{ settingsStore.settings.dataDir }}</span>
            </div>
            <IconButton
              label="打开数据目录"
              :icon="FolderOpen"
              @click="openDir(settingsStore.settings.dataDir)"
            />
          </div>

          <div class="flex min-h-[64px] items-center justify-between gap-4 border-b border-border py-2">
            <div class="grid min-w-0 gap-1">
              <strong class="font-medium text-text-primary">日志目录</strong>
              <span class="break-words text-[13px] leading-relaxed text-text-tertiary">{{ settingsStore.settings.logDir }}</span>
            </div>
            <IconButton
              label="打开日志目录"
              :icon="ExternalLink"
              @click="openDir(settingsStore.settings.logDir)"
            />
          </div>

          <div class="flex min-h-[64px] items-center justify-between gap-4 border-b border-border py-2">
            <div class="grid min-w-0 gap-1">
              <strong class="font-medium text-text-primary">备份目录</strong>
              <span class="break-words text-[13px] leading-relaxed text-text-tertiary">{{ settingsStore.settings.backupDir }}</span>
            </div>
            <IconButton
              label="打开备份目录"
              :icon="FolderOpen"
              @click="openDir(settingsStore.settings.backupDir)"
            />
          </div>
        </section>

        <section id="backup" class="mb-6">
          <h2 class="mb-4 text-[18px] font-semibold text-text-primary">备份</h2>
          <div class="flex min-h-[64px] items-center justify-between gap-4 border-b border-border py-2">
            <div class="grid min-w-0 gap-1">
              <strong class="font-medium text-text-primary">本地备份</strong>
              <span class="break-words text-[13px] leading-relaxed text-text-tertiary">创建当前 SQLite 快照，恢复前会自动再备份一次</span>
            </div>
            <button
              type="button"
              class="btn-secondary"
              :disabled="backupBusy"
              @click="createManualBackup"
            >
              立即备份
            </button>
          </div>
          <div v-if="backups.length" class="-mt-2 mb-3 grid gap-2">
            <div
              v-for="backup in backups"
              :key="backup.fileName"
              class="flex items-center justify-between gap-3 rounded-md border border-border bg-bg-secondary px-3 py-2 text-[13px] text-text-secondary transition-colors duration-150 hover:border-border-hover hover:bg-bg-hover"
            >
              <span>{{ backup.fileName }}</span>
              <button
                type="button"
                class="btn-secondary h-7 text-xs min-w-0"
                :disabled="backupBusy"
                @click="promptRestore(backup)"
              >
                恢复
              </button>
            </div>
          </div>
          <p v-if="backupMessage" class="text-success mt-2">
            {{ backupMessage }}
          </p>
          <p v-if="backupError" class="text-danger mt-2">
            {{ backupError }}
          </p>
        </section>

        <section id="knowledge" class="mb-6">
          <h2 class="mb-4 text-[18px] font-semibold text-text-primary">知识索引</h2>
          <div class="flex min-h-[64px] items-center justify-between gap-4 border-b border-border py-2">
            <div class="grid min-w-0 gap-1">
              <strong class="font-medium text-text-primary">重建索引</strong>
              <span class="break-words text-[13px] leading-relaxed text-text-tertiary">只重建搜索和关联索引，不修改条目正文</span>
            </div>
            <button
              type="button"
              class="btn-secondary"
              aria-label="重建知识索引"
              :disabled="indexBusy"
              @click="rebuildKnowledgeIndexes"
            >
              {{ indexBusy ? "重建中…" : "重建索引" }}
            </button>
          </div>
          <p v-if="indexReport" class="text-success mt-2">
            {{
              `重建完成：来源 ${indexReport.indexedSources}，链接 ${indexReport.linkOccurrences}，未解析 ${indexReport.unresolvedOccurrences}，${indexReport.searchIndexAvailable ? "搜索索引可用" : "搜索索引不可用"}`
            }}
          </p>
          <p v-if="indexError" class="text-danger mt-2" role="alert">
            {{ indexError }}
          </p>
        </section>

        <section id="export" class="mb-6">
          <h2 class="mb-4 text-[18px] font-semibold text-text-primary">导出</h2>
          <div class="flex min-h-[64px] items-center justify-between gap-4 border-b border-border py-2">
            <div class="grid min-w-0 gap-1">
              <strong class="font-medium text-text-primary">Markdown 导出</strong>
              <span class="break-words text-[13px] leading-relaxed text-text-tertiary">导出未进入回收站的条目</span>
            </div>
            <button
              type="button"
              class="btn-primary"
              @click="chooseExportDir"
            >
              <Download :size="16" />
              导出
            </button>
          </div>
          <p v-if="exportMessage" class="text-success mt-2">
            {{ exportMessage }}
          </p>
          <p v-if="exportError" class="text-danger mt-2">
            {{ exportError }}
          </p>
        </section>

        <ConfirmDialog
          :open="restoreConfirm"
          title="恢复备份"
          :message="restoreMessage"
          confirm-label="确认恢复"
          danger
          @cancel="restoreConfirm = false"
          @confirm="confirmRestore"
        />
      </template>
    </div>
  </section>
</template>
