<script setup lang="ts">
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";

import type { AppView } from "../../app/routes";
import { useAppQuitRequest } from "../../composables/useAppQuitRequest";
import { useAppShellShortcuts } from "../../composables/useAppShellShortcuts";
import { knowledgeMove } from "../../services/knowledgeApi";
import { windowOpenQuickCapture } from "../../services/windowApi";
import { useEntriesStore } from "../../stores/entries";
import type { EntryStatus, EntryType } from "../../types/generated";
import EntryDetail from "../entry/EntryDetail.vue";
import type { EntryDetailToolbarState } from "../entry/entryDetailToolbar";
import EntryList from "../entry/EntryList.vue";
import EntryTree from "../entry/EntryTree.vue";
import SettingsView from "../settings/SettingsView.vue";
import AppTopbar from "./AppTopbar.vue";
import SidebarNav from "./SidebarNav.vue";
import TagFilterPanel from "./TagFilterPanel.vue";

const entries = useEntriesStore();
const detailRef = ref<InstanceType<typeof EntryDetail> | null>(null);
const topbarRef = ref<InstanceType<typeof AppTopbar> | null>(null);
const tagPanelOpen = ref(false);
const creatingEntry = ref(false);
const knowledgeTreeToken = ref(0);
const tagPanelRef = ref<InstanceType<typeof TagFilterPanel> | null>(null);
const toolbarState = ref<EntryDetailToolbarState>({
  saveState: "idle",
  saveError: null,
  showPromote: false,
  canPromote: false,
  canDemote: false,
  deleted: false,
});
let unlistenEntriesChanged: (() => void) | null = null;
let disposed = false;

const viewLabels: Record<AppView, string> = {
  inbox: "收集箱",
  knowledge: "知识库",
  search: "搜索",
  tags: "标签",
  trash: "回收站",
  settings: "设置",
};
const currentViewLabel = computed(() => viewLabels[entries.view]);
const showDetailActions = computed(
  () => entries.view !== "settings" && Boolean(entries.detail),
);
const searchQuery = ref(entries.filters.query);
watch(
  () => entries.filters.query,
  (value) => {
    if (searchQuery.value !== value) searchQuery.value = value;
  },
);

function closeTagPanel(returnFocus = false) {
  if (!tagPanelOpen.value) return;
  tagPanelOpen.value = false;
  if (returnFocus) void nextTick(() => topbarRef.value?.tagButtonRef?.focus());
}

/** Detail menu / filter own Escape; AppShell only closes the tag panel. */
function closeTopmostOverlay() {
  if (tagPanelOpen.value) closeTagPanel(true);
}

function canDeleteSelected() {
  return (
    entries.view !== "trash" &&
    Boolean(entries.selectedId) &&
    Boolean(document.activeElement?.closest(".entry-list"))
  );
}

function onDocumentPointerDown(event: globalThis.PointerEvent) {
  const target = event.target as globalThis.Node;
  const tagButton = topbarRef.value?.tagButtonRef ?? null;
  const tagPanelEl = (tagPanelRef.value as { $el?: HTMLElement } | null)?.$el;
  if (
    tagPanelOpen.value &&
    !tagPanelEl?.contains(target) &&
    !tagButton?.contains(target)
  ) {
    closeTagPanel();
  }
}

async function flushDetail() {
  return (await detailRef.value?.flushPendingSave()) ?? true;
}

async function requestMoveSelectedToTrash() {
  if (!(await flushDetail())) return;
  detailRef.value?.requestMoveToTrash();
}

useAppQuitRequest(flushDetail);
useAppShellShortcuts({
  focusSearch: () => topbarRef.value?.focusSearch(),
  closeTopmostOverlay,
  canDelete: canDeleteSelected,
  requestDelete: () => {
    void requestMoveSelectedToTrash();
  },
});

onMounted(async () => {
  document.addEventListener("pointerdown", onDocumentPointerDown);
  if (!isTauri()) return;
  const unlisten = await listen("entries-changed", async () => {
    if (await flushDetail()) {
      await entries.load();
      await entries.refreshTags();
      entries.noteExternalChange();
    }
  });
  if (disposed) {
    unlisten();
    return;
  }
  unlistenEntriesChanged = unlisten;
  await entries.load();
  await entries.refreshTags();
});

onUnmounted(() => {
  disposed = true;
  unlistenEntriesChanged?.();
  document.removeEventListener("pointerdown", onDocumentPointerDown);
});

async function selectEntry(id: string) {
  if (await flushDetail()) await entries.select(id);
}
async function openRelatedEntry(id: string) {
  if (await flushDetail()) await entries.openEntry(id);
}
async function moveKnowledgeEntry(id: string, parentId: string | null) {
  const requestGeneration = entries.selectionGeneration;
  if (
    !entries.detail ||
    entries.detail.id !== id ||
    !(await flushDetail()) ||
    entries.selectionGeneration !== requestGeneration ||
    entries.detail?.id !== id
  ) {
    return;
  }
  const expectedRevision = entries.detail.revision;
  try {
    const updated = await knowledgeMove(id, parentId, 0, expectedRevision);
    if (
      entries.selectionGeneration === requestGeneration &&
      entries.selectedId === id
    ) {
      entries.applySavedEntry(updated, requestGeneration);
    } else {
      entries.applyEntryListUpdate(updated);
    }
    knowledgeTreeToken.value += 1;
  } catch (cause) {
    if (
      entries.selectionGeneration === requestGeneration &&
      entries.selectedId === id
    ) {
      entries.error =
        cause instanceof Error ? cause.message : "移动知识条目失败";
    }
  }
}
async function changeView(view: AppView) {
  if (await flushDetail()) {
    tagPanelOpen.value = false;
    topbarRef.value?.closeFilterPanel();
    await entries.setView(view);
  }
}
async function setTagFilter(tag: string) {
  if (await flushDetail()) {
    tagPanelOpen.value = false;
    await entries.setTagFilter(tag);
  }
}
async function setTypeFilter(value: EntryType | "") {
  if (await flushDetail()) await entries.setTypeFilter(value);
}
async function setStatusFilter(value: EntryStatus | "") {
  if (await flushDetail()) await entries.setStatusFilter(value);
}
async function createEntry() {
  if (creatingEntry.value) return;
  creatingEntry.value = true;
  try {
    if (!(await flushDetail())) return;
    const entry = await entries.createAndSelect();
    if (!entry) return;
    await nextTick();
    detailRef.value?.focusTitle();
  } catch {
    // The store retains the current selection and exposes the operation error.
  } finally {
    creatingEntry.value = false;
  }
}
function setQuery(value: string) {
  searchQuery.value = value;
  entries.setQuery(value);
}
</script>

<template>
  <a
    href="#workspace"
    class="absolute -top-10 left-4 z-[100] inline-flex h-[34px] items-center rounded-md bg-brand px-3 font-medium text-white no-underline transition-[top] duration-fast ease-token focus:top-3 ring-focus"
  >
    跳转到主内容
  </a>
  <main
    class="grid h-screen min-h-[480px] grid-cols-[64px_minmax(0,1fr)] overflow-hidden bg-bg-base md:grid-cols-[200px_minmax(0,1fr)]"
  >
    <SidebarNav :view="entries.view" @change="changeView" />
    <section
      id="workspace"
      class="flex min-w-0 flex-col overflow-hidden"
      tabindex="-1"
      aria-label="工作区"
    >
      <AppTopbar
        ref="topbarRef"
        :model-value="searchQuery"
        :tag-panel-open="tagPanelOpen"
        :view-label="currentViewLabel"
        :view="entries.view"
        :current-tag="entries.filters.tag"
        :entry-type="entries.filters.entryType"
        :status="entries.filters.status"
        :show-detail-actions="showDetailActions"
        :toolbar-state="toolbarState"
        :creating-entry="creatingEntry"
        @update:model-value="setQuery"
        @update:tag-panel-open="tagPanelOpen = $event"
        @type-change="setTypeFilter"
        @status-change="setStatusFilter"
        @create="createEntry"
        @quick-capture="windowOpenQuickCapture"
        @retry="detailRef?.retrySave()"
        @promote="detailRef?.promoteToKnowledge()"
        @demote="detailRef?.demoteFromKnowledge()"
        @restore="detailRef?.restore()"
        @delete-forever="detailRef?.requestDeleteForever()"
        @move-to-trash="requestMoveSelectedToTrash"
      />
      <p
        v-if="entries.error && entries.view !== 'settings'"
        class="elevation-1 z-10 m-0 shrink-0 border-x-0 border-t-0 bg-bg-elevated px-4 py-2 text-ui text-danger"
        role="alert"
      >
        {{ entries.error }}
      </p>
      <section class="relative min-h-0 flex-1 overflow-hidden">
        <section
          v-if="entries.view === 'settings'"
          class="h-full overflow-auto"
        >
          <SettingsView />
        </section>
        <section
          v-else
          class="grid h-full min-h-0 grid-cols-[minmax(300px,34%)_minmax(0,1fr)]"
        >
          <EntryTree
            v-if="entries.view === 'knowledge'"
            :selected-id="entries.selectedId"
            :refresh-token="knowledgeTreeToken + entries.externalChangeToken"
            @select="selectEntry"
            @move="moveKnowledgeEntry"
          />
          <EntryList
            v-else
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
            :selection-generation="entries.selectionGeneration"
            @toolbar-change="toolbarState = $event"
            @saved="
              (entry, generation) => {
                entries.applySavedEntry(entry, generation);
                knowledgeTreeToken += 1;
              }
            "
            @entry-updated="
              (entry) => {
                entries.applyEntryListUpdate(entry);
                knowledgeTreeToken += 1;
              }
            "
            @trash="
              async () => {
                await entries.moveSelectedToTrash();
                knowledgeTreeToken += 1;
              }
            "
            @restore="
              async () => {
                await entries.restoreSelected();
                knowledgeTreeToken += 1;
              }
            "
            @delete-forever="
              async () => {
                await entries.deleteSelectedForever();
                knowledgeTreeToken += 1;
              }
            "
            @open-related="openRelatedEntry"
          />
        </section>
        <Transition name="tag-panel">
          <TagFilterPanel
            v-if="entries.view === 'tags' && tagPanelOpen"
            ref="tagPanelRef"
            :tags="entries.tags"
            :current-tag="entries.filters.tag"
            :tags-error="entries.tagsError"
            @select="setTagFilter"
            @close="closeTagPanel"
            @retry="entries.refreshTags()"
          />
        </Transition>
      </section>
    </section>
  </main>
</template>

<style scoped>
.tag-panel-enter-active,
.tag-panel-leave-active {
  transition:
    opacity var(--duration-base) var(--ease-out),
    transform var(--duration-base) var(--ease-out);
}
.tag-panel-enter-from,
.tag-panel-leave-to {
  opacity: 0;
  transform: translateX(-10px);
}
</style>
