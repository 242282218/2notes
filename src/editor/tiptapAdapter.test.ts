import { describe, expect, it } from "vitest";

import type { BlockDocument, BlockNode, InlineMark } from "../types/generated";
import { emptyParagraphDocument } from "./blockDocument";
import {
  fromTiptapDocument,
  toTiptapDocument,
  type TiptapNode,
} from "./tiptapAdapter";

function paragraph(
  id: string,
  text: string,
  marks: InlineMark[] = [],
): BlockNode {
  return {
    id,
    kind: "paragraph",
    attrs: { level: null, language: null, start: null },
    content: text === "" ? [] : [{ type: "text", text, marks }],
    children: [],
  };
}

function heading(id: string, level: number, text: string): BlockNode {
  return {
    id,
    kind: "heading",
    attrs: { level, language: null, start: null },
    content: [{ type: "text", text, marks: [] }],
    children: [],
  };
}

function bullet(id: string, items: BlockNode[]): BlockNode {
  return {
    id,
    kind: "bulletList",
    attrs: { level: null, language: null, start: null },
    content: [],
    children: items,
  };
}

function listItem(
  id: string,
  content: BlockNode["content"],
  children: BlockNode[] = [],
): BlockNode {
  return {
    id,
    kind: "listItem",
    attrs: { level: null, language: null, start: null },
    content,
    children,
  };
}

describe("tiptapAdapter", () => {
  it("round-trips paragraphs, inline marks, and a hard break without losing ids", () => {
    const document: BlockDocument = {
      schemaVersion: 1,
      blocks: [
        paragraph("550e8400-e29b-41d4-a716-446655440000", "first"),
        {
          id: "6ba7b810-9dad-41d4-80b4-00c04fd430c8",
          kind: "paragraph",
          attrs: { level: null, language: null, start: null },
          content: [
            { type: "text", text: "bold", marks: [{ type: "bold" }] },
            { type: "hardBreak" },
            { type: "text", text: "italic", marks: [{ type: "italic" }] },
          ],
          children: [],
        },
      ],
    };

    const tiptap = toTiptapDocument(document);
    expect(tiptap.type).toBe("doc");
    const round = fromTiptapDocument(tiptap);
    expect(round).toEqual(document);
  });

  it("preserves nested lists and heading level across the round trip", () => {
    const document: BlockDocument = {
      schemaVersion: 1,
      blocks: [
        heading("550e8400-e29b-41d4-a716-446655440000", 2, "节标题"),
        bullet("6ba7b810-9dad-41d4-80b4-00c04fd430c8", [
          listItem("6ba7b811-9dad-41d4-80b4-00c04fd430c8", [
            { type: "text", text: "外层", marks: [] },
          ]),
          listItem(
            "6ba7b812-9dad-41d4-80b4-00c04fd430c8",
            [{ type: "text", text: "嵌套父", marks: [] }],
            [
              bullet("6ba7b813-9dad-41d4-80b4-00c04fd430c8", [
                listItem("6ba7b814-9dad-41d4-80b4-00c04fd430c8", [
                  { type: "text", text: "嵌套子", marks: [] },
                ]),
              ]),
            ],
          ),
        ]),
      ],
    };

    const round = fromTiptapDocument(toTiptapDocument(document));
    expect(round).toEqual(document);
  });

  it("preserves code block language and link hrefs", () => {
    const document: BlockDocument = {
      schemaVersion: 1,
      blocks: [
        {
          id: "550e8400-e29b-41d4-a716-446655440000",
          kind: "codeBlock",
          attrs: { level: null, language: "python", start: null },
          content: [{ type: "text", text: "print('hello')", marks: [] }],
          children: [],
        },
        {
          id: "6ba7b810-9dad-41d4-80b4-00c04fd430c8",
          kind: "paragraph",
          attrs: { level: null, language: null, start: null },
          content: [
            {
              type: "text",
              text: "Link",
              marks: [{ type: "link", href: "https://example.org" }],
            },
          ],
          children: [],
        },
      ],
    };

    const round = fromTiptapDocument(toTiptapDocument(document));
    expect(round).toEqual(document);
  });

  it("preserves blockquote and horizontal rule nodes", () => {
    const document: BlockDocument = {
      schemaVersion: 1,
      blocks: [
        paragraph("550e8400-e29b-41d4-a716-446655440000", "前导"),
        {
          id: "6ba7b810-9dad-41d4-80b4-00c04fd430c8",
          kind: "blockquote",
          attrs: { level: null, language: null, start: null },
          content: [],
          children: [paragraph("6ba7b811-9dad-41d4-80b4-00c04fd430c8", "引文")],
        },
        {
          id: "6ba7b812-9dad-41d4-80b4-00c04fd430c8",
          kind: "horizontalRule",
          attrs: { level: null, language: null, start: null },
          content: [],
          children: [],
        },
      ],
    };

    const round = fromTiptapDocument(toTiptapDocument(document));
    expect(round).toEqual(document);
  });

  it("regenerates ids that are missing, non-UUID-v4, or duplicated on load", () => {
    const document: BlockDocument = {
      schemaVersion: 1,
      blocks: [
        // valid and unique → preserved
        paragraph("550e8400-e29b-41d4-a716-446655440000", "keep"),
        // missing id → assigned a fresh UUID v4
        {
          id: "",
          kind: "paragraph",
          attrs: { level: null, language: null, start: null },
          content: [{ type: "text", text: "x", marks: [] }],
          children: [],
        },
        // invalid UUID → regenerated
        {
          id: "not-a-uuid",
          kind: "paragraph",
          attrs: { level: null, language: null, start: null },
          content: [{ type: "text", text: "y", marks: [] }],
          children: [],
        },
        // duplicate of an earlier valid id → regenerated
        paragraph("550e8400-e29b-41d4-a716-446655440000", "dup"),
      ],
    };

    const round = fromTiptapDocument(toTiptapDocument(document));
    const ids = round.blocks.map((block) => block.id);
    expect(ids[0]).toBe("550e8400-e29b-41d4-a716-446655440000");
    expect(ids[1]).not.toBe("");
    expect(ids[2]).not.toBe("not-a-uuid");
    expect(ids[3]).not.toBe("550e8400-e29b-41d4-a716-446655440000");
    // All regenerated ids must satisfy the UUID v4 shape.
    expect(ids[1]).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
    );
    expect(ids[2]).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
    );
    expect(ids[3]).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
    );
  });

  it("preserves dangerous-looking literals while dropping unsupported marks", () => {
    const tiptap: TiptapNode = {
      type: "doc",
      content: [
        {
          type: "paragraph",
          content: [
            { type: "text", text: "before" },
            { type: "text", text: "ignored", marks: [{ type: "underline" }] },
            { type: "text", text: "after" },
          ],
        },
        {
          type: "paragraph",
          content: [{ type: "text", text: "<script>alert(1)</script>" }],
        },
        // unknown node types are rendered as plain text when they produce text, else skipped.
        { type: "someExtension", content: [] },
      ],
    };

    const document = fromTiptapDocument(tiptap);
    // Adapter must not carry unknown marks; underline is dropped but surrounding text is kept.
    const para0 = document.blocks[0];
    const marks = para0.content.flatMap((node) =>
      "marks" in node ? node.marks : [],
    );
    expect(marks).toEqual([]);
    expect(
      para0.content.some((node) => "text" in node && node.text === "ignored"),
    ).toBe(true);
    // Literal text is inert at this boundary and must remain editable/searchable.
    const para1 = document.blocks[1];
    expect(
      para1.content.some(
        (node) => "text" in node && node.text.includes("script"),
      ),
    ).toBe(true);
    // Unknown top-level node becomes nothing harmful (either a vanilla paragraph or skipped).
    for (const node of document.blocks) {
      expect([
        "paragraph",
        "heading",
        "bulletList",
        "orderedList",
        "listItem",
        "blockquote",
        "codeBlock",
        "horizontalRule",
      ]).toContain(node.kind);
    }
  });

  it("preserves dangerous-looking literals in both adapter directions", () => {
    const document: BlockDocument = {
      schemaVersion: 1,
      blocks: [
        paragraph(
          "550e8400-e29b-41d4-a716-446655440000",
          '<script>alert(1)</script> javascript:alert(1) onerror="x"',
        ),
      ],
    };

    expect(fromTiptapDocument(toTiptapDocument(document))).toEqual(document);
  });

  it("yields a legal empty-paragraph document when given an empty tiptap doc", () => {
    const document = fromTiptapDocument({ type: "doc", content: [] });
    const fresh = emptyParagraphDocument();
    expect(document.schemaVersion).toBe(fresh.schemaVersion);
    expect(document.blocks).toHaveLength(1);
    expect(document.blocks[0].kind).toBe("paragraph");
    const id = document.blocks[0].id;
    expect(id).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
    );
  });
});
