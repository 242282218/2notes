import { describe, expect, it } from "vitest";

import {
  buildEntryFilter,
  entryMatchesCurrentFilter,
  normalizeTagNames,
} from "./useEntryFilters";
import type { EntryDetail } from "../types/generated";

describe("entry filters", () => {
  it("separates capture inbox from knowledge view", () => {
    expect(buildEntryFilter("inbox", emptyFilters())).toEqual(
      expect.objectContaining({ status: "pending", knowledgeState: "capture" }),
    );
    expect(buildEntryFilter("knowledge", emptyFilters())).toEqual(
      expect.objectContaining({ status: null, knowledgeState: "knowledge" }),
    );
  });
  it("normalizes duplicate tag names", () => {
    expect(normalizeTagNames([" Work ", "work", "中文", ""])).toEqual([
      "Work",
      "中文",
    ]);
  });

  it("keeps inbox scoped to pending non-trash entries", () => {
    expect(
      buildEntryFilter("inbox", {
        query: "",
        entryType: "",
        status: "",
        tag: "",
      }),
    ).toMatchObject({
      status: "pending",
      includeDeleted: false,
      trashOnly: false,
    });
  });
});

describe("entryMatchesCurrentFilter", () => {
  const base: EntryDetail = {
    id: "e1",
    title: "Hello World",
    titleSource: "auto",
    originalContent: "原始",
    currentContent: "当前",
    entryType: "idea",
    status: "pending",
    knowledgeState: "capture",
    knowledgePromotedAt: null,
    knowledgeAliases: [],
    tags: [
      {
        id: "t1",
        name: "work",
        normalizedName: "work",
        createdAt: "",
        entryCount: 0,
      },
    ],
    revision: 1,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    deletedAt: null,
  };

  it("matches when filter is empty", () => {
    const filter = buildEntryFilter("inbox", {
      query: "",
      entryType: "",
      status: "",
      tag: "",
    });
    expect(entryMatchesCurrentFilter(base, filter)).toBe(true);
  });

  it("excludes knowledge entries from inbox", () => {
    expect(
      entryMatchesCurrentFilter(
        { ...base, knowledgeState: "knowledge" },
        buildEntryFilter("inbox", emptyFilters()),
      ),
    ).toBe(false);
  });

  it("filters by tag normalized name", () => {
    const filter = buildEntryFilter("tags", {
      query: "",
      entryType: "",
      status: "",
      tag: "Work",
    });
    expect(entryMatchesCurrentFilter(base, filter)).toBe(true);
  });

  it("excludes trashed when trashOnly=false", () => {
    const trashed = { ...base, deletedAt: "2026-01-02T00:00:00Z" };
    const filter = buildEntryFilter("inbox", {
      query: "",
      entryType: "",
      status: "",
      tag: "",
    });
    expect(entryMatchesCurrentFilter(trashed, filter)).toBe(false);
  });
});

function emptyFilters() {
  return { query: "", entryType: "", status: "", tag: "" } as const;
}
