import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import TagInput from "./TagInput.vue";

const tagsSuggest = vi.fn();

vi.mock("../../services/tagApi", () => ({
  tagsSuggest: (...args: unknown[]) => tagsSuggest(...args),
}));

describe("TagInput", () => {
  beforeEach(() => {
    tagsSuggest.mockReset();
    tagsSuggest.mockResolvedValue([]);
  });

  it("commits draft tag on blur", async () => {
    vi.useFakeTimers();
    const wrapper = mount(TagInput, {
      props: {
        modelValue: [],
      },
    });

    await wrapper.find("input").setValue(" work ");
    await wrapper.find("input").trigger("blur");
    await vi.advanceTimersByTimeAsync(150);

    expect(wrapper.emitted("update:modelValue")?.[0]).toEqual([["work"]]);
    vi.useRealTimers();
  });

  it("preserves parent casing when committing a distinct tag alongside an existing one", async () => {
    const wrapper = mount(TagInput, {
      props: {
        modelValue: ["Work"],
      },
    });
    const input = wrapper.find("input");
    await input.setValue("rust");
    await input.trigger("blur");

    // The parent's original "Work" casing must be preserved verbatim; only the
    // new "rust" tag is appended, no case-folding rewrite is emitted.
    expect(wrapper.emitted("update:modelValue")?.[0]).toEqual([["Work", "rust"]]);
  });

  it("removes only the case-insensitive match and preserves remaining originals", async () => {
    const wrapper = mount(TagInput, {
      props: {
        modelValue: ["Work", "Rust", "docs"],
      },
    });
    // The display view folds to lower-case; click the remove button for "rust".
    const removeButtons = wrapper.findAll("button[aria-label^='移除标签']");
    const rustButton = removeButtons.find((button) =>
      button.attributes("aria-label")?.toLocaleLowerCase().includes("rust"),
    );
    expect(rustButton).toBeDefined();
    await rustButton!.trigger("click");

    expect(wrapper.emitted("update:modelValue")?.[0]).toEqual([["Work", "docs"]]);
  });

  it("does not commit duplicate tags from blur", async () => {
    vi.useFakeTimers();
    const wrapper = mount(TagInput, {
      props: {
        modelValue: ["work"],
      },
    });

    await wrapper.find("input").setValue("Work");
    await wrapper.find("input").trigger("blur");
    await vi.advanceTimersByTimeAsync(150);

    // A case-insensitive duplicate must not emit a protective rewrite that
    // would coerce the parent's original casing back through normalization.
    expect(wrapper.emitted("update:modelValue")).toBeUndefined();
    vi.useRealTimers();
  });

  it("commits the draft before blur navigation unmounts the component", async () => {
    const wrapper = mount(TagInput, {
      props: {
        modelValue: [],
      },
    });
    const input = wrapper.find("input");
    await input.setValue("work");

    await input.trigger("blur");
    const emitted = wrapper.emitted("update:modelValue");
    wrapper.unmount();

    expect(emitted?.[0]).toEqual([["work"]]);
  });

  it("does not commit a tag while an IME composition is active", async () => {
    const wrapper = mount(TagInput, {
      props: {
        modelValue: [],
      },
    });
    const input = wrapper.find("input");
    await input.setValue("中文");

    await input.trigger("keydown", {
      key: "Enter",
      isComposing: true,
    });

    expect(wrapper.emitted("update:modelValue")).toBeUndefined();
  });

  it("selects a suggested tag through the button click action", async () => {
    tagsSuggest.mockResolvedValue([
      {
        id: "tag-1",
        name: "work",
        normalizedName: "work",
        createdAt: "2026-07-15T00:00:00Z",
        entryCount: 1,
      },
    ]);
    const wrapper = mount(TagInput, {
      props: {
        modelValue: [],
      },
    });
    const input = wrapper.get("input");
    await input.setValue("wo");
    await flushPromises();
    const option = wrapper.get("[role='listbox'] button");
    const mouseDown = new MouseEvent("mousedown", {
      bubbles: true,
      cancelable: true,
    });

    option.element.dispatchEvent(mouseDown);
    expect(mouseDown.defaultPrevented).toBe(true);
    expect(wrapper.emitted("update:modelValue")).toBeUndefined();

    await option.trigger("click");

    expect(wrapper.emitted("update:modelValue")?.[0]).toEqual([["work"]]);
  });

  it("navigates and selects suggestions with the keyboard", async () => {
    tagsSuggest.mockResolvedValue([
      {
        id: "tag-1",
        name: "work",
        normalizedName: "work",
        createdAt: "2026-07-15T00:00:00Z",
        entryCount: 1,
      },
      {
        id: "tag-2",
        name: "workshop",
        normalizedName: "workshop",
        createdAt: "2026-07-15T00:00:00Z",
        entryCount: 1,
      },
    ]);
    const wrapper = mount(TagInput, {
      props: {
        modelValue: [],
      },
    });
    const input = wrapper.get("input");
    await input.setValue("wo");
    await flushPromises();

    const listbox = wrapper.get("[role='listbox']");
    const options = wrapper.findAll("[role='option']");
    expect(input.attributes()).toMatchObject({
      role: "combobox",
      "aria-autocomplete": "list",
      "aria-expanded": "true",
      "aria-controls": listbox.attributes("id"),
      "aria-activedescendant": options[0].attributes("id"),
    });
    expect(options[0].attributes("aria-selected")).toBe("true");

    await input.trigger("keydown", { key: "ArrowDown" });
    expect(options[1].attributes("aria-selected")).toBe("true");
    expect(input.attributes("aria-activedescendant")).toBe(
      options[1].attributes("id"),
    );

    await input.trigger("keydown", { key: "Enter" });
    expect(wrapper.emitted("update:modelValue")?.[0]).toEqual([["workshop"]]);
  });

  it("closes suggestions with Escape", async () => {
    tagsSuggest.mockResolvedValue([
      {
        id: "tag-1",
        name: "work",
        normalizedName: "work",
        createdAt: "2026-07-15T00:00:00Z",
        entryCount: 1,
      },
    ]);
    const wrapper = mount(TagInput, {
      props: {
        modelValue: [],
      },
    });
    await wrapper.get("input").setValue("wo");
    await flushPromises();

    expect(wrapper.find("[role='listbox']").exists()).toBe(true);

    await wrapper.get("input").trigger("keydown", { key: "Escape" });
    await flushPromises();

    expect(wrapper.find("[role='listbox']").exists()).toBe(false);
    expect(wrapper.get("input").attributes("aria-expanded")).toBe("false");
    expect(wrapper.get("input").attributes("aria-controls")).toBeUndefined();
    expect(
      wrapper.get("input").attributes("aria-activedescendant"),
    ).toBeUndefined();
  });
});
