import { defineStore } from "pinia";
import { computed, reactive, ref } from "vue";

import type { AppView } from "../app/routes";
import {
  entriesDeleteForever,
  entriesGet,
  entriesList,
  entriesMoveToTrash,
  entriesRestoreFromTrash,
  entriesUpdate,
} from "../services/entryApi";
import { tagsList } from "../services/tagApi";
import type {
  EntryDetail,
  EntryListItem,
  EntryPatch,
  EntryStatus,
  EntryType,
  Tag,
} from "../types/generated";
import {
  buildEntryFilter,
  entryMatchesCurrentFilter,
  type UiFilters,
} from "../composables/useEntryFilters";

const SEARCH_DEBOUNCE_MS = 150;

export const useEntriesStore = defineStore("entries", () => {
  const view = ref<AppView>("inbox");
  const filters = reactive<UiFilters>({
    query: "",
    entryType: "",
    status: "",
    tag: "",
  });
  const items = ref<EntryListItem[]>([]);
  const tags = ref<Tag[]>([]);
  const selectedId = ref<string | null>(null);
  const selectionGeneration = ref(0);
  const detail = ref<EntryDetail | null>(null);

  function beginSelection() {
    selectionGeneration.value += 1;
    return selectionGeneration.value;
  }
  const loading = ref(false);
  const detailLoading = ref(false);
  const error = ref<string | null>(null);
  const tagsError = ref<string | null>(null);
  const externalChangeToken = ref(0);
  const hasMore = ref(false);
  const offset = ref(0);
  const limit = 50;
  let loadRequestId = 0;
  let selectRequestId = 0;
  let openRequestId = 0;
  let tagsRequestId = 0;
  let searchTimer: ReturnType<typeof setTimeout> | null = null;

  const selectedItem = computed(() =>
    items.value.find((item) => item.id === selectedId.value),
  );

  async function load(resetPage = true) {
    await loadEntries(resetPage);
  }

  async function loadEntries(
    resetPage = true,
    isCurrent: () => boolean = () => true,
    preserveSelection = false,
    clearExisting = false,
  ) {
    const requestId = ++loadRequestId;
    loading.value = true;
    error.value = null;
    if (resetPage) {
      offset.value = 0;
      if (clearExisting) {
        items.value = [];
      }
    }
    const requestOffset = offset.value;
    const filter = buildEntryFilter(view.value, filters);
    try {
      const page = await entriesList(filter, {
        limit,
        offset: requestOffset,
      });
      if (requestId !== loadRequestId || !isCurrent()) {
        return;
      }
      items.value = resetPage ? page.items : [...items.value, ...page.items];
      hasMore.value = page.hasMore;
      if (!preserveSelection) {
        if (!selectedId.value && page.items[0]) {
          await select(page.items[0].id);
        }
        if (
          selectedId.value &&
          !items.value.some((item) => item.id === selectedId.value)
        ) {
          beginSelection();
          detail.value = null;
          selectedId.value = null;
        }
      }
    } catch (loadError) {
      if (requestId === loadRequestId && isCurrent()) {
        error.value =
          loadError instanceof Error ? loadError.message : "加载失败";
      }
    } finally {
      if (requestId === loadRequestId) {
        loading.value = false;
      }
    }
  }

  async function reconcileCurrentList() {
    await loadEntries(true, () => true, true);
  }

  async function loadMore() {
    if (!hasMore.value || loading.value) {
      return;
    }
    const previousOffset = offset.value;
    offset.value = previousOffset + limit;
    await load(false);
    if (error.value) {
      offset.value = previousOffset;
    }
  }

  async function refreshTags() {
    const requestId = ++tagsRequestId;
    tagsError.value = null;
    try {
      const next = await tagsList();
      if (requestId === tagsRequestId) {
        tags.value = next;
        tagsError.value = null;
      }
    } catch (cause) {
      if (requestId === tagsRequestId) {
        tagsError.value = getErrorMessage(cause, "标签加载失败");
      }
    }
  }

  async function select(id: string) {
    beginSelection();
    openRequestId += 1;
    const requestId = ++selectRequestId;
    selectedId.value = id;
    detail.value = null;
    detailLoading.value = true;
    error.value = null;
    try {
      const entry = await entriesGet(id);
      if (requestId === selectRequestId && selectedId.value === id) {
        detail.value = entry;
      }
    } catch (selectError) {
      if (requestId === selectRequestId) {
        error.value =
          selectError instanceof Error ? selectError.message : "加载失败";
        selectedId.value = null;
        detail.value = null;
      }
    } finally {
      if (requestId === selectRequestId) {
        detailLoading.value = false;
      }
    }
  }

  function noteExternalChange() {
    externalChangeToken.value += 1;
  }

  async function openEntry(id: string) {
    beginSelection();
    const requestId = ++openRequestId;
    const selectionRequestId = ++selectRequestId;
    error.value = null;
    detailLoading.value = true;
    try {
      const entry = await entriesGet(id);
      if (requestId !== openRequestId) {
        return;
      }
      view.value = entry.deletedAt
        ? "trash"
        : entry.knowledgeState === "knowledge"
          ? "knowledge"
          : entry.status === "pending"
            ? "inbox"
            : "search";
      Object.assign(filters, { query: "", entryType: "", status: "", tag: "" });
      selectedId.value = id;
      detail.value = null;
      await loadEntries(true, () => requestId === openRequestId);
      if (
        requestId !== openRequestId ||
        selectionRequestId !== selectRequestId
      ) {
        return;
      }
      selectedId.value = id;
      detail.value = entry;
      if (!items.value.some((item) => item.id === id)) {
        items.value = [toListItem(entry), ...items.value];
      }
    } catch (openError) {
      if (requestId === openRequestId) {
        error.value = getErrorMessage(openError, "打开关联条目失败");
      }
    } finally {
      if (requestId === openRequestId) {
        detailLoading.value = false;
      }
    }
  }

  async function updateSelected(
    patch: EntryPatch,
    expectedRevision: number,
  ): Promise<EntryDetail> {
    if (!detail.value) {
      throw new Error("未选择条目");
    }
    const updated = await entriesUpdate(
      detail.value.id,
      patch,
      expectedRevision,
    );
    detail.value = updated;
    upsertListItem(updated);
    await refreshTags();
    return updated;
  }

  function applySavedEntry(
    updated: EntryDetail,
    requestGeneration = selectionGeneration.value,
  ) {
    const updatesCurrentSelection =
      selectedId.value === updated.id &&
      selectionGeneration.value === requestGeneration;
    if (updatesCurrentSelection) detail.value = updated;
    applyEntryListUpdate(updated, updatesCurrentSelection);
  }

  function applyEntryListUpdate(updated: EntryDetail, reloadSearch = false) {
    if (reloadSearch && view.value === "search" && filters.query.trim()) {
      void reconcileCurrentList();
    } else if (!upsertListItem(updated)) {
      void reconcileCurrentList();
    }
    void refreshTags();
  }

  async function moveSelectedToTrash() {
    if (!detail.value) return;
    const requestEntry = detail.value;
    const requestGeneration = selectionGeneration.value;
    error.value = null;
    try {
      const updated = await entriesMoveToTrash(
        requestEntry.id,
        requestEntry.revision,
      );
      if (
        selectedId.value !== requestEntry.id ||
        selectionGeneration.value !== requestGeneration
      ) {
        await reconcileCurrentList();
        await refreshTags();
        return;
      }
      detail.value = updated;
      await load();
      await refreshTags();
    } catch (operationError) {
      if (
        selectedId.value === requestEntry.id &&
        selectionGeneration.value === requestGeneration
      ) {
        error.value = getErrorMessage(operationError, "移到回收站失败");
      }
    }
  }

  async function restoreSelected() {
    if (!detail.value) return;
    const requestEntry = detail.value;
    const requestGeneration = selectionGeneration.value;
    error.value = null;
    try {
      const updated = await entriesRestoreFromTrash(
        requestEntry.id,
        requestEntry.revision,
      );
      if (
        selectedId.value !== requestEntry.id ||
        selectionGeneration.value !== requestGeneration
      ) {
        await reconcileCurrentList();
        await refreshTags();
        return;
      }
      detail.value = updated;
      await load();
      await refreshTags();
    } catch (operationError) {
      if (
        selectedId.value === requestEntry.id &&
        selectionGeneration.value === requestGeneration
      ) {
        error.value = getErrorMessage(operationError, "恢复失败");
      }
    }
  }

  async function deleteSelectedForever() {
    if (!detail.value) return;
    const requestEntryId = detail.value.id;
    const requestGeneration = selectionGeneration.value;
    error.value = null;
    try {
      await entriesDeleteForever(requestEntryId);
      if (
        selectedId.value !== requestEntryId ||
        selectionGeneration.value !== requestGeneration
      ) {
        await reconcileCurrentList();
        await refreshTags();
        return;
      }
      detail.value = null;
      selectedId.value = null;
      await load();
      await refreshTags();
    } catch (operationError) {
      if (
        selectedId.value === requestEntryId &&
        selectionGeneration.value === requestGeneration
      ) {
        error.value = getErrorMessage(operationError, "永久删除失败");
      }
    }
  }

  async function setView(nextView: AppView) {
    const nextTag = nextView === "tags" ? filters.tag : "";
    const queryChanged = view.value !== nextView || filters.tag !== nextTag;
    beginSelection();
    view.value = nextView;
    filters.tag = nextTag;
    selectedId.value = null;
    detail.value = null;
    await loadEntries(true, () => true, false, queryChanged);
    await refreshTags();
  }

  async function setTagFilter(tag: string) {
    const queryChanged = view.value !== "tags" || filters.tag !== tag;
    beginSelection();
    filters.tag = tag;
    view.value = "tags";
    selectedId.value = null;
    detail.value = null;
    await loadEntries(true, () => true, false, queryChanged);
  }

  async function setTypeFilter(value: EntryType | "") {
    const queryChanged = filters.entryType !== value;
    filters.entryType = value;
    await loadEntries(true, () => true, false, queryChanged);
  }

  async function setStatusFilter(value: EntryStatus | "") {
    const queryChanged = filters.status !== value;
    filters.status = value;
    await loadEntries(true, () => true, false, queryChanged);
  }

  let pendingSearchQueryChange = false;

  function setQuery(value: string, switchView = true) {
    const nextView = switchView && value.trim() ? "search" : view.value;
    pendingSearchQueryChange ||=
      filters.query !== value || view.value !== nextView;
    filters.query = value;
    view.value = nextView;
    if (searchTimer) {
      clearTimeout(searchTimer);
    }
    searchTimer = setTimeout(() => {
      searchTimer = null;
      const clearExisting = pendingSearchQueryChange;
      pendingSearchQueryChange = false;
      void loadEntries(true, () => true, false, clearExisting);
    }, SEARCH_DEBOUNCE_MS);
  }

  function getErrorMessage(operationError: unknown, fallback: string) {
    return operationError instanceof Error ? operationError.message : fallback;
  }

  function upsertListItem(updated: EntryDetail) {
    const index = items.value.findIndex((item) => item.id === updated.id);
    if (index < 0) return false;
    if (!matchesCurrentFilter(updated)) {
      items.value.splice(index, 1);
      // Clear selected detail if the entry no longer matches the current filter.
      if (selectedId.value === updated.id) {
        beginSelection();
        selectedId.value = null;
        detail.value = null;
      }
      return true;
    }
    items.value.splice(index, 1, toListItem(updated));
    return true;
  }

  function toListItem(updated: EntryDetail): EntryListItem {
    return {
      id: updated.id,
      title: updated.title,
      summary: updated.currentContent.split(/\s+/).join(" ").slice(0, 120),
      entryType: updated.entryType,
      status: updated.status,
      knowledgeState: updated.knowledgeState,
      searchSnippet: null,
      tags: updated.tags,
      revision: updated.revision,
      createdAt: updated.createdAt,
      updatedAt: updated.updatedAt,
      deletedAt: updated.deletedAt,
    };
  }

  function matchesCurrentFilter(entry: EntryDetail) {
    const filter = buildEntryFilter(view.value, filters);
    return entryMatchesCurrentFilter(entry, filter);
  }

  return {
    view,
    filters,
    items,
    tags,
    selectedId,
    selectedItem,
    selectionGeneration,
    detail,
    loading,
    detailLoading,
    error,
    tagsError,
    externalChangeToken,
    hasMore,
    load,
    loadMore,
    refreshTags,
    select,
    openEntry,
    noteExternalChange,
    updateSelected,
    applySavedEntry,
    applyEntryListUpdate,
    moveSelectedToTrash,
    restoreSelected,
    deleteSelectedForever,
    setView,
    setTagFilter,
    setTypeFilter,
    setStatusFilter,
    setQuery,
  };
});
