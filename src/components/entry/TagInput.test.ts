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

    expect(wrapper.emitted("update:modelValue")?.[0]).toEqual([["work"]]);
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
    await wrapper.get("input").setValue("wo");
    await flushPromises();

    await wrapper.get("[role='listbox'] button").trigger("click");

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

    const options = wrapper.findAll("[role='option']");
    expect(options[0].attributes("aria-selected")).toBe("true");

    await input.trigger("keydown", { key: "ArrowDown" });
    expect(options[1].attributes("aria-selected")).toBe("true");

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
  });
});
