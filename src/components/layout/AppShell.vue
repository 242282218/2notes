<script setup lang="ts">
import { Search, SquarePen } from "lucide-vue-next";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { onMounted, onUnmounted, ref } from "vue";

import type { AppView } from "../../app/routes";
import { useAppQuitRequest } from "../../composables/useAppQuitRequest";
import { windowOpenQuickCapture } from "../../services/windowApi";
import { useEntriesStore } from "../../stores/entries";
import type { EntryStatus, EntryType } from "../../types/generated";
import EntryDetail from "../entry/EntryDetail.vue";
import EntryList from "../entry/EntryList.vue";
import SettingsView from "../settings/SettingsView.vue";
import SidebarNav from "./SidebarNav.vue";

const entries = useEntriesStore();
const detailRef = ref<InstanceType<typeof EntryDetail> | null>(null);
let unlistenEntriesChanged: (() => void) | null = null;

function onGlobalKeydown(event: KeyboardEvent) {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f") {
    event.preventDefault();
    focusSearch();
  }
}

const typeOptions: Array<{ value: EntryType | ""; label: string }> = [
  { value: "", label: "全部类型" },
  { value: "unclear", label: "未澄清" },
  { value: "idea", label: "想法" },
  { value: "task", label: "任务" },
  { value: "material", label: "素材" },
  { value: "question", label: "问题" },
];

const statusOptions: Array<{ value: EntryStatus | ""; label: string }> = [
  { value: "", label: "全部状态" },
  { value: "pending", label: "待处理" },
  { value: "done", label: "已完成" },
  { value: "archived", label: "已归档" },
];

useAppQuitRequest(flushDetail);

onMounted(async () => {
  window.addEventListener("keydown", onGlobalKeydown);
  if (isTauri()) {
    unlistenEntriesChanged = await listen("entries-changed", async () => {
      await entries.load();
      await entries.refreshTags();
    });
    await entries.load();
    await entries.refreshTags();
  }
});

onUnmounted(() => {
  unlistenEntriesChanged?.();
  window.removeEventListener("keydown", onGlobalKeydown);
});

function focusSearch() {
  const input = document.querySelector<HTMLInputElement>("#global-search");
  input?.focus();
}

async function flushDetail() {
  return (await detailRef.value?.flushPendingSave()) ?? true;
}

async function selectEntry(id: string) {
  if (await flushDetail()) {
    await entries.select(id);
  }
}

async function changeView(view: AppView) {
  if (await flushDetail()) {
    await entries.setView(view);
  }
}

async function setTagFilter(tag: string) {
  if (await flushDetail()) {
    await entries.setTagFilter(tag);
  }
}

async function setTypeFilter(value: EntryType | "") {
  if (await flushDetail()) {
    await entries.setTypeFilter(value);
  }
}

async function setStatusFilter(value: EntryStatus | "") {
  if (await flushDetail()) {
    await entries.setStatusFilter(value);
  }
}

async function setQuery(value: string) {
  if (await flushDetail()) {
    await entries.setQuery(value);
  }
}

</script>

<template>
  <main class="app-shell">
    <SidebarNav
      :view="entries.view"
      @change="changeView"
    />

    <section class="workspace">
      <header class="topbar">
        <div class="search-box">
          <Search :size="18" />
          <input
            id="global-search"
            :value="entries.filters.query"
            type="search"
            placeholder="搜索标题、正文、原文或标签"
            @input="setQuery(($event.target as HTMLInputElement).value)"
          >
        </div>
        <select
          class="filter-select"
          :value="entries.filters.entryType"
          @change="
            setTypeFilter(
              ($event.target as HTMLSelectElement).value as EntryType | '',
            )
          "
        >
          <option
            v-for="option in typeOptions"
            :key="option.value || 'all'"
            :value="option.value"
          >
            {{ option.label }}
          </option>
        </select>
        <select
          class="filter-select"
          :value="entries.filters.status"
          @change="
            setStatusFilter(
              ($event.target as HTMLSelectElement).value as EntryStatus | '',
            )
          "
        >
          <option
            v-for="option in statusOptions"
            :key="option.value || 'all'"
            :value="option.value"
          >
            {{ option.label }}
          </option>
        </select>
        <button
          type="button"
          class="primary-button"
          @click="windowOpenQuickCapture"
        >
          <SquarePen :size="16" />
          记录
        </button>
      </header>

      <section
        v-if="entries.view === 'settings'"
        class="content-area settings-only"
      >
        <SettingsView />
      </section>
      <section
        v-else
        class="content-area"
      >
        <aside
          v-if="entries.view === 'tags'"
          class="tags-panel"
        >
          <button
            v-for="tag in entries.tags"
            :key="tag.id"
            type="button"
            :class="{ active: entries.filters.tag === tag.name }"
            @click="setTagFilter(tag.name)"
          >
            <span>#{{ tag.name }}</span>
            <strong>{{ tag.entryCount }}</strong>
          </button>
        </aside>

        <EntryList
          :items="entries.items"
          :selected-id="entries.selectedId"
          :loading="entries.loading"
          :has-more="entries.hasMore"
          @select="selectEntry"
          @more="entries.loadMore"
        />

        <EntryDetail
          ref="detailRef"
          :detail="entries.detail"
          :loading="entries.detailLoading"
          @saved="entries.applySavedEntry"
          @trash="entries.moveSelectedToTrash"
          @restore="entries.restoreSelected"
          @delete-forever="entries.deleteSelectedForever"
        />
      </section>
    </section>
  </main>
</template>
