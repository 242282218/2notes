<script setup lang="ts">
import { isTauri } from "@tauri-apps/api/core";
import { computed, onMounted, ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";

import AppShell from "../components/layout/AppShell.vue";
import QuickCapture from "../components/quick-capture/QuickCapture.vue";

const windowLabel = ref("main");
const isQuickCaptureView = new URLSearchParams(window.location.search).get("view") === "quick-capture";

onMounted(() => {
  if (isTauri()) {
    windowLabel.value = getCurrentWindow().label;
  }
});

const isQuickCapture = computed(
  () => windowLabel.value === "quick-capture" || isQuickCaptureView,
);
</script>

<template>
  <QuickCapture v-if="isQuickCapture" />
  <AppShell v-else />
</template>
