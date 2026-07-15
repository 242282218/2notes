import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  entriesDeleteForever,
  entriesGet,
  entriesList,
  entriesMoveToTrash,
  entriesRestoreFromTrash,
} from "../services/entryApi";
import type { EntryDetail } from "../types/generated";
import { useEntriesStore } from "./entries";

vi.mock("../services/entryApi", () => ({
  entriesDeleteForever: vi.fn(),
  entriesGet: vi.fn(),
  entriesList: vi.fn(),
  entriesMoveToTrash: vi.fn(),
  entriesRestoreFromTrash: vi.fn(),
  entriesUpdate: vi.fn(),
}));

vi.mock("../services/tagApi", () => ({
  tagsList: vi.fn(),
}));

describe("entries store", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.mocked(entriesGet).mockReset();
    vi.mocked(entriesList).mockReset();
    vi.mocked(entriesMoveToTrash).mockReset();
    vi.mocked(entriesRestoreFromTrash).mockReset();
    vi.mocked(entriesDeleteForever).mockReset();
  });

  it("clears the previous detail when selecting a new entry fails", async () => {
    const store = useEntriesStore();
    store.selectedId = "entry-a";
    store.detail = entry("entry-a");
    vi.mocked(entriesGet).mockRejectedValue(new Error("load failed"));

    await store.select("entry-b");

    expect(store.selectedId).toBeNull();
    expect(store.detail).toBeNull();
    expect(store.error).toBe("load failed");
  });

  it("reloads backend search results after a saved entry changes", async () => {
    const store = selectedStore();
    store.view = "search";
    store.filters.query = "foo bar";
    store.items = [
      {
        id: "entry-a",
        title: "foo",
        summary: "bar",
        entryType: "unclear",
        status: "pending",
        knowledgeState: "capture",
        searchSnippet: null,
        tags: [],
        revision: 0,
        createdAt: "2026-07-15T00:00:00Z",
        updatedAt: "2026-07-15T00:00:00Z",
        deletedAt: null,
      },
    ];
    vi.mocked(entriesList).mockResolvedValue({
      items: store.items,
      limit: 50,
      offset: 0,
      hasMore: false,
    });

    store.applySavedEntry({
      ...entry("entry-a"),
      title: "foo",
      currentContent: "bar",
      revision: 1,
    });
    await vi.waitFor(() => expect(entriesList).toHaveBeenCalledOnce());

    expect(store.selectedId).toBe("entry-a");
    expect(store.detail?.id).toBe("entry-a");
  });

  it("keeps the selected entry and exposes trash failures", async () => {
    const store = selectedStore();
    vi.mocked(entriesMoveToTrash).mockRejectedValue(new Error("trash failed"));

    await store.moveSelectedToTrash();

    expect(store.detail?.id).toBe("entry-a");
    expect(store.error).toBe("trash failed");
  });

  it("keeps the selected entry and exposes restore failures", async () => {
    const store = selectedStore();
    vi.mocked(entriesRestoreFromTrash).mockRejectedValue(
      new Error("restore failed"),
    );

    await store.restoreSelected();

    expect(store.detail?.id).toBe("entry-a");
    expect(store.error).toBe("restore failed");
  });

  it("keeps the selected entry and exposes permanent delete failures", async () => {
    const store = selectedStore();
    vi.mocked(entriesDeleteForever).mockRejectedValue(
      new Error("delete failed"),
    );

    await store.deleteSelectedForever();

    expect(store.detail?.id).toBe("entry-a");
    expect(store.error).toBe("delete failed");
  });
});

function selectedStore() {
  const store = useEntriesStore();
  store.selectedId = "entry-a";
  store.detail = entry("entry-a");
  return store;
}

function entry(id: string): EntryDetail {
  return {
    id,
    title: id,
    titleSource: "user",
    originalContent: id,
    currentContent: id,
    entryType: "unclear",
    status: "pending",
    knowledgeState: "capture",
    knowledgePromotedAt: null,
    knowledgeAliases: [],
    tags: [],
    revision: 0,
    createdAt: "2026-07-15T00:00:00Z",
    updatedAt: "2026-07-15T00:00:00Z",
    deletedAt: null,
  };
}
