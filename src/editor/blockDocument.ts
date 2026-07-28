import type { BlockDocument, BlockNode } from "../types/generated";

function randomUuidV4(): string {
  // Prefer the platform Web Crypto UUID; fall back to a polyfill for SSR/older engines.
  const cryptoRef = globalThis.crypto as
    | {
        randomUUID?: () => string;
        getRandomValues?: <T extends ArrayBufferView>(arr: T) => T;
      }
    | undefined;
  if (cryptoRef?.randomUUID) {
    return cryptoRef.randomUUID();
  }
  if (cryptoRef?.getRandomValues) {
    const bytes = new Uint8Array(16);
    cryptoRef.getRandomValues(bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0"));
    return `${hex.slice(0, 4).join("")}-${hex.slice(4, 6).join("")}-${hex.slice(6, 8).join("")}-${hex
      .slice(8, 10)
      .join("")}-${hex.slice(10, 16).join("")}`;
  }
  // Last-resort polyfill using Math.random: still satisfies the UUID v4 shape we validate.
  const random = (offset: number) =>
    Math.floor(Math.random() * 16 ** offset)
      .toString(16)
      .padStart(offset, "0");
  return `${random(8)}-${random(4)}-4${random(3)}-${(8 + Math.floor(Math.random() * 4)).toString(16)}${random(
    3,
  )}-${random(12)}`;
}

const UUID_V4_PATTERN =
  /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

/** True for canonical UUID v4 strings; used to keep block ids stable across round trips. */
export function isUuidV4(value: string): boolean {
  return UUID_V4_PATTERN.test(value);
}

/** Allocate a fresh UUID v4 for blocks needing a new identity. */
export function newBlockId(): string {
  return randomUuidV4();
}

const EMPTY_ATTRS = { level: null, language: null, start: null };

/** Default shape for a fenced empty-paragraph document the backend accepts. */
export function emptyParagraphDocument(): BlockDocument {
  return {
    schemaVersion: 1,
    blocks: [emptyParagraphNode()],
  };
}

export function emptyParagraphNode(): BlockNode {
  return {
    id: newBlockId(),
    kind: "paragraph",
    attrs: { ...EMPTY_ATTRS },
    content: [],
    children: [],
  };
}

/** Shallow clone of the empty-paragraph attrs so callers cannot mutate shared state. */
export function defaultBlockAttrs(): BlockNode["attrs"] {
  return { ...EMPTY_ATTRS };
}
