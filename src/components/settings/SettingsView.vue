<script setup lang="ts">
import { Download, ExternalLink, FolderOpen } from "lucide-vue-next";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { onMounted, ref } from "vue";

import { exportMarkdown } from "../../services/exportApi";
import { useSettingsStore } from "../../stores/settings";
import IconButton from "../shared/IconButton.vue";

const settingsStore = useSettingsStore();
const exportMessage = ref("");
const exportError = ref("");

onMounted(() => {
  void settingsStore.load();
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
</script>

<template>
  <section class="settings-view">
    <h1>设置</h1>
    <div
      v-if="settingsStore.error"
      class="error-text"
    >
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
          {{ settingsStore.settings.shortcutRegistered ? "已注册" : "注册失败" }}
        </span>
      </div>
      <p
        v-if="settingsStore.settings.shortcutError"
        class="error-text"
      >
        {{ settingsStore.settings.shortcutError }}
      </p>

      <label class="settings-row">
        <div>
          <strong>开机自启动</strong>
          <span>随 Windows 会话启动</span>
        </div>
        <input
          type="checkbox"
          :checked="settingsStore.settings.autostartEnabled"
          @change="settingsStore.setAutostart(($event.target as HTMLInputElement).checked)"
        >
      </label>

      <div class="settings-row">
        <div>
          <strong>数据目录</strong>
          <span>{{ settingsStore.settings.dataDir }}</span>
        </div>
        <IconButton
          label="打开数据目录"
          :icon="FolderOpen"
          @click="openPath(settingsStore.settings.dataDir)"
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
          @click="openPath(settingsStore.settings.logDir)"
        />
      </div>

      <div class="settings-row">
        <div>
          <strong>Markdown 导出</strong>
          <span>导出全部未永久删除条目</span>
        </div>
        <button
          type="button"
          class="primary-button"
          @click="chooseExportDir"
        >
          <Download :size="16" />
          导出
        </button>
      </div>
      <p
        v-if="exportMessage"
        class="success-text"
      >
        {{ exportMessage }}
      </p>
      <p
        v-if="exportError"
        class="error-text"
      >
        {{ exportError }}
      </p>
    </template>
  </section>
</template>
