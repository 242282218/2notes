import { defineStore } from "pinia";
import { computed, reactive, ref } from "vue";

import type { AppView } from "../app/routes";
import {
  entriesCreate,
  entriesDeleteForever,
  entriesGet,
  entriesList,
  entriesMoveToTrash,
  entriesRestoreFromTrash,
  entriesUpdate,
} from "../services/entryApi";
import {
  knowledgeDemote,
  knowledgeMove,
  knowledgePromote,
} from "../services/knowledgeApi";
import { tagsList } from "../services/tagApi";
import { toErrorMessage, isKnownNonRecoverable } from "../utils/errors";
import { PAGE_SIZE, SUMMARY_MAX_CHARS } from "../constants/limits";
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
import { trackPendingOperation } from "../composables/usePendingOperations";

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

  function clearError() {
    error.value = null;
    errorRecoverable.value = true;
  }

  function reportError(cause: unknown, fallback: string) {
    error.value = toErrorMessage(cause, fallback);
    errorRecoverable.value = !isKnownNonRecoverable(cause);
  }
  const loading = ref(false);
  const detailLoading = ref(false);
  const error = ref<string | null>(null);
  /** True while `error` holds a failure that retrying cannot fix. */
  const errorRecoverable = ref(true);
  const tagsError = ref<string | null>(null);
  const externalChangeToken = ref(0);
  /** Bumped when the knowledge tree structure may have changed. */
  const knowledgeTreeToken = ref(0);
  /** Bumped when the knowledge health report may be stale. */
  const healthToken = ref(0);
  const hasMore = ref(false);
  const offset = ref(0);
  const limit = PAGE_SIZE;
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
    clearError();
    const requestOffset = resetPage ? 0 : offset.value;
    if (resetPage) {
      if (clearExisting) {
        items.value = [];
        offset.value = 0;
        hasMore.value = false;
      }
    }
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
      offset.value = page.offset + page.items.length;
      hasMore.value = page.hasMore;
      if (!preserveSelection) {
        if (!selectedId.value && page.items[0]) {
          await select(page.items[0].id);
        }
        if (requestId !== loadRequestId || !isCurrent()) {
          return;
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
        reportError(loadError, "加载失败");
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
    await load(false);
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
        tagsError.value = toErrorMessage(cause, "标签加载失败");
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
    clearError();
    try {
      const entry = await entriesGet(id);
      if (requestId === selectRequestId && selectedId.value === id) {
        detail.value = entry;
      }
    } catch (selectError) {
      if (requestId === selectRequestId) {
        reportError(selectError, "加载失败");
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
    cancelPendingSearch();
    beginSelection();
    const requestId = ++openRequestId;
    const selectionRequestId = ++selectRequestId;
    clearError();
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
      // A linked entry outside the first page is pinned locally. The offset was
      // already advanced by the reset-page load, so do not bump it again here:
      // the entry is not the server-side first item (it would already be listed).
      if (!items.value.some((item) => item.id === id)) {
        items.value = [toListItem(entry), ...items.value];
      }
    } catch (openError) {
      if (requestId === openRequestId) {
        reportError(openError, "打开关联条目失败");
      }
    } finally {
      if (requestId === openRequestId) {
        detailLoading.value = false;
      }
    }
  }

  async function createAndSelect(): Promise<EntryDetail | null> {
    const requestGeneration = selectionGeneration.value;
    clearError();
    try {
      const entry = await trackPendingOperation(entriesCreate());
      if (matchesCurrentFilter(entry) && !upsertListItem(entry)) {
        items.value = [toListItem(entry), ...items.value];
        offset.value += 1;
      }
      if (selectionGeneration.value !== requestGeneration) return null;

      beginSelection();
      openRequestId += 1;
      selectRequestId += 1;
      selectedId.value = entry.id;
      detail.value = entry;
      detailLoading.value = false;
      return entry;
    } catch (createError) {
      if (selectionGeneration.value === requestGeneration) {
        reportError(createError, "新建条目失败");
      }
      throw createError;
    }
  }

  async function saveSelected(
    id: string,
    patch: EntryPatch,
    expectedRevision: number,
  ): Promise<EntryDetail> {
    const updated = await trackPendingOperation(
      entriesUpdate(id, patch, expectedRevision),
    );
    healthToken.value += 1;
    return updated;
  }

  async function promoteKnowledge(
    id: string,
    expectedRevision: number,
  ): Promise<EntryDetail> {
    const updated = await trackPendingOperation(
      knowledgePromote(id, expectedRevision),
    );
    bumpKnowledgeTokens();
    return updated;
  }

  async function demoteKnowledge(
    id: string,
    expectedRevision: number,
  ): Promise<EntryDetail> {
    const updated = await trackPendingOperation(
      knowledgeDemote(id, expectedRevision),
    );
    bumpKnowledgeTokens();
    return updated;
  }

  async function moveKnowledge(
    id: string,
    parentId: string | null,
    expectedRevision: number,
  ): Promise<EntryDetail> {
    const updated = await trackPendingOperation(
      knowledgeMove(id, parentId, 0, expectedRevision),
    );
    bumpKnowledgeTokens();
    return updated;
  }

  function bumpKnowledgeTokens() {
    knowledgeTreeToken.value += 1;
    healthToken.value += 1;
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
    clearError();
    try {
      const updated = await trackPendingOperation(
        entriesMoveToTrash(requestEntry.id, requestEntry.revision),
      );
      bumpKnowledgeTokens();
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
        reportError(operationError, "移到回收站失败");
      }
    }
  }

  async function restoreSelected() {
    if (!detail.value) return;
    const requestEntry = detail.value;
    const requestGeneration = selectionGeneration.value;
    clearError();
    try {
      const updated = await trackPendingOperation(
        entriesRestoreFromTrash(requestEntry.id, requestEntry.revision),
      );
      bumpKnowledgeTokens();
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
        reportError(operationError, "恢复失败");
      }
    }
  }

  async function deleteSelectedForever() {
    if (!detail.value) return;
    const requestEntryId = detail.value.id;
    const requestGeneration = selectionGeneration.value;
    clearError();
    try {
      await trackPendingOperation(entriesDeleteForever(requestEntryId));
      bumpKnowledgeTokens();
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
        reportError(operationError, "永久删除失败");
      }
    }
  }

  async function setView(nextView: AppView) {
    const nextTag = nextView === "tags" ? filters.tag : "";
    const queryChanged = view.value !== nextView || filters.tag !== nextTag;
    cancelPendingSearch();
    beginSelection();
    view.value = nextView;
    filters.tag = nextTag;
    selectedId.value = null;
    detail.value = null;
    if (nextView === "health") {
      return;
    }
    await loadEntries(true, () => true, false, queryChanged);
    await refreshTags();
  }

  async function setTagFilter(tag: string) {
    const queryChanged = view.value !== "tags" || filters.tag !== tag;
    cancelPendingSearch();
    beginSelection();
    filters.tag = tag;
    view.value = "tags";
    selectedId.value = null;
    detail.value = null;
    await loadEntries(true, () => true, false, queryChanged);
  }

  async function setTypeFilter(value: EntryType | "") {
    const queryChanged = filters.entryType !== value;
    cancelPendingSearch();
    filters.entryType = value;
    await loadEntries(true, () => true, false, queryChanged);
  }

  async function setStatusFilter(value: EntryStatus | "") {
    const queryChanged = filters.status !== value;
    cancelPendingSearch();
    filters.status = value;
    await loadEntries(true, () => true, false, queryChanged);
  }

  let pendingSearchQueryChange = false;

  function cancelPendingSearch() {
    if (searchTimer) {
      clearTimeout(searchTimer);
      searchTimer = null;
    }
    pendingSearchQueryChange = false;
  }

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

  function upsertListItem(updated: EntryDetail) {
    const index = items.value.findIndex((item) => item.id === updated.id);
    if (index < 0) return false;
    if (!matchesCurrentFilter(updated)) {
      items.value.splice(index, 1);
      offset.value = Math.max(0, offset.value - 1);
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
      summary: updated.currentContent
        .split(/\s+/)
        .join(" ")
        .slice(0, SUMMARY_MAX_CHARS),
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
    errorRecoverable,
    tagsError,
    externalChangeToken,
    knowledgeTreeToken,
    healthToken,
    hasMore,
    load,
    loadMore,
    refreshTags,
    reconcileCurrentList,
    select,
    openEntry,
    createAndSelect,
    noteExternalChange,
    saveSelected,
    applySavedEntry,
    applyEntryListUpdate,
    promoteKnowledge,
    demoteKnowledge,
    moveKnowledge,
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
