<script setup lang="ts">
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  BookOpenCheck,
  BookX,
  Filter,
  MoreHorizontal,
  RotateCcw,
  Search,
  SquarePen,
  Tags,
  Trash2,
  X,
} from "lucide-vue-next";
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";

import type { AppView } from "../../app/routes";
import { useAppQuitRequest } from "../../composables/useAppQuitRequest";
import { windowOpenQuickCapture } from "../../services/windowApi";
import { useEntriesStore } from "../../stores/entries";
import type { EntryStatus, EntryType } from "../../types/generated";
import EntryDetail from "../entry/EntryDetail.vue";
import type { EntryDetailToolbarState } from "../entry/entryDetailToolbar";
import EntryList from "../entry/EntryList.vue";
import SettingsView from "../settings/SettingsView.vue";
import IconButton from "../shared/IconButton.vue";
import SaveState from "../shared/SaveState.vue";
import SidebarNav from "./SidebarNav.vue";

const entries = useEntriesStore();
const detailRef = ref<InstanceType<typeof EntryDetail> | null>(null);
const tagPanelOpen = ref(false);
const filterPanelOpen = ref(false);
const detailMenuOpen = ref(false);
const filterMenuRef = ref<HTMLElement | null>(null);
const detailMenuRef = ref<HTMLElement | null>(null);
const filterButtonRef = ref<HTMLButtonElement | null>(null);
const detailMenuButtonRef = ref<HTMLButtonElement | null>(null);
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

function onGlobalKeydown(event: KeyboardEvent) {
  if (event.key === "Escape" && detailMenuOpen.value) {
    event.preventDefault();
    detailMenuOpen.value = false;
    void nextTick(() => detailMenuButtonRef.value?.focus());
    return;
  }
  if (event.key === "Escape" && filterPanelOpen.value) {
    event.preventDefault();
    filterPanelOpen.value = false;
    void nextTick(() => filterButtonRef.value?.focus());
    return;
  }
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
    void requestMoveSelectedToTrash();
  }
}

function onDocumentPointerDown(event: globalThis.PointerEvent) {
  const target = event.target as globalThis.Node;
  if (filterPanelOpen.value && !filterMenuRef.value?.contains(target)) {
    filterPanelOpen.value = false;
  }
  if (detailMenuOpen.value && !detailMenuRef.value?.contains(target)) {
    detailMenuOpen.value = false;
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
const searchInputRef = ref<HTMLInputElement | null>(null);
const searchQuery = ref(entries.filters.query);
const typeSelectRef = ref<HTMLSelectElement | null>(null);
const statusSelectRef = ref<HTMLSelectElement | null>(null);
const compactTypeSelectRef = ref<HTMLSelectElement | null>(null);
const compactStatusSelectRef = ref<HTMLSelectElement | null>(null);

watch(
  () => entries.filters.query,
  (value) => {
    if (searchQuery.value !== value) {
      searchQuery.value = value;
    }
  },
);

useAppQuitRequest(flushDetail);

onMounted(async () => {
  window.addEventListener("keydown", onGlobalKeydown);
  document.addEventListener("pointerdown", onDocumentPointerDown);
  if (isTauri()) {
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
  }
});

onUnmounted(() => {
  disposed = true;
  unlistenEntriesChanged?.();
  window.removeEventListener("keydown", onGlobalKeydown);
  document.removeEventListener("pointerdown", onDocumentPointerDown);
});

function focusSearch() {
  searchInputRef.value?.focus();
}

async function flushDetail() {
  return (await detailRef.value?.flushPendingSave()) ?? true;
}

async function requestMoveSelectedToTrash() {
  if (!(await flushDetail())) return;
  detailRef.value?.requestMoveToTrash();
}

async function selectEntry(id: string) {
  if (await flushDetail()) await entries.select(id);
}

async function openRelatedEntry(id: string) {
  if (await flushDetail()) await entries.openEntry(id);
}

async function changeView(view: AppView) {
  if (await flushDetail()) {
    tagPanelOpen.value = false;
    filterPanelOpen.value = false;
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
      <header
        data-testid="topbar-layout"
        class="sticky top-0 z-20 grid h-[52px] min-w-0 shrink-0 grid-cols-[minmax(0,1fr)_minmax(120px,min(480px,calc(100%_-_960px)))_minmax(0,1fr)] items-center gap-2 border-b border-border bg-bg-elevated px-3 md:px-4"
      >
        <div
          data-testid="topbar-start"
          class="flex min-w-0 items-center justify-start"
        >
          <h1 class="hidden text-heading text-text-primary xl:block">
            {{ currentViewLabel }}
          </h1>
        </div>

        <div class="relative min-w-0 text-text-secondary">
            <div class="relative w-full text-text-secondary">
            <Search
              :size="16"
              class="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2"
              aria-hidden="true"
            />
            <input
              id="global-search"
              ref="searchInputRef"
              :value="searchQuery"
              type="search"
              aria-label="搜索"
              placeholder="搜索标题与内容"
              class="input-base h-[36px] min-w-[120px] pl-9 pr-3 text-ui"
              @input="setQuery(($event.target as HTMLInputElement).value)"
            />
          </div>
        </div>

        <div
          data-testid="topbar-end"
          class="flex min-w-0 items-center justify-end gap-1"
        >
          <button
          v-if="entries.view === 'tags'"
          type="button"
          class="btn-secondary min-w-0 px-2.5 text-ui"
          :aria-expanded="tagPanelOpen"
          aria-controls="tag-panel"
          @click="tagPanelOpen = !tagPanelOpen"
        >
          <Tags :size="15" aria-hidden="true" />
          <span class="hidden xl:inline">{{
            entries.filters.tag ? `#${entries.filters.tag}` : "选择标签"
          }}</span>
        </button>

        <select
          ref="typeSelectRef"
          class="select-base hidden w-[112px] text-ui xl:block"
          aria-label="类型筛选"
          :value="entries.filters.entryType"
          @change="setTypeFilter(typeSelectRef!.value as EntryType | '')"
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
          ref="statusSelectRef"
          class="select-base hidden w-[112px] text-ui xl:block"
          aria-label="状态筛选"
          :value="entries.filters.status"
          @change="setStatusFilter(statusSelectRef!.value as EntryStatus | '')"
        >
          <option
            v-for="option in statusOptions"
            :key="option.value || 'all'"
            :value="option.value"
          >
            {{ option.label }}
          </option>
        </select>

        <div ref="filterMenuRef" class="relative xl:hidden">
          <button
            ref="filterButtonRef"
            type="button"
            class="btn-secondary min-w-0 px-2.5 text-ui"
            :aria-expanded="filterPanelOpen"
            aria-controls="entry-filter-popover"
            @click="filterPanelOpen = !filterPanelOpen"
          >
            <Filter :size="15" aria-hidden="true" />
            <span class="hidden min-[1100px]:inline">筛选</span>
          </button>
          <Transition name="filter-panel">
            <div
              v-if="filterPanelOpen"
              id="entry-filter-popover"
              class="elevation-2 absolute right-0 top-[42px] z-50 grid w-[220px] gap-3 rounded-lg bg-bg-elevated p-3"
              aria-label="条目筛选"
            >
              <label class="grid gap-1 text-micro text-text-tertiary">
                类型
                <select
                  ref="compactTypeSelectRef"
                  class="select-base w-full text-ui"
                  aria-label="类型筛选"
                  :value="entries.filters.entryType"
                  @change="setTypeFilter(compactTypeSelectRef!.value as EntryType | '')"
                >
                  <option
                    v-for="option in typeOptions"
                    :key="option.value || 'all'"
                    :value="option.value"
                  >
                    {{ option.label }}
                  </option>
                </select>
              </label>
              <label class="grid gap-1 text-micro text-text-tertiary">
                状态
                <select
                  ref="compactStatusSelectRef"
                  class="select-base w-full text-ui"
                  aria-label="状态筛选"
                  :value="entries.filters.status"
                  @change="setStatusFilter(compactStatusSelectRef!.value as EntryStatus | '')"
                >
                  <option
                    v-for="option in statusOptions"
                    :key="option.value || 'all'"
                    :value="option.value"
                  >
                    {{ option.label }}
                  </option>
                </select>
              </label>
            </div>
          </Transition>
        </div>

        <div
          v-if="showDetailActions"
          data-testid="detail-actions"
          class="flex min-w-0 shrink-0 items-center gap-1"
        >
          <SaveState
            data-testid="save-state"
            :state="toolbarState.saveState"
            :error="toolbarState.saveError"
            @retry="detailRef?.retrySave()"
          />
          <button
            v-if="toolbarState.showPromote"
            type="button"
            class="btn-primary hidden min-w-0 px-2.5 text-ui min-[1100px]:inline-flex"
            aria-label="沉淀为知识"
            :disabled="!toolbarState.canPromote"
            title="沉淀为知识"
            @click="detailRef?.promoteToKnowledge()"
          >
            <BookOpenCheck :size="15" aria-hidden="true" />
            <span class="hidden min-[1440px]:inline">沉淀为知识</span>
          </button>
          <button
            v-if="toolbarState.canDemote"
            type="button"
            class="btn-secondary hidden min-w-0 px-2.5 text-ui min-[1100px]:inline-flex"
            aria-label="移出知识库"
            title="移出知识库"
            @click="detailRef?.demoteFromKnowledge()"
          >
            <BookX :size="15" aria-hidden="true" />
            <span class="hidden min-[1440px]:inline">移出知识库</span>
          </button>
          <IconButton
            v-if="toolbarState.deleted"
            class="hidden min-[1100px]:inline-flex"
            label="恢复"
            :icon="RotateCcw"
            @click="detailRef?.restore()"
          />
          <IconButton
            v-if="toolbarState.deleted"
            class="hidden min-[1100px]:inline-flex"
            label="永久删除"
            :icon="Trash2"
            danger
            @click="detailRef?.requestDeleteForever()"
          />
          <IconButton
            v-else
            class="hidden min-[1100px]:inline-flex"
            label="移到回收站"
            :icon="Trash2"
            danger
            @click="requestMoveSelectedToTrash"
          />

          <div ref="detailMenuRef" class="relative min-[1100px]:hidden">
            <button
              ref="detailMenuButtonRef"
              type="button"
              class="btn-icon"
              aria-label="更多详情操作"
              :aria-expanded="detailMenuOpen"
              aria-controls="detail-actions-menu"
              @click="detailMenuOpen = !detailMenuOpen"
            >
              <MoreHorizontal :size="17" aria-hidden="true" />
            </button>
            <Transition name="filter-panel">
              <div
                v-if="detailMenuOpen"
                id="detail-actions-menu"
                class="elevation-2 absolute right-0 top-[40px] z-50 grid w-[180px] gap-1 rounded-lg bg-bg-elevated p-2"
                aria-label="详情操作"
              >
                <button
                  v-if="toolbarState.showPromote"
                  type="button"
                  class="btn-secondary w-full justify-start"
                  aria-label="沉淀为知识"
                  :disabled="!toolbarState.canPromote"
                  @click="detailMenuOpen = false; detailRef?.promoteToKnowledge()"
                >
                  <BookOpenCheck :size="15" aria-hidden="true" />
                  沉淀为知识
                </button>
                <button
                  v-if="toolbarState.canDemote"
                  type="button"
                  class="btn-secondary w-full justify-start"
                  aria-label="移出知识库"
                  @click="detailMenuOpen = false; detailRef?.demoteFromKnowledge()"
                >
                  <BookX :size="15" aria-hidden="true" />
                  移出知识库
                </button>
                <button
                  v-if="toolbarState.deleted"
                  type="button"
                  class="btn-secondary w-full justify-start"
                  aria-label="恢复"
                  @click="detailMenuOpen = false; detailRef?.restore()"
                >
                  <RotateCcw :size="15" aria-hidden="true" />
                  恢复
                </button>
                <button
                  v-if="toolbarState.deleted"
                  type="button"
                  class="btn-secondary w-full justify-start text-danger"
                  aria-label="永久删除"
                  @click="detailMenuOpen = false; detailRef?.requestDeleteForever()"
                >
                  <Trash2 :size="15" aria-hidden="true" />
                  永久删除
                </button>
                <button
                  v-else
                  type="button"
                  class="btn-secondary w-full justify-start text-danger"
                  aria-label="移到回收站"
                  @click="detailMenuOpen = false; void requestMoveSelectedToTrash()"
                >
                  <Trash2 :size="15" aria-hidden="true" />
                  移到回收站
                </button>
              </div>
            </Transition>
          </div>
        </div>

        <button
          type="button"
          class="btn-primary min-w-0 px-3 text-ui"
          @click="windowOpenQuickCapture"
        >
          <SquarePen :size="15" aria-hidden="true" />
          <span class="hidden min-[1100px]:inline">记录</span>
        </button>
        </div>
      </header>

      <p
        v-if="entries.error && entries.view !== 'settings'"
        class="elevation-1 z-10 m-0 shrink-0 border-x-0 border-t-0 bg-bg-elevated px-4 py-2 text-ui text-danger"
        role="alert"
      >
        {{ entries.error }}
      </p>

      <section class="relative min-h-0 flex-1 overflow-hidden">
        <section v-if="entries.view === 'settings'" class="h-full overflow-auto">
          <SettingsView />
        </section>
        <section
          v-else
          class="grid h-full min-h-0 grid-cols-[minmax(300px,34%)_minmax(0,1fr)]"
        >
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
            :selection-generation="entries.selectionGeneration"
            @toolbar-change="toolbarState = $event"
            @saved="entries.applySavedEntry"
            @entry-updated="entries.applyEntryListUpdate"
            @trash="entries.moveSelectedToTrash"
            @restore="entries.restoreSelected"
            @delete-forever="entries.deleteSelectedForever"
            @open-related="openRelatedEntry"
          />
        </section>

        <Transition name="tag-panel">
          <aside
            v-if="entries.view === 'tags' && tagPanelOpen"
            id="tag-panel"
            class="elevation-2 absolute inset-y-0 left-0 z-40 flex w-[236px] flex-col bg-bg-elevated"
            aria-label="标签筛选"
          >
            <header
              class="flex h-[48px] items-center justify-between border-b border-border px-3"
            >
              <strong class="text-ui font-semibold">全部标签</strong>
              <button
                type="button"
                class="btn-icon"
                aria-label="关闭标签面板"
                @click="tagPanelOpen = false"
              >
                <X :size="16" aria-hidden="true" />
              </button>
            </header>
            <div class="min-h-0 flex-1 overflow-auto p-2">
              <button
                type="button"
                class="mb-1 flex h-[36px] w-full items-center justify-between rounded-md border-none bg-transparent px-3 text-ui text-text-secondary transition-colors duration-fast ease-token ring-focus hover:bg-bg-hover hover:text-text-primary"
                :class="{
                  'bg-selected font-medium text-text-primary hover:bg-selected':
                    !entries.filters.tag,
                }"
                @click="setTagFilter('')"
              >
                <span>全部标签</span>
                <span class="text-micro text-text-tertiary">{{
                  entries.tags.length
                }}</span>
              </button>
              <button
                v-for="tag in entries.tags"
                :key="tag.id"
                type="button"
                class="flex h-[36px] w-full items-center justify-between rounded-md border-none bg-transparent px-3 text-ui text-text-secondary transition-colors duration-fast ease-token ring-focus hover:bg-bg-hover hover:text-text-primary"
                :class="{
                  'bg-selected font-medium text-text-primary hover:bg-selected':
                    entries.filters.tag === tag.name,
                }"
                @click="setTagFilter(tag.name)"
              >
                <span class="overflow-hidden text-ellipsis whitespace-nowrap"
                  >#{{ tag.name }}</span
                >
                <span class="text-micro text-text-tertiary">{{
                  tag.entryCount
                }}</span>
              </button>
            </div>
          </aside>
        </Transition>
      </section>
    </section>
  </main>
</template>

<style scoped>
.tag-panel-enter-active,
.tag-panel-leave-active,
.filter-panel-enter-active,
.filter-panel-leave-active {
  transition:
    opacity var(--duration-base) var(--ease-out),
    transform var(--duration-base) var(--ease-out);
}

.tag-panel-enter-from,
.tag-panel-leave-to {
  opacity: 0;
  transform: translateX(-10px);
}

.filter-panel-enter-from,
.filter-panel-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
