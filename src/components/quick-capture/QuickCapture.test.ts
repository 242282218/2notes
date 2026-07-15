import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { Draft } from "../../types/generated";
import QuickCapture from "./QuickCapture.vue";

const draftGet = vi.fn();
const draftUpdate = vi.fn();
const quickCaptureSubmit = vi.fn();
const databaseRestoreReady = vi.fn();
const eventListeners = new Map<
  string,
  (event: { payload: unknown }) => unknown
>();

vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => true,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (
      event: string,
      callback: (event: { payload: unknown }) => unknown,
    ) => {
      eventListeners.set(event, callback);
      return vi.fn();
    },
  ),
}));

vi.mock("../../services/backupApi", () => ({
  databaseRestoreReady: (...args: unknown[]) => databaseRestoreReady(...args),
}));

vi.mock("../../services/draftApi", () => ({
  draftGet: (...args: unknown[]) => draftGet(...args),
  draftUpdate: (...args: unknown[]) => draftUpdate(...args),
  quickCaptureSubmit: (...args: unknown[]) => quickCaptureSubmit(...args),
}));

vi.mock("../../services/windowApi", () => ({
  appQuitReady: vi.fn(),
  windowHideQuickCapture: vi.fn(),
}));

vi.mock("../../composables/useWindowReveal", () => ({
  revealCurrentWindow: vi.fn(),
}));

describe("QuickCapture", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    draftGet.mockReset();
    draftUpdate.mockReset();
    quickCaptureSubmit.mockReset();
    databaseRestoreReady.mockReset();
    eventListeners.clear();
    draftGet.mockResolvedValue(draft("", 0));
    draftUpdate.mockResolvedValue(draft("", 1));
    quickCaptureSubmit.mockResolvedValue(draft("", 2));
  });

  it("uses the revision returned by a stale save for the next save", async () => {
    let resolveFirstSave: ((value: Draft) => void) | undefined;
    draftUpdate
      .mockImplementationOnce(
        () =>
          new Promise<Draft>((resolve) => {
            resolveFirstSave = resolve;
          }),
      )
      .mockResolvedValueOnce(draft("second", 2));

    const wrapper = mount(QuickCapture);
    await flushPromises();
    const textarea = wrapper.get("textarea");

    await textarea.setValue("first");
    await vi.advanceTimersByTimeAsync(250);
    expect(draftUpdate).toHaveBeenNthCalledWith(1, "first", 0);

    await textarea.setValue("second");
    resolveFirstSave?.(draft("first", 1));
    await flushPromises();

    expect(draftUpdate).toHaveBeenNthCalledWith(2, "second", 1);
  });

  it("does not submit while an IME composition is active", async () => {
    const wrapper = mount(QuickCapture);
    await flushPromises();
    const textarea = wrapper.get("textarea");
    await textarea.setValue("中文输入");

    await textarea.trigger("keydown", {
      key: "Enter",
      isComposing: true,
    });
    await flushPromises();

    expect(quickCaptureSubmit).not.toHaveBeenCalled();
  });

  it("flushes the local draft before acknowledging database restore", async () => {
    draftUpdate.mockResolvedValue(draft("unsaved draft", 1));
    const wrapper = mount(QuickCapture);
    await flushPromises();
    await wrapper.get("textarea").setValue("unsaved draft");

    const listener = eventListeners.get("database-restore-prepare");
    expect(listener).toBeDefined();
    await listener?.({ payload: { requestId: "restore-1" } });
    await flushPromises();

    expect(draftUpdate).toHaveBeenCalledWith("unsaved draft", 0);
    expect(databaseRestoreReady).toHaveBeenCalledWith("restore-1");
  });

  it("reloads the restored draft after a database restore event", async () => {
    draftGet
      .mockResolvedValueOnce(draft("before restore", 5))
      .mockResolvedValueOnce(draft("restored draft", 2));
    const wrapper = mount(QuickCapture);
    await flushPromises();
    expect(wrapper.get("textarea").element.value).toBe("before restore");

    const listener = eventListeners.get("database-restored");
    expect(listener).toBeDefined();
    await listener?.({ payload: null });
    await flushPromises();

    expect(wrapper.get("textarea").element.value).toBe("restored draft");
    await wrapper.get("textarea").setValue("restored draft updated");
    await vi.advanceTimersByTimeAsync(250);
    expect(draftUpdate).toHaveBeenLastCalledWith("restored draft updated", 2);
  });
});

function draft(content: string, revision: number): Draft {
  return {
    content,
    revision,
    updatedAt: "2026-07-15T00:00:00Z",
  };
}
