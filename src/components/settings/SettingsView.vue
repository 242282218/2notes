<script setup lang="ts">
import { Download, ExternalLink, FolderOpen } from "lucide-vue-next";
import { openPath } from "@tauri-apps/plugin-opener";
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";

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
import SettingRow from "./SettingRow.vue";

const settingsStore = useSettingsStore();

const categories = [
  { id: "appearance", label: "外观" },
  { id: "shortcut", label: "快捷键" },
  { id: "directories", label: "目录" },
  { id: "backup", label: "备份" },
  { id: "knowledge", label: "知识索引" },
  { id: "export", label: "导出" },
];

const activeCategory = ref(categories[0].id);
const settingsContentRef = ref<HTMLElement | null>(null);
let sectionObserver: {
  observe: (target: HTMLElement) => void;
  disconnect: () => void;
} | null = null;
let disposed = false;
let ignoreObserverUntil = 0;
let scrollLockTimer: number | null = null;

function getSection(id: string) {
  return settingsContentRef.value?.querySelector<HTMLElement>(`#${id}`) ?? null;
}

function scrollToSection(id: string) {
  activeCategory.value = id;
  ignoreObserverUntil = Date.now() + 300;
  if (scrollLockTimer !== null) {
    window.clearTimeout(scrollLockTimer);
  }
  scrollLockTimer = window.setTimeout(() => {
    scrollLockTimer = null;
    ignoreObserverUntil = 0;
  }, 300);
  getSection(id)?.scrollIntoView({ behavior: "smooth", block: "start" });
}

function observeSections() {
  sectionObserver?.disconnect();
  sectionObserver = null;
  if (typeof IntersectionObserver === "undefined") {
    return;
  }
  sectionObserver = new window.IntersectionObserver(
    (entries) => {
      if (Date.now() < ignoreObserverUntil) {
        return;
      }
      const visible = entries
        .filter((entry) => entry.isIntersecting)
        .sort((left, right) => right.intersectionRatio - left.intersectionRatio);
      const id = visible[0]?.target.id;
      if (id) {
        activeCategory.value = id;
      }
    },
    { rootMargin: "-10% 0px -65% 0px", threshold: [0, 0.25, 0.5, 0.75] },
  );
  for (const category of categories) {
    const section = getSection(category.id);
    if (section) {
      sectionObserver.observe(section);
    }
  }
}

const entriesStore = useEntriesStore();
const exportMessage = ref("");
const exportError = ref("");
const exportBusy = ref(false);
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

onMounted(async () => {
  void loadBackups();
  await settingsStore.ensureLoaded();
  if (disposed) return;
  await nextTick();
  if (disposed) return;
  observeSections();
});

onBeforeUnmount(() => {
  disposed = true;
  if (scrollLockTimer !== null) {
    window.clearTimeout(scrollLockTimer);
    scrollLockTimer = null;
  }
  sectionObserver?.disconnect();
  sectionObserver = null;
});

async function chooseExportDir() {
  exportMessage.value = "";
  exportError.value = "";
  exportBusy.value = true;
  try {
    const result = await exportMarkdown();
    if (result == null) {
      return;
    }
    exportMessage.value = `已导出 ${result.exportedCount} 个文件`;
  } catch (error) {
    exportError.value = error instanceof Error ? error.message : "导出失败";
  } finally {
    exportBusy.value = false;
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
  <section
    class="grid w-full max-w-[1060px] grid-cols-[164px_minmax(0,1fr)] gap-8 p-6 max-[700px]:grid-cols-[132px_minmax(0,1fr)] max-[700px]:gap-5 max-[700px]:p-4"
  >
    <nav
      class="sticky top-4 flex self-start flex-col gap-0.5 border-l border-border p-1"
      aria-label="设置分类"
    >
      <button
        v-for="category in categories"
        :key="category.id"
        type="button"
        class="ring-focus h-9 rounded-md border-0 bg-transparent px-3 text-left text-ui text-text-secondary transition-[color,background-color] duration-fast ease-token hover:bg-bg-hover hover:text-text-primary"
        :class="{
          'bg-selected text-text-primary': activeCategory === category.id,
        }"
        :data-category="category.id"
        :aria-current="activeCategory === category.id ? 'location' : undefined"
        @click="scrollToSection(category.id)"
      >
        {{ category.label }}
      </button>
    </nav>

    <main ref="settingsContentRef" class="min-w-0 max-w-[820px]">
      <h1 class="mb-6 mt-0 text-title text-text-primary">设置</h1>
      <p
        v-if="settingsStore.loading && !settingsStore.settings"
        class="mt-2 text-ui text-text-secondary"
        role="status"
        aria-live="polite"
      >
        正在加载设置…
      </p>
      <p
        v-if="settingsStore.error"
        class="mt-2 text-ui text-danger"
        role="alert"
      >
        {{ settingsStore.error }}
      </p>
      <template v-if="settingsStore.settings">
        <AppearanceSettings id="appearance" />

        <section id="shortcut" class="mb-8 scroll-mt-4">
          <h2 class="m-0 border-b border-border-strong pb-2 text-title text-text-primary">
            快捷键
          </h2>
          <SettingRow
            title="全局快捷键"
            :description="settingsStore.settings.shortcut"
          >
            <span
              class="whitespace-nowrap rounded-sm px-2 py-1 text-caption font-semibold"
              :class="
                settingsStore.settings.shortcutRegistered
                  ? 'bg-success-subtle text-success'
                  : 'bg-danger/10 text-danger'
              "
            >
              {{ settingsStore.settings.shortcutRegistered ? "已注册" : "注册失败" }}
            </span>
          </SettingRow>
          <p
            v-if="settingsStore.settings.shortcutError"
            class="mt-2 text-ui text-danger"
            role="alert"
          >
            {{ settingsStore.settings.shortcutError }}
          </p>

          <SettingRow
            as="label"
            class="cursor-pointer"
            title="开机自启动"
            :description="
              settingsStore.autostartSaving ? '正在更新…' : '随 Windows 会话启动'
            "
          >
            <input
              type="checkbox"
              class="ring-focus mr-2 size-5 accent-brand"
              :checked="settingsStore.settings?.autostartEnabled ?? false"
              :disabled="settingsStore.autostartSaving"
              :aria-busy="settingsStore.autostartSaving ? 'true' : undefined"
              @change="updateAutostart"
            />
          </SettingRow>
        </section>

        <section id="directories" class="mb-8 scroll-mt-4">
          <h2 class="m-0 border-b border-border-strong pb-2 text-title text-text-primary">
            目录
          </h2>
          <SettingRow title="数据目录">
            <template #description>
              <span class="block truncate" :title="settingsStore.settings.dataDir">
                {{ settingsStore.settings.dataDir }}
              </span>
            </template>
            <IconButton
              label="打开数据目录"
              :icon="FolderOpen"
              @click="openDir(settingsStore.settings.dataDir)"
            />
          </SettingRow>

          <SettingRow title="日志目录">
            <template #description>
              <span class="block truncate" :title="settingsStore.settings.logDir">
                {{ settingsStore.settings.logDir }}
              </span>
            </template>
            <IconButton
              label="打开日志目录"
              :icon="ExternalLink"
              @click="openDir(settingsStore.settings.logDir)"
            />
          </SettingRow>

          <SettingRow title="备份目录">
            <template #description>
              <span class="block truncate" :title="settingsStore.settings.backupDir">
                {{ settingsStore.settings.backupDir }}
              </span>
            </template>
            <IconButton
              label="打开备份目录"
              :icon="FolderOpen"
              @click="openDir(settingsStore.settings.backupDir)"
            />
          </SettingRow>
        </section>

        <section id="backup" class="mb-8 scroll-mt-4">
          <h2 class="m-0 border-b border-border-strong pb-2 text-title text-text-primary">
            备份
          </h2>
          <SettingRow
            title="本地备份"
            description="创建当前 SQLite 快照，恢复前会自动再备份一次"
          >
            <button
              type="button"
              class="btn-secondary"
              :disabled="backupBusy"
              @click="createManualBackup"
            >
              {{ backupBusy ? "处理中…" : "立即备份" }}
            </button>
          </SettingRow>
          <div
            v-if="backups.length"
            class="border-b border-border"
            aria-label="可用备份"
          >
            <div
              v-for="backup in backups"
              :key="backup.fileName"
              class="grid min-h-11 grid-cols-[minmax(0,1fr)_auto] items-center gap-3 px-0 py-2 pl-3 text-ui text-text-secondary not-first:border-t not-first:border-border"
            >
              <span class="block truncate" :title="backup.fileName">{{
                backup.fileName
              }}</span>
              <button
                type="button"
                class="btn-secondary h-7 min-w-0 text-caption"
                :disabled="backupBusy"
                @click="promptRestore(backup)"
              >
                恢复
              </button>
            </div>
          </div>
          <p
            v-if="backupMessage"
            class="mt-2 text-ui text-success"
            role="status"
            aria-live="polite"
          >
            {{ backupMessage }}
          </p>
          <p
            v-if="backupError"
            class="mt-2 text-ui text-danger"
            role="alert"
          >
            {{ backupError }}
          </p>
        </section>

        <section id="knowledge" class="mb-8 scroll-mt-4">
          <h2 class="m-0 border-b border-border-strong pb-2 text-title text-text-primary">
            知识索引
          </h2>
          <SettingRow
            title="重建索引"
            description="只重建搜索和关联索引，不修改条目正文"
          >
            <button
              type="button"
              class="btn-secondary"
              aria-label="重建知识索引"
              :disabled="indexBusy"
              @click="rebuildKnowledgeIndexes"
            >
              {{ indexBusy ? "重建中…" : "重建索引" }}
            </button>
          </SettingRow>
          <p
            v-if="indexReport"
            class="mt-2 text-ui text-success"
            role="status"
            aria-live="polite"
          >
            {{
              `重建完成：来源 ${indexReport.indexedSources}，链接 ${indexReport.linkOccurrences}，未解析 ${indexReport.unresolvedOccurrences}，${indexReport.searchIndexAvailable ? "搜索索引可用" : "搜索索引不可用"}`
            }}
          </p>
          <p
            v-if="indexError"
            class="mt-2 text-ui text-danger"
            role="alert"
          >
            {{ indexError }}
          </p>
        </section>

        <section id="export" class="mb-8 scroll-mt-4">
          <h2 class="m-0 border-b border-border-strong pb-2 text-title text-text-primary">
            导出
          </h2>
          <SettingRow
            title="Markdown 导出"
            description="导出未进入回收站的条目"
          >
            <button
              type="button"
              class="btn-primary"
              :disabled="exportBusy"
              @click="chooseExportDir"
            >
              <Download :size="16" />
              {{ exportBusy ? "导出中…" : "导出" }}
            </button>
          </SettingRow>
          <p
            v-if="exportMessage"
            class="mt-2 text-ui text-success"
            role="status"
            aria-live="polite"
          >
            {{ exportMessage }}
          </p>
          <p
            v-if="exportError"
            class="mt-2 text-ui text-danger"
            role="alert"
          >
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
    </main>
  </section>
</template>
