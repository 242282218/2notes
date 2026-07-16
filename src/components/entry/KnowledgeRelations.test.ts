import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { knowledgeRelationsGet } from "../../services/knowledgeApi";
import type { KnowledgeRelations } from "../../types/generated";
import KnowledgeRelationsPanel from "./KnowledgeRelations.vue";

vi.mock("../../services/knowledgeApi", () => ({
  knowledgeRelationsGet: vi.fn(),
}));

describe("KnowledgeRelations", () => {
  beforeEach(() => {
    vi.mocked(knowledgeRelationsGet).mockReset();
  });

  it("loads and renders all relation groups with occurrence counts", async () => {
    vi.mocked(knowledgeRelationsGet).mockResolvedValue(relations());
    const wrapper = mountPanel();
    await flushPromises();

    expect(wrapper.text()).toContain("出链");
    expect(wrapper.text()).toContain("目标条目");
    expect(wrapper.text()).toContain("2 次");
    expect(wrapper.text()).toContain("反向链接");
    expect(wrapper.text()).toContain("来源条目");
    expect(wrapper.text()).toContain("已在回收站");
    expect(wrapper.text()).toContain("未解析链接");
    expect(wrapper.text()).toContain("缺失目标");
    expect(wrapper.text()).toContain("3 次");

    const target = wrapper
      .findAll("button")
      .find((button) => button.text().includes("目标条目"));
    expect(target).toBeDefined();
    await target!.trigger("click");
    expect(wrapper.emitted("openRelated")).toEqual([["target"]]);
  });

  it("shows an explicit empty state", async () => {
    vi.mocked(knowledgeRelationsGet).mockResolvedValue(emptyRelations());
    const wrapper = mountPanel();
    await flushPromises();

    expect(wrapper.text()).toContain("暂无关联");
  });

  it("keeps the panel visible and reports API errors", async () => {
    vi.mocked(knowledgeRelationsGet).mockRejectedValue(new Error("加载失败"));
    const wrapper = mountPanel();
    await flushPromises();

    expect(wrapper.get('[role="alert"]').text()).toBe("加载失败");
    expect(wrapper.find(".knowledge-relations").exists()).toBe(true);
  });

  it("reports malformed API payloads instead of throwing during render", async () => {
    vi.mocked(knowledgeRelationsGet).mockResolvedValue(
      undefined as unknown as KnowledgeRelations,
    );
    const wrapper = mountPanel();
    await flushPromises();

    expect(wrapper.get('[role="alert"]').text()).toBe("关联数据格式无效");
  });

  it("reloads when the entry, revision, or refresh token changes", async () => {
    vi.mocked(knowledgeRelationsGet).mockResolvedValue(emptyRelations());
    const wrapper = mountPanel();
    await flushPromises();

    await wrapper.setProps({ revision: 2 });
    await flushPromises();
    await wrapper.setProps({ refreshToken: 1 });
    await flushPromises();
    await wrapper.setProps({ entryId: "entry-b" });
    await flushPromises();

    expect(knowledgeRelationsGet).toHaveBeenCalledTimes(4);
    expect(vi.mocked(knowledgeRelationsGet).mock.calls).toEqual([
      ["entry-a"],
      ["entry-a"],
      ["entry-a"],
      ["entry-b"],
    ]);
  });

  it("discards a stale successful response", async () => {
    const first = deferred<KnowledgeRelations>();
    const second = deferred<KnowledgeRelations>();
    vi.mocked(knowledgeRelationsGet)
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const wrapper = mountPanel();
    await wrapper.setProps({ entryId: "entry-b" });
    second.resolve({
      outgoing: [related("new", "新结果")],
      backlinks: [],
      unresolved: [],
    });
    await flushPromises();

    first.resolve({
      outgoing: [related("old", "旧结果")],
      backlinks: [],
      unresolved: [],
    });
    await flushPromises();

    expect(wrapper.text()).toContain("新结果");
    expect(wrapper.text()).not.toContain("旧结果");
  });

  it("ignores stale errors and keeps loading for the current request", async () => {
    const first = deferred<KnowledgeRelations>();
    const second = deferred<KnowledgeRelations>();
    vi.mocked(knowledgeRelationsGet)
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const wrapper = mountPanel();
    await wrapper.setProps({ refreshToken: 1 });

    first.reject(new Error("旧错误"));
    await flushPromises();

    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    expect(wrapper.text()).toContain("加载关联中");

    second.resolve(emptyRelations());
    await flushPromises();
    expect(wrapper.text()).toContain("暂无关联");
  });
});

function mountPanel() {
  return mount(KnowledgeRelationsPanel, {
    props: { entryId: "entry-a", revision: 1, refreshToken: 0 },
  });
}

function relations(): KnowledgeRelations {
  return {
    outgoing: [related("target", "目标条目", 2)],
    backlinks: [
      {
        ...related("source", "来源条目"),
        deletedAt: "2026-07-16T00:00:00Z",
      },
    ],
    unresolved: [{ rawTarget: "缺失目标", occurrenceCount: 3 }],
  };
}

function emptyRelations(): KnowledgeRelations {
  return { outgoing: [], backlinks: [], unresolved: [] };
}

function related(
  id: string,
  title: string,
  occurrenceCount = 1,
): KnowledgeRelations["outgoing"][number] {
  return {
    id,
    title,
    summary: `${title}摘要`,
    occurrenceCount,
    deletedAt: null,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}
