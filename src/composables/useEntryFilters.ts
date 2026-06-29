import type { EntryListFilter, EntryStatus, EntryType } from "../types/generated";
import type { AppView } from "../app/routes";

export interface UiFilters {
  query: string;
  entryType: EntryType | "";
  status: EntryStatus | "";
  tag: string;
}

export function buildEntryFilter(
  view: AppView,
  filters: UiFilters,
): EntryListFilter {
  return {
    query: filters.query.trim() || null,
    entryType: filters.entryType || null,
    status:
      filters.status ||
      (view === "inbox" ? "pending" : null),
    tag: filters.tag.trim() || null,
    includeDeleted: view === "trash",
    trashOnly: view === "trash",
  };
}

export function normalizeTagNames(names: string[]): string[] {
  const seen = new Set<string>();
  const normalized: string[] = [];
  for (const name of names) {
    const trimmed = name.trim();
    const key = trimmed.toLocaleLowerCase();
    if (!trimmed || seen.has(key)) {
      continue;
    }
    seen.add(key);
    normalized.push(trimmed);
  }
  return normalized;
}
