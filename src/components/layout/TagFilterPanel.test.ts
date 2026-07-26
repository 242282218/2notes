import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import TagFilterPanel from "./TagFilterPanel.vue";

const tags = [
  {
    id: "t1",
    name: "alpha",
    normalizedName: "alpha",
    createdAt: "2026-01-01",
    entryCount: 2,
  },
  {
    id: "t2",
    name: "beta",
    normalizedName: "beta",
    createdAt: "2026-01-02",
    entryCount: 1,
  },
];

describe("TagFilterPanel", () => {
  it("renders tags and highlights the current selection", () => {
    const wrapper = mount(TagFilterPanel, {
      props: {
        tags,
        currentTag: "alpha",
        tagsError: null,
      },
    });

    expect(wrapper.find("#tag-panel").exists()).toBe(true);
    expect(wrapper.text()).toContain("#alpha");
    expect(wrapper.text()).toContain("#beta");
    const selected = wrapper
      .findAll("button")
      .find((button) => button.text().includes("#alpha"));
    expect(selected?.classes().join(" ")).toContain("bg-selected");
  });

  it("emits select when a tag is chosen", async () => {
    const wrapper = mount(TagFilterPanel, {
      props: {
        tags,
        currentTag: "",
        tagsError: null,
      },
    });

    const beta = wrapper
      .findAll("button")
      .find((button) => button.text().includes("#beta"));
    await beta!.trigger("click");
    expect(wrapper.emitted("select")?.[0]).toEqual(["beta"]);
  });

  it("emits close on Escape and close button", async () => {
    const wrapper = mount(TagFilterPanel, {
      props: {
        tags,
        currentTag: "",
        tagsError: null,
      },
      attachTo: document.body,
    });

    await wrapper.get('button[aria-label="关闭标签面板"]').trigger("click");
    expect(wrapper.emitted("close")?.[0]).toEqual([true]);

    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
    );
    await flushPromises();
    expect(wrapper.emitted("close")?.length).toBeGreaterThanOrEqual(2);
    wrapper.unmount();
  });

  it("shows tags error and emits retry", async () => {
    const wrapper = mount(TagFilterPanel, {
      props: {
        tags: [],
        currentTag: "",
        tagsError: "标签加载失败",
      },
    });

    expect(wrapper.get('[role="alert"]').text()).toContain("标签加载失败");
    const retry = wrapper
      .findAll("button")
      .find((button) => button.text() === "重试");
    await retry!.trigger("click");
    expect(wrapper.emitted("retry")).toBeTruthy();
  });
});
