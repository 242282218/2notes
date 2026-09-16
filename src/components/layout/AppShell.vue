<script setup lang="ts">
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";

import type { AppView } from "../../app/routes";
import { VIEW_LABELS } from "../../constants/labels";
import { useAppQuitRequest } from "../../composables/useAppQuitRequest";
import { useAppShellShortcuts } from "../../composables/useAppShellShortcuts";
import { waitForPendingOperations } from "../../composables/usePendingOperations";
import { windowOpenQuickCapture } from "../../services/windowApi";
import { toErrorMessage, isKnownNonRecoverable } from "../../utils/errors";
import { useEntriesStore } from "../../stores/entries";
import type {
  EntryDetail as EntryDetailType,
  EntryStatus,
  EntryType,
} from "../../types/generated";
import EntryDetail from "../entry/EntryDetail.vue";
import type { EntryDetailToolbarState } from "../entry/entryDetailToolbar";
import EntryList from "../entry/EntryList.vue";
import EntryTree from "../entry/EntryTree.vue";
import KnowledgeHealthView from "../health/KnowledgeHealthView.vue";
import SettingsView from "../settings/SettingsView.vue";
import AppTopbar from "./AppTopbar.vue";
import SidebarNav from "./SidebarNav.vue";
import TagFilterPanel from "./TagFilterPanel.vue";

const entries = useEntriesStore();
const detailRef = ref<InstanceType<typeof EntryDetail> | null>(null);
const entryListRef = ref<InstanceType<typeof EntryList> | null>(null);
const topbarRef = ref<InstanceType<typeof AppTopbar> | null>(null);
const tagPanelOpen = ref(false);
const creatingEntry = ref(false);
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

const viewLabels = VIEW_LABELS;
const currentViewLabel = computed(() => viewLabels[entries.view]);
const showDetailActions = computed(
  () =>
    entries.view !== "settings" &&
    entries.view !== "health" &&
    Boolean(entries.detail),
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

function clarify(key: string): boolean {
  if (!entries.selectedId || entries.view === "trash") return false;
  if (key === "j" || key === "k") {
    entryListRef.value?.focusRelative(key === "j" ? 1 : -1);
    return true;
  }
  if (key === "e") {
    detailRef.value?.focusTitle();
    return true;
  }
  if (key === "t") {
    (
      document.querySelector(".entry-type-select") as HTMLSelectElement | null
    )?.focus();
    return true;
  }
  if (key === "l") {
    (document.querySelector("[data-tag-input]") as HTMLElement | null)?.focus();
    return true;
  }
  if (key === "d") {
    void requestMoveSelectedToTrash();
    return true;
  }
  if (key === "a") {
    // Through the detail form, not the store: the store write would bypass the
    // detail's local state and the next autosave would restore the old status.
    return detailRef.value?.archive() ?? false;
  }
  if (key === "m") {
    if (entries.view === "knowledge") {
      (
        document.querySelector("[data-entry-tree]") as HTMLElement | null
      )?.focus();
      return true;
    }
  }
  return false;
}

useAppQuitRequest(async () => {
  if (!(await flushDetail())) return false;
  return waitForPendingOperations();
});
useAppShellShortcuts({
  focusSearch: () => topbarRef.value?.focusSearch(),
  closeTopmostOverlay,
  canDelete: canDeleteSelected,
  requestDelete: () => {
    void requestMoveSelectedToTrash();
  },
  clarify,
});

onMounted(async () => {
  document.addEventListener("pointerdown", onDocumentPointerDown);
  // Tauri event wiring only; browser dev (pnpm dev without Tauri) falls through
  // to the store load below, which surfaces a load-error banner via the services
  // instead of leaving a silent blank workspace.
  if (isTauri()) {
    const unlisten = await listen("entries-changed", async () => {
      if (disposed) return;
      if (await flushDetail()) {
        if (entries.view !== "health") {
          // Preserve the current selection: a quick-capture submit must not
          // kick the user off an entry that fell out of the first page.
          await entries.reconcileCurrentList();
          await entries.refreshTags();
        }
        entries.noteExternalChange();
      }
    });
    if (disposed) {
      unlisten();
      return;
    }
    unlistenEntriesChanged = unlisten;
  }
  await entries.load();
  await entries.refreshTags();
  if (disposed) return;
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
    const updated = await entries.moveKnowledge(id, parentId, expectedRevision);
    entries.applySavedEntry(updated, requestGeneration);
  } catch (cause) {
    if (
      entries.selectionGeneration === requestGeneration &&
      entries.selectedId === id
    ) {
      entries.error = toErrorMessage(cause, "移动知识条目失败");
      entries.errorRecoverable = !isKnownNonRecoverable(cause);
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

function onSaved(entry: EntryDetailType, generation: number) {
  entries.applySavedEntry(entry, generation);
}

function onEntryUpdated(entry: EntryDetailType) {
  entries.applyEntryListUpdate(entry);
}

async function onMoveToTrash() {
  await entries.moveSelectedToTrash();
}

async function onRestoreSelected() {
  await entries.restoreSelected();
}

async function onDeleteForever() {
  await entries.deleteSelectedForever();
}

function openQuickCapture() {
  void windowOpenQuickCapture().catch((error) => {
    entries.error = toErrorMessage(error, "打开快速记录失败");
    entries.errorRecoverable = !isKnownNonRecoverable(error);
  });
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
        @quick-capture="openQuickCapture"
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
        {{ entries.error
        }}<template v-if="!entries.errorRecoverable">（无法重试）</template>
      </p>
      <section class="relative min-h-0 flex-1 overflow-hidden">
        <section
          v-if="entries.view === 'settings'"
          class="h-full overflow-auto"
        >
          <SettingsView />
        </section>
        <section
          v-else-if="entries.view === 'health'"
          class="h-full overflow-auto"
        >
          <KnowledgeHealthView
            :invalidated-token="
              entries.healthToken + entries.externalChangeToken
            "
            @open-entry="openRelatedEntry"
          />
        </section>
        <section
          v-else
          class="grid h-full min-h-0 grid-cols-[minmax(300px,34%)_minmax(0,1fr)]"
        >
          <EntryTree
            v-if="entries.view === 'knowledge'"
            :selected-id="entries.selectedId"
            :refresh-token="
              entries.knowledgeTreeToken + entries.externalChangeToken
            "
            @select="selectEntry"
            @move="moveKnowledgeEntry"
          />
          <EntryList
            v-else
            ref="entryListRef"
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
            @saved="onSaved"
            @entry-updated="onEntryUpdated"
            @trash="onMoveToTrash"
            @restore="onRestoreSelected"
            @delete-forever="onDeleteForever"
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
