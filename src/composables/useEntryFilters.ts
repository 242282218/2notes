import type {
  EntryDetail,
  EntryListFilter,
  EntryStatus,
  EntryType,
} from "../types/generated";
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
    status: filters.status || (view === "inbox" ? "pending" : null),
    knowledgeState: null,
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

/**
 * Pure predicate: does this EntryDetail belong in the current filtered view?
 * Extracted so both store (client-side pruning) and filter builder can share logic.
 */
export function entryMatchesCurrentFilter(
  entry: EntryDetail,
  filter: EntryListFilter,
): boolean {
  if (filter.trashOnly && !entry.deletedAt) {
    return false;
  }
  if (!filter.includeDeleted && entry.deletedAt) {
    return false;
  }
  if (filter.status && entry.status !== filter.status) {
    return false;
  }
  if (filter.knowledgeState && entry.knowledgeState !== filter.knowledgeState) {
    return false;
  }
  if (filter.entryType && entry.entryType !== filter.entryType) {
    return false;
  }
  if (
    filter.tag &&
    !entry.tags.some(
      (tag) => tag.normalizedName === filter.tag?.trim().toLocaleLowerCase(),
    )
  ) {
    return false;
  }
  if (!filter.query) {
    return true;
  }
  const query = filter.query.toLocaleLowerCase();
  return [
    entry.title || "",
    entry.currentContent,
    entry.originalContent,
    entry.tags.map((tag) => tag.name).join(" "),
  ].some((value) => value.toLocaleLowerCase().includes(query));
}
