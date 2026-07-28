import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  entriesDeleteForever,
  entriesGet,
  entriesList,
  entriesMoveToTrash,
  entriesRestoreFromTrash,
} from "../services/entryApi";
import { tagsList } from "../services/tagApi";
import type { EntryDetail, Tag } from "../types/generated";
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
    vi.mocked(tagsList).mockReset();
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
    const pendingPage = deferred<ReturnType<typeof pageWith>>();
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

  it("does not replace the current detail with a stale saved entry", () => {
    const store = selectedStore();
    const requestGeneration = store.selectionGeneration;
    store.selectedId = "entry-b";
    store.detail = entry("entry-b");

    store.applySavedEntry(
      { ...entry("entry-a"), revision: 1 },
      requestGeneration,
    );

    expect(store.selectedId).toBe("entry-b");
    expect(store.detail?.id).toBe("entry-b");
  });

  it.each([
    ["trash", entriesMoveToTrash, "moveSelectedToTrash"],
    ["restore", entriesRestoreFromTrash, "restoreSelected"],
  ] as const)(
    "discards stale %s responses after selection changes",
    async (_, api, action) => {
      const store = selectedStore();
      const pending = deferred<EntryDetail>();
      vi.mocked(api).mockReturnValue(pending.promise);

      vi.mocked(entriesGet)
        .mockResolvedValueOnce(entry("entry-b"))
        .mockResolvedValueOnce(entry("entry-a"));
      const operation = store[action]();
      await store.select("entry-b");
      await store.select("entry-a");
      pending.resolve({ ...entry("entry-a"), revision: 1 });
      await operation;

      expect(store.selectedId).toBe("entry-a");
      expect(store.detail?.id).toBe("entry-a");
      expect(store.detail?.revision).toBe(0);
    },
  );

  it.each([
    ["trash", entriesMoveToTrash, "moveSelectedToTrash"],
    ["restore", entriesRestoreFromTrash, "restoreSelected"],
  ] as const)(
    "treats repeated selection as a new lifecycle for stale %s",
    async (_, api, action) => {
      const store = selectedStore();
      const pending = deferred<EntryDetail>();
      vi.mocked(api).mockReturnValue(pending.promise);
      vi.mocked(entriesGet).mockResolvedValue(entry("entry-a"));

      const operation = store[action]();
      await store.select("entry-a");
      pending.resolve({ ...entry("entry-a"), revision: 2 });
      await operation;

      expect(store.detail?.id).toBe("entry-a");
      expect(store.detail?.revision).toBe(0);
    },
  );

  it("treats repeated selection as a new lifecycle for stale deletion", async () => {
    const store = selectedStore();
    const pending = deferred<void>();
    vi.mocked(entriesDeleteForever).mockReturnValue(pending.promise);
    vi.mocked(entriesGet).mockResolvedValue(entry("entry-a"));

    const operation = store.deleteSelectedForever();
    await store.select("entry-a");
    pending.resolve();
    await operation;

    expect(store.selectedId).toBe("entry-a");
    expect(store.detail?.id).toBe("entry-a");
  });

  it("reconciles the current list after a stale trash response", async () => {
    const store = selectedStore();
    const pending = deferred<EntryDetail>();
    const trashed = { ...entry("entry-a"), deletedAt: "2026-07-25T00:00:00Z" };
    vi.mocked(entriesMoveToTrash).mockReturnValue(pending.promise);
    vi.mocked(entriesList).mockResolvedValue(pageWith(trashed));

    const operation = store.moveSelectedToTrash();
    store.selectedId = "entry-b";
    store.detail = entry("entry-b");
    store.view = "trash";
    store.items = pageWith(trashed).items;
    pending.resolve(trashed);
    await operation;

    expect(entriesList).toHaveBeenCalled();
    expect(store.items.map((item) => item.id)).toContain("entry-a");
    expect(store.selectedId).toBe("entry-b");
    expect(store.detail?.id).toBe("entry-b");
  });

  it("reloads a target list when a stale knowledge update is not present", async () => {
    const store = selectedStore();
    const promoted = {
      ...entry("entry-a"),
      knowledgeState: "knowledge" as const,
    };
    store.selectedId = "entry-b";
    store.detail = entry("entry-b");
    store.view = "knowledge";
    store.items = [];
    vi.mocked(entriesList).mockResolvedValue(pageWith(promoted));

    store.applyEntryListUpdate(promoted);

    await vi.waitFor(() => expect(entriesList).toHaveBeenCalled());
    expect(store.items.map((item) => item.id)).toContain("entry-a");
    expect(store.detail?.id).toBe("entry-b");
  });

  it("does not clear a newer selection after stale permanent deletion completes", async () => {
    const store = selectedStore();
    const pending = deferred<void>();
    vi.mocked(entriesDeleteForever).mockReturnValue(pending.promise);

    vi.mocked(entriesGet)
      .mockResolvedValueOnce(entry("entry-b"))
      .mockResolvedValueOnce(entry("entry-a"));
    const operation = store.deleteSelectedForever();
    await store.select("entry-b");
    await store.select("entry-a");
    pending.resolve();
    await operation;

    expect(store.selectedId).toBe("entry-a");
    expect(store.detail?.id).toBe("entry-a");
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

  it("clears stale items while a view change is loading", async () => {
    const store = useEntriesStore();
    const pendingPage = deferred<ReturnType<typeof pageWith>>();
    store.items = pageWith(entry("old-entry")).items;
    store.selectedId = "old-entry";
    store.detail = entry("old-entry");
    vi.mocked(entriesList).mockReturnValue(pendingPage.promise);

    const changingView = store.setView("knowledge");

    expect(store.view).toBe("knowledge");
    expect(store.loading).toBe(true);
    expect(store.items).toEqual([]);
    expect(store.selectedId).toBeNull();
    expect(store.detail).toBeNull();

    pendingPage.resolve(pageWith(entry("new-entry")));
    await changingView;

    expect(store.loading).toBe(false);
    expect(store.items.map((item) => item.id)).toEqual(["new-entry"]);
  });

  it("keeps current items visible during a reconcile reload", async () => {
    const store = selectedStore();
    const pendingPage = deferred<ReturnType<typeof pageWith>>();
    store.view = "knowledge";
    store.items = pageWith(entry("old-entry")).items;
    vi.mocked(entriesList).mockReturnValue(pendingPage.promise);

    store.applyEntryListUpdate({
      ...entry("new-entry"),
      knowledgeState: "knowledge",
    });
    await vi.waitFor(() => expect(entriesList).toHaveBeenCalledOnce());

    expect(store.loading).toBe(true);
    expect(store.items.map((item) => item.id)).toEqual(["old-entry"]);

    pendingPage.resolve(pageWith(entry("new-entry")));
    await vi.waitFor(() => expect(store.loading).toBe(false));
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

  it("clears items for changed search queries but not identical ones", async () => {
    vi.useFakeTimers();
    try {
      const store = useEntriesStore();
      store.items = pageWith(entry("old-entry")).items;
      const pendingPage = deferred<ReturnType<typeof pageWith>>();
      vi.mocked(entriesList).mockReturnValue(pendingPage.promise);

      store.setQuery("知识库");
      await vi.advanceTimersByTimeAsync(150);

      expect(store.loading).toBe(true);
      expect(store.items).toEqual([]);

      pendingPage.resolve(pageWith(entry("new-entry")));
      await vi.waitFor(() => expect(store.loading).toBe(false));
      expect(store.items.map((item) => item.id)).toEqual(["new-entry"]);

      const sameQueryPage = deferred<ReturnType<typeof pageWith>>();
      vi.mocked(entriesList).mockReturnValue(sameQueryPage.promise);
      store.setQuery("知识库");
      await vi.advanceTimersByTimeAsync(150);

      expect(store.loading).toBe(true);
      expect(store.items.map((item) => item.id)).toEqual(["new-entry"]);

      sameQueryPage.resolve(pageWith(entry("new-entry")));
      await vi.waitFor(() => expect(store.loading).toBe(false));
    } finally {
      vi.useRealTimers();
    }
  });

  it("clears items for type and status filter changes", async () => {
    const store = useEntriesStore();
    store.items = pageWith(entry("old-entry")).items;
    const pendingType = deferred<ReturnType<typeof pageWith>>();
    vi.mocked(entriesList).mockReturnValue(pendingType.promise);

    const changingType = store.setTypeFilter("idea");
    expect(store.loading).toBe(true);
    expect(store.items).toEqual([]);
    pendingType.resolve(pageWith(entry("typed")));
    await changingType;
    expect(store.items.map((item) => item.id)).toEqual(["typed"]);

    store.items = pageWith(entry("typed")).items;
    const pendingStatus = deferred<ReturnType<typeof pageWith>>();
    vi.mocked(entriesList).mockReturnValue(pendingStatus.promise);

    const changingStatus = store.setStatusFilter("done");
    expect(store.loading).toBe(true);
    expect(store.items).toEqual([]);
    pendingStatus.resolve(pageWith(entry("done-entry")));
    await changingStatus;
    expect(store.items.map((item) => item.id)).toEqual(["done-entry"]);
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

  it("keeps only the latest tags result when an older refresh resolves later", async () => {
    const store = useEntriesStore();
    const first = deferred<Tag[]>();
    const second = deferred<Tag[]>();
    const olderTags = [tag("old", "old")];
    const newerTags = [tag("new", "new")];
    vi.mocked(tagsList)
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);

    const firstRefresh = store.refreshTags();
    const secondRefresh = store.refreshTags();
    second.resolve(newerTags);
    await secondRefresh;
    first.resolve(olderTags);
    await firstRefresh;

    expect(store.tags).toEqual(newerTags);
    expect(store.tagsError).toBeNull();
  });

  it("keeps entries usable when tagsList fails and isolates tagsError", async () => {
    const store = useEntriesStore();
    const listed = pageWith(entry("entry-a"));
    vi.mocked(entriesList).mockResolvedValue(listed);
    vi.mocked(tagsList).mockRejectedValue(new Error("tags down"));

    await store.load();
    await expect(store.refreshTags()).resolves.toBeUndefined();

    expect(store.items.map((item) => item.id)).toEqual(["entry-a"]);
    expect(store.error).toBeNull();
    expect(store.tagsError).toBe("tags down");
    expect(store.tags).toEqual([]);
  });

  it("clears tagsError when a new refresh starts and when the latest succeeds", async () => {
    const store = useEntriesStore();
    const pending = deferred<Tag[]>();
    vi.mocked(tagsList)
      .mockRejectedValueOnce(new Error("tags down"))
      .mockReturnValueOnce(pending.promise)
      .mockResolvedValueOnce([tag("ok", "ok")]);

    await store.refreshTags();
    expect(store.tagsError).toBe("tags down");

    const second = store.refreshTags();
    expect(store.tagsError).toBeNull();
    pending.resolve([tag("pending", "pending")]);
    await second;
    expect(store.tags).toEqual([tag("pending", "pending")]);
    expect(store.tagsError).toBeNull();

    await store.refreshTags();
    expect(store.tags).toEqual([tag("ok", "ok")]);
    expect(store.tagsError).toBeNull();
  });

  it("ignores stale tags failures after a newer refresh succeeds", async () => {
    const store = useEntriesStore();
    const first = deferred<Tag[]>();
    const second = deferred<Tag[]>();
    const newerTags = [tag("fresh", "fresh")];
    vi.mocked(tagsList)
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);

    const firstRefresh = store.refreshTags();
    const secondRefresh = store.refreshTags();
    second.resolve(newerTags);
    await secondRefresh;
    first.reject(new Error("stale tags failure"));
    await firstRefresh;

    expect(store.tags).toEqual(newerTags);
    expect(store.tagsError).toBeNull();
    expect(store.error).toBeNull();
  });

  it("does not overwrite entry save errors with tagsError", async () => {
    const store = selectedStore();
    store.error = "保存失败";
    vi.mocked(tagsList).mockRejectedValue(new Error("tags down"));

    await store.refreshTags();

    expect(store.error).toBe("保存失败");
    expect(store.tagsError).toBe("tags down");
  });
});

function selectedStore() {
  const store = useEntriesStore();
  store.selectedId = "entry-a";
  store.detail = entry("entry-a");
  return store;
}

function emptyDocument() {
  return {
    schemaVersion: 1,
    blocks: [
      {
        id: "550e8400-e29b-41d4-a716-446655440000",
        kind: "paragraph" as const,
        attrs: { level: null, language: null, start: null },
        content: [],
        children: [],
      },
    ],
  };
}

function entry(id: string): EntryDetail {
  return {
    id,
    title: id,
    titleSource: "user",
    originalContent: id,
    currentContent: id,
    document: emptyDocument(),
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

function pageWith(...entries: EntryDetail[]) {
  return {
    items: entries.map((value) => ({
      id: value.id,
      title: value.title,
      summary: value.currentContent,
      entryType: value.entryType,
      status: value.status,
      knowledgeState: value.knowledgeState,
      searchSnippet: null,
      tags: value.tags,
      revision: value.revision,
      createdAt: value.createdAt,
      updatedAt: value.updatedAt,
      deletedAt: value.deletedAt,
    })),
    limit: 50,
    offset: 0,
    hasMore: false,
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

function tag(id: string, name: string): Tag {
  return {
    id,
    name,
    normalizedName: name,
    createdAt: "2026-07-15T00:00:00Z",
    entryCount: 1,
  };
}

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}
