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
    return;
  }
  if (
    event.key === "Delete" &&
    entries.view !== "trash" &&
    entries.selectedId &&
    document.activeElement?.closest(".entry-list")
  ) {
    event.preventDefault();
    void entries.moveSelectedToTrash();
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

const searchInputRef = ref<HTMLInputElement | null>(null);
const typeSelectRef = ref<HTMLSelectElement | null>(null);
const statusSelectRef = ref<HTMLSelectElement | null>(null);

useAppQuitRequest(flushDetail);

onMounted(async () => {
  window.addEventListener("keydown", onGlobalKeydown);
  if (isTauri()) {
    unlistenEntriesChanged = await listen("entries-changed", async () => {
      if (await flushDetail()) {
        await entries.load();
        await entries.refreshTags();
        entries.noteExternalChange();
      }
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

async function openRelatedEntry(id: string) {
  if (await flushDetail()) {
    await entries.openEntry(id);
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
  <a
    href="#workspace"
    class="absolute -top-10 left-4 z-[100] inline-flex h-[34px] items-center rounded-md bg-brand px-3 font-medium text-white no-underline transition-[top] duration-150 focus:top-3 ring-focus"
  >
    跳转到主内容
  </a>
  <main class="grid min-h-screen grid-cols-[64px_1fr] bg-bg-base md:grid-cols-[200px_1fr]">
    <SidebarNav :view="entries.view" @change="changeView" />

    <section id="workspace" class="grid min-w-0 grid-rows-[58px_1fr]" tabindex="-1" aria-label="工作区">
      <header
        class="sticky top-0 z-20 grid grid-cols-[1fr_auto] items-center gap-3 border-b border-border-subtle p-3 px-4 glass-panel md:grid-cols-[minmax(220px,1fr)_150px_150px_auto]"
      >
        <div
          class="group flex h-[38px] min-w-0 items-center gap-2 rounded-md border border-border-subtle bg-bg-inset px-3 text-text-secondary transition-all duration-150 focus-within:border-brand focus-within:bg-bg-elevated focus-within:text-text-primary focus-within:shadow-[0_0_0_3px_var(--color-focus-ring-bg),var(--color-brand-glow)]"
        >
          <Search :size="18" />
          <input
            id="global-search"
            ref="searchInputRef"
            :value="entries.filters.query"
            type="search"
            aria-label="搜索"
            placeholder="搜索..."
            class="min-w-0 flex-1 bg-transparent outline-none border-none"
            @input="setQuery(($event.target as HTMLInputElement).value)"
          />
          <kbd
            class="hidden md:inline-flex items-center gap-0.5 rounded-sm border border-border-strong bg-bg-elevated px-1.5 py-0.5 text-[11px] font-medium leading-none text-text-tertiary shadow-[0_1px_2px_rgba(0,0,0,0.06)] font-mono"
          >
            <abbr title="Command" class="no-underline">⌘</abbr>F
          </kbd>
        </div>
        <select
          ref="typeSelectRef"
          class="select-base hidden md:block"
          aria-label="类型筛选"
          :value="entries.filters.entryType"
          @change="setTypeFilter((typeSelectRef!.value as EntryType | '') ?? '')"
        >
          <option v-for="option in typeOptions" :key="option.value || 'all'" :value="option.value">
            {{ option.label }}
          </option>
        </select>
        <select
          ref="statusSelectRef"
          class="select-base hidden md:block"
          aria-label="状态筛选"
          :value="entries.filters.status"
          @change="setStatusFilter((statusSelectRef!.value as EntryStatus | '') ?? '')"
        >
          <option v-for="option in statusOptions" :key="option.value || 'all'" :value="option.value">
            {{ option.label }}
          </option>
        </select>
        <button type="button" class="btn-primary" @click="windowOpenQuickCapture">
          <SquarePen :size="16" />
          记录
        </button>
      </header>

      <p v-if="entries.error && entries.view !== 'settings'" class="text-danger p-4" role="alert">
        {{ entries.error }}
      </p>

      <section
        v-if="entries.view === 'settings'"
        class="block overflow-auto"
      >
        <SettingsView />
      </section>
      <section
        v-else
        class="grid min-h-0 grid-cols-[minmax(220px,42%)_minmax(320px,1fr)] md:grid-cols-[minmax(280px,32%)_minmax(440px,1fr)]"
        :class="{ 'md:grid-cols-[200px_minmax(260px,28%)_minmax(440px,1fr)]': entries.view === 'tags' }"
      >
        <aside
          v-if="entries.view === 'tags'"
          class="hidden overflow-auto border-r border-border bg-bg-secondary p-3 md:block"
        >
          <button
            v-for="tag in entries.tags"
            :key="tag.id"
            type="button"
            class="flex w-full items-center justify-between h-[36px] rounded-md px-3 text-text-secondary transition-colors duration-150 hover:bg-bg-hover hover:text-text-primary ring-focus border-none bg-transparent"
            :class="{ 'font-semibold text-brand bg-brand-subtle hover:bg-brand-subtle hover:text-brand': entries.filters.tag === tag.name }"
            @click="setTagFilter(tag.name)"
          >
            <span class="overflow-hidden text-ellipsis whitespace-nowrap">#{{ tag.name }}</span>
            <strong class="text-xs text-text-tertiary font-normal">{{ tag.entryCount }}</strong>
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
          :refresh-token="entries.externalChangeToken"
          @saved="entries.applySavedEntry"
          @trash="entries.moveSelectedToTrash"
          @restore="entries.restoreSelected"
          @delete-forever="entries.deleteSelectedForever"
          @open-related="openRelatedEntry"
        />
      </section>
    </section>
  </main>
</template>
