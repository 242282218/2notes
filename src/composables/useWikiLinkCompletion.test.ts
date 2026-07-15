import { describe, expect, it } from "vitest";

import {
  applyWikiLinkCompletion,
  findWikiLinkCompletion,
} from "./useWikiLinkCompletion";

describe("findWikiLinkCompletion", () => {
  it("finds the nearest unmatched wiki link before the caret", () => {
    const value = "[[closed]] and [[Cur";

    expect(findWikiLinkCompletion(value, value.length)).toEqual({
      start: 17,
      query: "Cur",
    });
  });

  it("ignores a link that already closes later on the current line", () => {
    expect(findWikiLinkCompletion("[[知识]]", 4)).toBeNull();
  });

  it("counts the 200 character limit by Unicode code point", () => {
    const emojiQuery = "😀".repeat(101);

    expect(
      findWikiLinkCompletion(`[[${emojiQuery}`, 2 + emojiQuery.length),
    ).toEqual({
      start: 2,
      query: emojiQuery,
    });
    expect(findWikiLinkCompletion(`[[${"汉".repeat(201)}`, 203)).toBeNull();
  });

  it.each([
    String.raw`\[[escaped`,
    "![[embedded",
    "[[target|label",
    "[[closed]]",
    "[[line\nbreak",
  ])("ignores invalid completion context %s", (value) => {
    expect(findWikiLinkCompletion(value, value.length)).toBeNull();
  });
});

describe("applyWikiLinkCompletion", () => {
  it("replaces only the query through the caret and returns the next caret", () => {
    const value = "before [[pa suffix";
    const completion = findWikiLinkCompletion(value, 11);

    expect(completion).not.toBeNull();
    expect(applyWikiLinkCompletion(value, completion!, "Page Title")).toEqual({
      value: "before [[Page Title]] suffix",
      caret: 21,
    });
  });
});
