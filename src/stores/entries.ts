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
  type UiFilters,
} from "../composables/useEntryFilters";

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
  const hasMore = ref(false);
  const offset = ref(0);
  const limit = 50;
  let loadRequestId = 0;
  let selectRequestId = 0;

  const selectedItem = computed(() =>
    items.value.find((item) => item.id === selectedId.value),
  );

  async function load(resetPage = true) {
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
      if (requestId !== loadRequestId) {
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
      if (requestId === loadRequestId) {
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
    offset.value += limit;
    await load(false);
  }

  async function refreshTags() {
    tags.value = await tagsList();
  }

  async function select(id: string) {
    const requestId = ++selectRequestId;
    selectedId.value = id;
    detailLoading.value = true;
    try {
      const entry = await entriesGet(id);
      if (requestId === selectRequestId && selectedId.value === id) {
        detail.value = entry;
      }
    } finally {
      if (requestId === selectRequestId) {
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
    upsertListItem(updated);
    void refreshTags();
  }

  async function moveSelectedToTrash() {
    if (!detail.value) {
      return;
    }
    detail.value = await entriesMoveToTrash(detail.value.id);
    await load();
    await refreshTags();
  }

  async function restoreSelected() {
    if (!detail.value) {
      return;
    }
    detail.value = await entriesRestoreFromTrash(detail.value.id);
    await load();
    await refreshTags();
  }

  async function deleteSelectedForever() {
    if (!detail.value) {
      return;
    }
    await entriesDeleteForever(detail.value.id);
    detail.value = null;
    selectedId.value = null;
    await load();
    await refreshTags();
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

  async function setQuery(value: string) {
    filters.query = value;
    if (value.trim()) {
      view.value = "search";
    }
    await load();
  }

  function upsertListItem(updated: EntryDetail) {
    if (!matchesCurrentFilter(updated)) {
      items.value = items.value.filter((item) => item.id !== updated.id);
      return;
    }
    const listItem: EntryListItem = {
      id: updated.id,
      title: updated.title,
      summary: updated.currentContent.split(/\s+/).join(" ").slice(0, 120),
      entryType: updated.entryType,
      status: updated.status,
      tags: updated.tags,
      revision: updated.revision,
      createdAt: updated.createdAt,
      updatedAt: updated.updatedAt,
      deletedAt: updated.deletedAt,
    };
    const index = items.value.findIndex((item) => item.id === updated.id);
    if (index >= 0) {
      items.value[index] = listItem;
    }
  }

  function matchesCurrentFilter(entry: EntryDetail): boolean {
    const filter = buildEntryFilter(view.value, filters);
    if (filter.trashOnly && !entry.deletedAt) {
      return false;
    }
    if (!filter.includeDeleted && entry.deletedAt) {
      return false;
    }
    if (filter.status && entry.status !== filter.status) {
      return false;
    }
    if (filter.entryType && entry.entryType !== filter.entryType) {
      return false;
    }
    if (filter.tag) {
      const tag = filter.tag.trim().toLocaleLowerCase();
      if (!entry.tags.some((item) => item.name.toLocaleLowerCase() === tag)) {
        return false;
      }
    }
    if (filter.query) {
      const query = filter.query.toLocaleLowerCase();
      const haystack = [
        entry.title || "",
        entry.currentContent,
        entry.originalContent,
        ...entry.tags.map((tag) => tag.name),
      ]
        .join("\n")
        .toLocaleLowerCase();
      if (!haystack.includes(query)) {
        return false;
      }
    }
    return true;
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
    hasMore,
    load,
    loadMore,
    refreshTags,
    select,
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
