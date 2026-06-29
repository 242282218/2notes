import { describe, expect, it } from "vitest";

import { buildEntryFilter, normalizeTagNames } from "./useEntryFilters";

describe("entry filters", () => {
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
