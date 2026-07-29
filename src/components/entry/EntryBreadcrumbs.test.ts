import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";

import { knowledgeBreadcrumbsGet } from "../../services/knowledgeApi";
import EntryBreadcrumbs from "./EntryBreadcrumbs.vue";

vi.mock("../../services/knowledgeApi", () => ({
  knowledgeBreadcrumbsGet: vi.fn(),
}));

describe("EntryBreadcrumbs", () => {
  it("renders ancestors and opens the selected ancestor", async () => {
    vi.mocked(knowledgeBreadcrumbsGet).mockResolvedValue([
      { id: "root", title: "根" },
      { id: "current", title: "当前" },
    ]);
    const wrapper = mount(EntryBreadcrumbs, {
      props: { entryId: "current", enabled: true },
    });
    await flushPromises();

    expect(wrapper.text()).toContain("根");
    expect(wrapper.text()).not.toContain("当前");
    await wrapper.get("button").trigger("click");
    expect(wrapper.emitted("open")).toEqual([["root"]]);
  });

  it("drops a stale breadcrumb response after selection changes", async () => {
    let resolveFirst: (value: { id: string; title: string }[]) => void;
    const first = new Promise<{ id: string; title: string }[]>((resolve) => {
      resolveFirst = resolve;
    });
    vi.mocked(knowledgeBreadcrumbsGet)
      .mockReturnValueOnce(first)
      .mockResolvedValueOnce([
        { id: "second-root", title: "第二根" },
        { id: "second", title: "第二当前" },
      ]);
    const wrapper = mount(EntryBreadcrumbs, {
      props: { entryId: "first", enabled: true },
    });
    await wrapper.setProps({ entryId: "second" });
    await flushPromises();
    resolveFirst!([
      { id: "first-root", title: "第一根" },
      { id: "first", title: "第一当前" },
    ]);
    await flushPromises();

    expect(wrapper.text()).toContain("第二根");
    expect(wrapper.text()).not.toContain("第一根");
  });
});
