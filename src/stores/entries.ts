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
  const detail = ref<EntryDetail | null>(null);
  const loading = ref(false);
  const detailLoading = ref(false);
  const error = ref<string | null>(null);
  const externalChangeToken = ref(0);
  const hasMore = ref(false);
  const offset = ref(0);
  const limit = 50;
  let loadRequestId = 0;
  let selectRequestId = 0;
  let openRequestId = 0;
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
  ) {
    const requestId = ++loadRequestId;
    loading.value = true;
    error.value = null;
    if (resetPage) {
      offset.value = 0;
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
      if (!selectedId.value && page.items[0]) {
        await select(page.items[0].id);
      }
      if (
        selectedId.value &&
        !items.value.some((item) => item.id === selectedId.value)
      ) {
        detail.value = null;
        selectedId.value = null;
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
    tags.value = await tagsList();
  }

  async function select(id: string) {
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

  function applySavedEntry(updated: EntryDetail) {
    detail.value = updated;
    if (view.value === "search" && filters.query.trim()) {
      void load();
    } else {
      upsertListItem(updated);
    }
    refreshTags().catch(() => {
      // Silently swallow tag refresh errors in autosave callback context.
      // The tags list will be refreshed on next manual action.
    });
  }

  async function moveSelectedToTrash() {
    if (!detail.value) {
      return;
    }
    error.value = null;
    try {
      detail.value = await entriesMoveToTrash(
        detail.value.id,
        detail.value.revision,
      );
      await load();
      await refreshTags();
    } catch (operationError) {
      error.value = getErrorMessage(operationError, "移到回收站失败");
    }
  }

  async function restoreSelected() {
    if (!detail.value) {
      return;
    }
    error.value = null;
    try {
      detail.value = await entriesRestoreFromTrash(
        detail.value.id,
        detail.value.revision,
      );
      await load();
      await refreshTags();
    } catch (operationError) {
      error.value = getErrorMessage(operationError, "恢复失败");
    }
  }

  async function deleteSelectedForever() {
    if (!detail.value) {
      return;
    }
    error.value = null;
    try {
      await entriesDeleteForever(detail.value.id);
      detail.value = null;
      selectedId.value = null;
      await load();
      await refreshTags();
    } catch (operationError) {
      error.value = getErrorMessage(operationError, "永久删除失败");
    }
  }

  async function setView(nextView: AppView) {
    view.value = nextView;
    filters.tag = nextView === "tags" ? filters.tag : "";
    selectedId.value = null;
    detail.value = null;
    await load();
    await refreshTags();
  }

  async function setTagFilter(tag: string) {
    filters.tag = tag;
    view.value = "tags";
    selectedId.value = null;
    detail.value = null;
    await load();
  }

  async function setTypeFilter(value: EntryType | "") {
    filters.entryType = value;
    await load();
  }

  async function setStatusFilter(value: EntryStatus | "") {
    filters.status = value;
    await load();
  }

  function setQuery(value: string, switchView = true) {
    filters.query = value;
    if (switchView && value.trim()) {
      view.value = "search";
    }
    if (searchTimer) {
      clearTimeout(searchTimer);
    }
    searchTimer = setTimeout(() => {
      searchTimer = null;
      void load();
    }, SEARCH_DEBOUNCE_MS);
  }

  function getErrorMessage(operationError: unknown, fallback: string) {
    return operationError instanceof Error ? operationError.message : fallback;
  }

  function upsertListItem(updated: EntryDetail) {
    const index = items.value.findIndex((item) => item.id === updated.id);
    if (index < 0) {
      return;
    }
    if (!matchesCurrentFilter(updated)) {
      items.value.splice(index, 1);
      // Clear selected detail if the entry no longer matches the current filter.
      if (selectedId.value === updated.id) {
        selectedId.value = null;
        detail.value = null;
      }
      return;
    }
    items.value.splice(index, 1, toListItem(updated));
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
    detail,
    loading,
    detailLoading,
    error,
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
