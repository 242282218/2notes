import { defineStore } from "pinia";
import { ref } from "vue";

import { settingsGet, settingsUpdate } from "../services/settingsApi";
import type { AppSettings } from "../types/generated";

export const useSettingsStore = defineStore("settings", () => {
  const settings = ref<AppSettings | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function load() {
    loading.value = true;
    error.value = null;
    try {
      settings.value = await settingsGet();
    } catch (loadError) {
      error.value = loadError instanceof Error ? loadError.message : "设置加载失败";
    } finally {
      loading.value = false;
    }
  }

  async function setAutostart(enabled: boolean) {
    settings.value = await settingsUpdate({ autostartEnabled: enabled });
  }

  return {
    settings,
    loading,
    error,
    load,
    setAutostart,
  };
});
