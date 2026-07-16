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

  it("tracks external changes with a monotonic token", () => {
    const store = useEntriesStore();

    expect(store.externalChangeToken).toBe(0);

    store.noteExternalChange();

    expect(store.externalChangeToken).toBe(1);
  });

  it.each([
    ["trash", { deletedAt: "2026-07-16T00:00:00Z" }],
    ["knowledge", { knowledgeState: "knowledge" as const }],
    ["inbox", { status: "pending" as const }],
    ["search", { status: "done" as const }],
  ])("opens a related entry in the %s view", async (expectedView, patch) => {
    const store = useEntriesStore();
    const target = { ...entry("target"), ...patch };
    store.filters.query = "old query";
    store.filters.entryType = "idea";
    store.filters.status = "archived";
    store.filters.tag = "old-tag";
    vi.mocked(entriesGet).mockResolvedValue(target);
    vi.mocked(entriesList).mockResolvedValue(emptyPage());

    await store.openEntry(target.id);

    expect(store.view).toBe(expectedView);
    expect(store.filters).toEqual({
      query: "",
      entryType: "",
      status: "",
      tag: "",
    });
    expect(store.selectedId).toBe(target.id);
    expect(store.detail).toEqual(target);
    expect(store.items[0]?.id).toBe(target.id);
  });

  it("discards an older open request after its entry response arrives", async () => {
    const store = useEntriesStore();
    const first = deferred<EntryDetail>();
    const second = deferred<EntryDetail>();
    vi.mocked(entriesGet)
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    vi.mocked(entriesList).mockResolvedValue(emptyPage());

    const firstOpen = store.openEntry("first");
    const secondOpen = store.openEntry("second");
    second.resolve(entry("second"));
    await secondOpen;
    first.resolve(entry("first"));
    await firstOpen;

    expect(store.selectedId).toBe("second");
    expect(store.detail?.id).toBe("second");
  });

  it("discards an older open request after its list response arrives", async () => {
    const store = useEntriesStore();
    const firstPage = deferred<ReturnType<typeof emptyPage>>();
    vi.mocked(entriesGet)
      .mockResolvedValueOnce(entry("first"))
      .mockResolvedValueOnce(entry("second"));
    vi.mocked(entriesList)
      .mockReturnValueOnce(firstPage.promise)
      .mockResolvedValueOnce(emptyPage());

    const firstOpen = store.openEntry("first");
    await vi.waitFor(() => expect(entriesList).toHaveBeenCalledOnce());
    await store.openEntry("second");
    firstPage.resolve(emptyPage());
    await firstOpen;

    expect(store.selectedId).toBe("second");
    expect(store.detail?.id).toBe("second");
    expect(store.items[0]?.id).toBe("second");
  });

  it("lets a normal selection cancel an open request waiting on the list", async () => {
    const store = useEntriesStore();
    const pendingPage = deferred<ReturnType<typeof emptyPage>>();
    vi.mocked(entriesGet)
      .mockResolvedValueOnce(entry("related"))
      .mockResolvedValueOnce(entry("selected"));
    vi.mocked(entriesList).mockReturnValueOnce(pendingPage.promise);

    const opening = store.openEntry("related");
    await vi.waitFor(() => expect(entriesList).toHaveBeenCalledOnce());
    await store.select("selected");
    pendingPage.resolve(emptyPage());
    await opening;

    expect(store.selectedId).toBe("selected");
    expect(store.detail?.id).toBe("selected");
  });

  it("clears detail loading when opening an entry cancels a selection", async () => {
    const store = useEntriesStore();
    const pendingSelection = deferred<EntryDetail>();
    vi.mocked(entriesGet)
      .mockReturnValueOnce(pendingSelection.promise)
      .mockResolvedValueOnce(entry("related"));
    vi.mocked(entriesList).mockResolvedValue(emptyPage());

    const selecting = store.select("old");
    expect(store.detailLoading).toBe(true);
    await store.openEntry("related");
    pendingSelection.resolve(entry("old"));
    await selecting;

    expect(store.detailLoading).toBe(false);
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

  it("coalesces rapid search queries before calling the backend", async () => {
    vi.useFakeTimers();
    try {
      const store = useEntriesStore();
      vi.mocked(entriesList).mockResolvedValue(emptyPage());

      void store.setQuery("知");
      void store.setQuery("知识");
      void store.setQuery("知识库");

      expect(store.filters.query).toBe("知识库");
      expect(entriesList).not.toHaveBeenCalled();

      await vi.advanceTimersByTimeAsync(150);

      expect(entriesList).toHaveBeenCalledOnce();
    } finally {
      vi.useRealTimers();
    }
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

function emptyPage() {
  return {
    items: [],
    limit: 50,
    offset: 0,
    hasMore: false,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}
