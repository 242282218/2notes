<script setup lang="ts">
import { useSettingsStore } from "../../stores/settings";
import ThemeToggle from "../shared/ThemeToggle.vue";
import SettingRow from "./SettingRow.vue";

const settingsStore = useSettingsStore();

async function onThemeChange(mode: "system" | "light" | "dark") {
  try {
    await settingsStore.setThemeMode(mode);
  } catch {
    // Error is surfaced in settingsStore.error.
  }
}
</script>

<template>
  <section class="mb-8 scroll-mt-4">
    <h2
      class="m-0 border-b border-border-strong pb-2 text-title text-text-primary"
    >
      外观
    </h2>
    <SettingRow title="主题" description="选择浅色、深色或跟随系统设置">
      <ThemeToggle
        :disabled="settingsStore.themeSaving"
        @change="onThemeChange"
      />
    </SettingRow>
  </section>
</template>
