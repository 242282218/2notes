import { afterEach, describe, expect, it, vi } from "vitest";

import { useAutosave } from "./useAutosave";

describe("useAutosave", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("keeps newer input from being overwritten by an old response", async () => {
    vi.useFakeTimers();
    const saved: number[] = [];
    const stale: number[] = [];
    let resolveFirst: ((value: number) => void) | undefined;
    const save = vi
      .fn()
      .mockImplementationOnce(
        () => new Promise<number>((resolve) => (resolveFirst = resolve)),
      )
      .mockResolvedValueOnce(2);
    const autosave = useAutosave<number>({
      delay: 10,
      save,
      onSaved: (value) => saved.push(value),
      onStaleSaved: (value) => stale.push(value),
    });

    autosave.markDirty();
    await vi.advanceTimersByTimeAsync(10);
    autosave.markDirty();
    await vi.advanceTimersByTimeAsync(10);
    expect(save).toHaveBeenCalledTimes(1);
    resolveFirst?.(1);
    await vi.advanceTimersByTimeAsync(0);

    expect(saved).toEqual([2]);
    expect(stale).toEqual([1]);
    expect(autosave.state.value).toBe("saved");
  });

  it("rejects explicit flush when the current save fails", async () => {
    const saveError = new Error("disk full");
    const onFailed = vi.fn();
    const autosave = useAutosave<string>({
      delay: 10,
      save: vi.fn().mockRejectedValue(saveError),
      onFailed,
    });

    autosave.markDirty();

    await expect(autosave.flush()).rejects.toThrow("disk full");
    expect(autosave.state.value).toBe("failed");
    expect(autosave.error.value).toBe("disk full");
    expect(onFailed).toHaveBeenCalledWith(saveError);
  });

  it("retries after a failed save without an unhandled rejection", async () => {
    const save = vi
      .fn()
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValueOnce("ok");
    const saved: string[] = [];
    const autosave = useAutosave<string>({
      delay: 10,
      save,
      onSaved: (value) => saved.push(value),
    });

    autosave.markDirty();
    await expect(autosave.flush()).rejects.toThrow("offline");

    autosave.retry();
    await vi.waitFor(() => {
      expect(autosave.state.value).toBe("saved");
    });

    expect(saved).toEqual(["ok"]);
  });
});
