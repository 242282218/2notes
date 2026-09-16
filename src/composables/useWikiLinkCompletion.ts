import { WIKI_LINK_SUGGEST_MAX } from "../constants/limits";

export interface WikiLinkCompletion {
  start: number;
  query: string;
}

export interface AppliedWikiLinkCompletion {
  value: string;
  caret: number;
}

export function findWikiLinkCompletion(
  value: string,
  caret: number,
): WikiLinkCompletion | null {
  const safeCaret = Math.min(Math.max(caret, 0), value.length);
  if (safeCaret < 2) return null;

  const open = value.lastIndexOf("[[", safeCaret - 2);
  if (open < 0 || value.lastIndexOf("]]", safeCaret - 2) > open) return null;
  if (value[open - 1] === "\\" || value[open - 1] === "!") return null;
  const restOfLine = value.slice(safeCaret).split(/[\r\n]/, 1)[0];
  if (restOfLine.includes("]]")) return null;

  const query = value.slice(open + 2, safeCaret);
  if (
    Array.from(query).length > WIKI_LINK_SUGGEST_MAX ||
    query.includes("|") ||
    /[\r\n]/.test(query)
  ) {
    return null;
  }
  return { start: open + 2, query };
}

// Mirrors the backend wiki-link parser's rejection rules: a title containing
// `|`, newlines, closing brackets, or exceeding the suggestion window can never
// form a resolvable link, so it must not be inserted as one.
export function sanitizeWikiLinkTitle(title: string): string | null {
  const trimmed = title.trim();
  if (
    !trimmed ||
    Array.from(trimmed).length > WIKI_LINK_SUGGEST_MAX ||
    trimmed.includes("|") ||
    trimmed.includes("]]") ||
    /[\r\n]/.test(trimmed)
  ) {
    return null;
  }
  return trimmed;
}

export function applyWikiLinkCompletion(
  value: string,
  completion: WikiLinkCompletion,
  title: string,
): AppliedWikiLinkCompletion | null {
  const safeTitle = sanitizeWikiLinkTitle(title);
  if (safeTitle === null) return null;
  const caret = completion.start + completion.query.length;
  const replacement = `${safeTitle}]]`;
  return {
    value: `${value.slice(0, completion.start)}${replacement}${value.slice(caret)}`,
    caret: completion.start + replacement.length,
  };
}
