import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  knowledgeHealthIssuesGet,
  knowledgeHealthSummaryGet,
} from "../../services/healthApi";
import KnowledgeHealthView from "./KnowledgeHealthView.vue";

vi.mock("../../services/healthApi", () => ({
  knowledgeHealthSummaryGet: vi.fn(),
  knowledgeHealthIssuesGet: vi.fn(),
}));

const summary = {
  unresolvedLink: 2,
  orphanKnowledge: 1,
  untaggedKnowledge: 0,
  staleCapture: 0,
};

const issuePage = {
  items: [
    {
      entryId: "entry-1",
      title: "来源条目",
      updatedAt: "2026-07-01T00:00:00Z",
      rawTarget: "Missing",
      occurrenceCount: 2,
    },
  ],
  limit: 50,
  offset: 0,
  hasMore: false,
};

describe("KnowledgeHealthView", () => {
  beforeEach(() => {
    vi.mocked(knowledgeHealthSummaryGet).mockReset();
    vi.mocked(knowledgeHealthIssuesGet).mockReset();
    vi.mocked(knowledgeHealthSummaryGet).mockResolvedValue(summary);
  });

  it("loads only the summary until a category is selected", async () => {
    const wrapper = mount(KnowledgeHealthView, {
      props: { invalidatedToken: 0 },
    });
    await flushPromises();

    expect(wrapper.text()).toContain("未解析链接");
    expect(wrapper.text()).toContain("2");
    expect(knowledgeHealthIssuesGet).not.toHaveBeenCalled();

    vi.mocked(knowledgeHealthIssuesGet).mockResolvedValue(issuePage);
    await wrapper.get('[data-health-kind="unresolved_link"]').trigger("click");
    await flushPromises();

    expect(knowledgeHealthIssuesGet).toHaveBeenCalledWith("unresolved_link", {
      limit: 50,
      offset: 0,
    });
    expect(wrapper.text()).toContain("[[Missing]] · 2 处");
  });

  it("discards an older category response and opens the selected issue", async () => {
    const first = deferred<typeof issuePage>();
    const second = deferred<typeof issuePage>();
    vi.mocked(knowledgeHealthIssuesGet)
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const wrapper = mount(KnowledgeHealthView, {
      props: { invalidatedToken: 0 },
    });
    await flushPromises();

    await wrapper.get('[data-health-kind="unresolved_link"]').trigger("click");
    await wrapper.get('[data-health-kind="orphan_knowledge"]').trigger("click");
    second.resolve({
      ...issuePage,
      items: [{ ...issuePage.items[0], title: "孤立条目", rawTarget: "Other" }],
    });
    await flushPromises();
    first.resolve(issuePage);
    await flushPromises();

    expect(wrapper.text()).toContain("孤立条目");
    expect(wrapper.text()).not.toContain("[[Missing]]");
    await wrapper.get("li button").trigger("click");
    expect(wrapper.emitted("openEntry")).toEqual([["entry-1"]]);
  });

  it("keeps the stale marker when an invalidated summary request resolves", async () => {
    const pendingSummary = deferred<typeof summary>();
    vi.mocked(knowledgeHealthSummaryGet).mockReturnValue(
      pendingSummary.promise,
    );
    const wrapper = mount(KnowledgeHealthView, {
      props: { invalidatedToken: 0 },
    });

    await wrapper.setProps({ invalidatedToken: 1 });
    pendingSummary.resolve(summary);
    await flushPromises();

    expect(wrapper.text()).toContain("数据已变化，点击刷新查看最新报告。");
    expect(knowledgeHealthSummaryGet).toHaveBeenCalledOnce();
  });

  it("marks data stale without issuing a request after invalidation", async () => {
    const wrapper = mount(KnowledgeHealthView, {
      props: { invalidatedToken: 0 },
    });
    await flushPromises();
    vi.mocked(knowledgeHealthSummaryGet).mockClear();

    await wrapper.setProps({ invalidatedToken: 1 });

    expect(wrapper.text()).toContain("数据已变化");
    expect(knowledgeHealthSummaryGet).not.toHaveBeenCalled();
    expect(knowledgeHealthIssuesGet).not.toHaveBeenCalled();
  });
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((nextResolve) => {
    resolve = nextResolve;
  });
  return { promise, resolve };
}
