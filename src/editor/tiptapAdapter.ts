/**
 * Adapter between the application `BlockDocument` (the persistence contract) and the
 * Tiptap/ProseMirror JSON shape used by the editor. The editor's JSON is never persisted
 * directly: `toTiptapDocument` runs before hydrating the editor and `fromTiptapDocument`
 * runs to produce the patch payload sent to the Rust aggregator.
 *
 * Stability rules (matching the stage-3 plan §2 architecture):
 * - Block UUIDs that are valid and unique survive the round trip unmodified. They are
 *   carried in the node `attrs` under `BLOCK_ID_ATTR` so the editor (and pastes that
 *   duplicate a node's attrs) cannot lose them.
 * - Missing, non-UUID-v4, or duplicate ids are regenerated on load — never silently
 *   forwarded. The same dedup also repairs paste-induced duplicates.
 * - Inline nodes/marks outside the domain (underline, script, event handlers) are dropped
 *   rather than letting editor-private schema leak into storage.
 * - An empty tiptap doc yields a single empty paragraph so the document always has one block.
 */
import type {
  BlockAttrs,
  BlockDocument,
  BlockNode,
  InlineMark,
  InlineNode,
} from "../types/generated";

import {
  DOCUMENT_SCHEMA_VERSION,
  defaultBlockAttrs,
  isUuidV4,
  newBlockId,
  emptyParagraphNode,
} from "./blockDocument";

export interface TiptapNode {
  type: string;
  attrs?: Record<string, unknown>;
  content?: TiptapNode[];
  marks?: TiptapMark[];
  text?: string;
}

export interface TiptapMark {
  type: string;
  attrs?: Record<string, unknown>;
}

const EMPTY_ATTRS: BlockAttrs = { level: null, language: null, start: null };

/** Stable attrs slot that carries the app block id across the editor boundary. */
export const BLOCK_ID_ATTR = "blockId";

/** Convert an app document to the Tiptap doc shape used by `Editor#setContent`. */
export function toTiptapDocument(document: BlockDocument): TiptapNode {
  return {
    type: "doc",
    content: document.blocks.map(blockToTiptap),
  };
}

/** Convert Tiptap doc JSON back to the app document. Regenerates any bad ids. */
export function fromTiptapDocument(
  node: TiptapNode | null | undefined,
): BlockDocument {
  const normalized =
    node && typeof node === "object" && node.type === "doc"
      ? node
      : { type: "doc" };
  const rawChildren = Array.isArray(normalized.content)
    ? normalized.content.flatMap(blockFromTiptap)
    : [];
  const blocks = rawChildren.length > 0 ? rawChildren : [emptyParagraphNode()];
  const document: BlockDocument = {
    schemaVersion: DOCUMENT_SCHEMA_VERSION,
    blocks: regenerateUnstableIds(blocks),
  };
  return document;
}

function attrsWithId(
  id: string,
  extras: Record<string, unknown>,
): Record<string, unknown> {
  return { ...extras, [BLOCK_ID_ATTR]: id };
}

function readId(attrs: Record<string, unknown> | undefined): string {
  const candidate = attrs?.[BLOCK_ID_ATTR];
  return typeof candidate === "string" ? candidate : "";
}

function emptyParagraphTiptapNode(): TiptapNode {
  return {
    type: "paragraph",
    attrs: attrsWithId(newBlockId(), {}),
    content: [],
  };
}

function emptyListItemTiptapNode(): TiptapNode {
  return {
    type: "listItem",
    attrs: attrsWithId(newBlockId(), {}),
    content: [emptyParagraphTiptapNode()],
  };
}

function blockToTiptap(node: BlockNode): TiptapNode {
  switch (node.kind) {
    case "paragraph":
      return {
        type: "paragraph",
        attrs: attrsWithId(node.id, {}),
        content: inlineToTiptap(node.content),
      };
    case "heading": {
      const level = node.attrs.level ?? 1;
      return {
        type: "heading",
        attrs: attrsWithId(node.id, { level }),
        content: inlineToTiptap(node.content),
      };
    }
    case "bulletList":
      return {
        type: "bulletList",
        attrs: attrsWithId(node.id, {}),
        content:
          node.children.length > 0
            ? node.children.map(blockToTiptap)
            : [emptyListItemTiptapNode()],
      };
    case "orderedList": {
      const start = node.attrs.start ?? 1;
      return {
        type: "orderedList",
        attrs: attrsWithId(node.id, { start }),
        content:
          node.children.length > 0
            ? node.children.map(blockToTiptap)
            : [emptyListItemTiptapNode()],
      };
    }
    case "listItem":
      // First paragraph carries the inline content; any nested blocks (lists/paragraphs
      // for nested list trees) are appended as sibling content nodes so Tiptap renders them
      // as nested structure under the same item.
      return {
        type: "listItem",
        attrs: attrsWithId(node.id, {}),
        content: [
          // List-item text is represented by a schema-required paragraph, but that
          // paragraph is not an app block. It still needs a UniqueID value so editor
          // initialization does not create a spurious update event.
          {
            type: "paragraph",
            attrs: attrsWithId(newBlockId(), {}),
            content: inlineToTiptap(node.content),
          },
          ...node.children.map(blockToTiptap),
        ],
      };
    case "blockquote":
      return {
        type: "blockquote",
        attrs: attrsWithId(node.id, {}),
        // ProseMirror requires a text block inside a blockquote. The persistence
        // schema allows empty containers, so hydrate one as an empty paragraph.
        content:
          node.children.length > 0
            ? node.children.map(blockToTiptap)
            : [emptyParagraphTiptapNode()],
      };
    case "codeBlock": {
      const language = node.attrs.language ?? null;
      return {
        type: "codeBlock",
        attrs: attrsWithId(node.id, { language }),
        content: inlineToTiptap(node.content),
      };
    }
    case "horizontalRule":
      return { type: "horizontalRule", attrs: attrsWithId(node.id, {}) };
  }
}

function inlineToTiptap(content: InlineNode[]): TiptapNode[] {
  if (content.length === 0) return [];
  const nodes: TiptapNode[] = [];
  for (const node of content) {
    if (node.type === "hardBreak") {
      nodes.push({ type: "hardBreak" });
      continue;
    }
    nodes.push(inlineNodeToTiptap(node));
  }
  return nodes;
}

function inlineNodeToTiptap(node: InlineNode): TiptapNode {
  if (node.type === "hardBreak") {
    return { type: "hardBreak" };
  }
  return {
    type: "text",
    text: node.text,
    marks: node.marks.map(markToTiptap),
  };
}

function markToTiptap(mark: InlineMark): TiptapMark {
  if (mark.type === "link") {
    return {
      type: "link",
      attrs: {
        href: mark.href,
        target: "_blank",
        rel: "noopener noreferrer nofollow",
      },
    };
  }
  return { type: mark.type };
}

function blockFromTiptap(node: TiptapNode): BlockNode[] {
  const id = readId(node.attrs);
  switch (node.type) {
    case "paragraph":
      return [
        {
          id,
          kind: "paragraph",
          attrs: { ...EMPTY_ATTRS },
          content: inlineFromTiptap(node.content),
          children: [],
        },
      ];
    case "heading": {
      const level = clampLevel(node.attrs?.level);
      return [
        {
          id,
          kind: "heading",
          attrs: { level, language: null, start: null },
          content: inlineFromTiptap(node.content),
          children: [],
        },
      ];
    }
    case "bulletList":
      return [
        {
          id,
          kind: "bulletList",
          attrs: { ...EMPTY_ATTRS },
          content: [],
          children: (node.content ?? []).flatMap(blockFromTiptap),
        },
      ];
    case "orderedList": {
      const start = clampStart(node.attrs?.start);
      return [
        {
          id,
          kind: "orderedList",
          attrs: { level: null, language: null, start },
          content: [],
          children: (node.content ?? []).flatMap(blockFromTiptap),
        },
      ];
    }
    case "listItem": {
      // The first paragraph carries inline text; remaining siblings become nested children.
      const children = (node.content ?? []).flatMap(blockFromTiptap);
      const [first, ...rest] = children;
      if (first && first.kind === "paragraph") {
        return [
          {
            id,
            kind: "listItem",
            attrs: { ...EMPTY_ATTRS },
            content: first.content,
            children: rest,
          },
        ];
      }
      return [
        {
          id,
          kind: "listItem",
          attrs: { ...EMPTY_ATTRS },
          content: [],
          children,
        },
      ];
    }
    case "blockquote":
      return [
        {
          id,
          kind: "blockquote",
          attrs: { ...EMPTY_ATTRS },
          content: [],
          children: (node.content ?? []).flatMap(blockFromTiptap),
        },
      ];
    case "codeBlock":
      return [
        {
          id,
          kind: "codeBlock",
          attrs: {
            level: null,
            language:
              typeof node.attrs?.language === "string"
                ? node.attrs.language
                : null,
            start: null,
          },
          content: inlineFromTiptap(node.content),
          children: [],
        },
      ];
    case "horizontalRule":
      return [
        {
          id,
          kind: "horizontalRule",
          attrs: { ...EMPTY_ATTRS },
          content: [],
          children: [],
        },
      ];
    default:
      // Unknown top-level nodes become a plain paragraph if they emit text, else skipped.
      if (hasText(node)) {
        return [
          {
            id: "",
            kind: "paragraph",
            attrs: { ...EMPTY_ATTRS },
            content: inlineFromTiptap(node.content),
            children: [],
          },
        ];
      }
      return [];
  }
}

function inlineFromTiptap(content: TiptapNode[] | undefined): InlineNode[] {
  if (!content) return [];
  const nodes: InlineNode[] = [];
  for (const child of content) {
    if (child.type === "hardBreak") {
      nodes.push({ type: "hardBreak" });
      continue;
    }
    if (child.type !== "text" || typeof child.text !== "string") {
      continue;
    }
    const text = child.text;
    const marks = (child.marks ?? [])
      .map(markFromTiptap)
      .filter((mark): mark is InlineMark => mark !== null);
    nodes.push({ type: "text", text, marks });
  }
  return nodes;
}

function markFromTiptap(mark: TiptapMark): InlineMark | null {
  switch (mark.type) {
    case "bold":
    case "italic":
    case "strike":
    case "code":
      return { type: mark.type };
    case "link": {
      const href = typeof mark.attrs?.href === "string" ? mark.attrs.href : "";
      if (!hasAllowedLinkScheme(href)) return null;
      return { type: "link", href };
    }
    default:
      // Underline and other non-domain marks are deliberately dropped.
      return null;
  }
}

function regenerateUnstableIds(blocks: BlockNode[]): BlockNode[] {
  const seen = new Set<string>();
  return blocks.map((block) => walkRegenerate(block, seen));
}

function walkRegenerate(node: BlockNode, seen: Set<string>): BlockNode {
  let nextId = node.id;
  if (!nextId || !isUuidV4(nextId) || seen.has(nextId)) {
    nextId = allocateFresh(seen);
  }
  seen.add(nextId);
  return {
    id: nextId,
    kind: node.kind,
    attrs: { ...node.attrs },
    content: node.content,
    children: node.children.map((child) => walkRegenerate(child, seen)),
  };
}

function allocateFresh(seen: Set<string>): string {
  let fresh = newBlockId();
  while (seen.has(fresh)) {
    fresh = newBlockId();
  }
  return fresh;
}

function clampLevel(raw: unknown): number | null {
  if (typeof raw !== "number" || !Number.isFinite(raw)) return null;
  const level = Math.round(raw);
  return level >= 1 && level <= 6 ? level : 1;
}

function clampStart(raw: unknown): number | null {
  if (typeof raw !== "number" || !Number.isFinite(raw) || raw < 1) return 1;
  return Math.round(raw);
}

function hasAllowedLinkScheme(href: string): boolean {
  return /^(https?:|mailto:)/i.test(href);
}

function hasText(node: TiptapNode): boolean {
  return Boolean(
    node.content &&
    node.content.some(
      (child) => child.type === "text" && typeof child.text === "string",
    ),
  );
}

export { defaultBlockAttrs };
