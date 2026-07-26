<script setup lang="ts">
import { isTauri } from "@tauri-apps/api/core";
import { computed, onMounted, ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";

import AppShell from "../components/layout/AppShell.vue";
import QuickCapture from "../components/quick-capture/QuickCapture.vue";
import { useSettingsStore } from "../stores/settings";

const windowLabel = ref("main");
// One-time read from URL search params; Tauri apps don't navigate via URL changes.
const isQuickCaptureView =
  new URLSearchParams(window.location.search).get("view") === "quick-capture";

const isQuickCapture = computed(
  () => windowLabel.value === "quick-capture" || isQuickCaptureView,
);

const settingsStore = useSettingsStore();

onMounted(() => {
  if (isTauri()) {
    windowLabel.value = getCurrentWindow().label;
  }

  // Main window only: settings_get is not available / not desired in Quick Capture.
  if (!isQuickCapture.value) {
    void settingsStore.ensureLoaded();
  }
});
</script>

<template>
  <QuickCapture v-if="isQuickCapture" />
  <AppShell v-else />
</template>
