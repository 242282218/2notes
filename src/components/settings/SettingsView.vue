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
import { useEntriesStore } from "../../stores/entries";
import { useSettingsStore } from "../../stores/settings";
import type { BackupInfo } from "../../types/generated";
import ConfirmDialog from "../shared/ConfirmDialog.vue";
import IconButton from "../shared/IconButton.vue";

const settingsStore = useSettingsStore();
const entriesStore = useEntriesStore();
const exportMessage = ref("");
const exportError = ref("");
const backupMessage = ref("");
const backupError = ref("");
const backupBusy = ref(false);
const backups = ref<BackupInfo[]>([]);
const restoreConfirm = ref(false);
const pendingRestoreBackup = ref<BackupInfo | null>(null);

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
    await backupsRestore(backup.path);
    backupMessage.value = `已恢复备份：${backup.fileName}`;
    await loadBackups();
    await entriesStore.load();
    await entriesStore.refreshTags();
  } catch (error) {
    backupError.value = error instanceof Error ? error.message : "恢复备份失败";
  } finally {
    backupBusy.value = false;
    pendingRestoreBackup.value = null;
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
  <section class="settings-view">
    <h1>设置</h1>
    <div v-if="settingsStore.error" class="error-text">
      {{ settingsStore.error }}
    </div>
    <template v-if="settingsStore.settings">
      <div class="settings-row">
        <div>
          <strong>全局快捷键</strong>
          <span>{{ settingsStore.settings.shortcut }}</span>
        </div>
        <span
          class="status-chip"
          :class="{ failed: !settingsStore.settings.shortcutRegistered }"
        >
          {{
            settingsStore.settings.shortcutRegistered ? "已注册" : "注册失败"
          }}
        </span>
      </div>
      <p v-if="settingsStore.settings.shortcutError" class="error-text">
        {{ settingsStore.settings.shortcutError }}
      </p>

      <label class="settings-row">
        <div>
          <strong>开机自启动</strong>
          <span>随 Windows 会话启动</span>
        </div>
        <input
          type="checkbox"
          :checked="settingsStore.settings?.autostartEnabled ?? false"
          @change="updateAutostart"
        />
      </label>

      <div class="settings-row">
        <div>
          <strong>数据目录</strong>
          <span>{{ settingsStore.settings.dataDir }}</span>
        </div>
        <IconButton
          label="打开数据目录"
          :icon="FolderOpen"
          @click="openDir(settingsStore.settings.dataDir)"
        />
      </div>

      <div class="settings-row">
        <div>
          <strong>日志目录</strong>
          <span>{{ settingsStore.settings.logDir }}</span>
        </div>
        <IconButton
          label="打开日志目录"
          :icon="ExternalLink"
          @click="openDir(settingsStore.settings.logDir)"
        />
      </div>

      <div class="settings-row">
        <div>
          <strong>备份目录</strong>
          <span>{{ settingsStore.settings.backupDir }}</span>
        </div>
        <IconButton
          label="打开备份目录"
          :icon="FolderOpen"
          @click="openDir(settingsStore.settings.backupDir)"
        />
      </div>

      <div class="settings-row">
        <div>
          <strong>本地备份</strong>
          <span>创建当前 SQLite 快照，恢复前会自动再备份一次</span>
        </div>
        <button
          type="button"
          class="secondary-button"
          :disabled="backupBusy"
          @click="createManualBackup"
        >
          立即备份
        </button>
      </div>
      <div v-if="backups.length" class="backup-list">
        <div
          v-for="backup in backups"
          :key="backup.fileName"
          class="backup-item"
        >
          <span>{{ backup.fileName }}</span>
          <button
            type="button"
            class="secondary-button"
            :disabled="backupBusy"
            @click="promptRestore(backup)"
          >
            恢复
          </button>
        </div>
      </div>
      <p v-if="backupMessage" class="success-text">
        {{ backupMessage }}
      </p>
      <p v-if="backupError" class="error-text">
        {{ backupError }}
      </p>

      <div class="settings-row">
        <div>
          <strong>Markdown 导出</strong>
          <span>导出未进入回收站的条目</span>
        </div>
        <button type="button" class="primary-button" @click="chooseExportDir">
          <Download :size="16" />
          导出
        </button>
      </div>
      <p v-if="exportMessage" class="success-text">
        {{ exportMessage }}
      </p>
      <p v-if="exportError" class="error-text">
        {{ exportError }}
      </p>

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
  </section>
</template>
