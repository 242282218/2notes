import { afterEach, describe, expect, it, vi } from "vitest";

import { useAutosave } from "./useAutosave";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

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

  it("waits until changes queued during an in-flight save are persisted", async () => {
    const first = deferred<number>();
    const second = deferred<number>();
    const save = vi
      .fn()
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const autosave = useAutosave({ delay: 10, save });

    autosave.markDirty();
    const flushing = autosave.flush();
    autosave.markDirty();
    first.resolve(1);
    await Promise.resolve();

    expect(save).toHaveBeenCalledTimes(2);
    let finished = false;
    void flushing.then(() => (finished = true));
    expect(finished).toBe(false);

    second.resolve(2);
    await flushing;
    expect(autosave.state.value).toBe("saved");
  });

  it("ignores stale save completion after reset", async () => {
    const first = deferred<number>();
    const onSaved = vi.fn();
    const onStaleSaved = vi.fn();
    const onFailed = vi.fn();
    const save = vi.fn().mockReturnValueOnce(first.promise);
    const autosave = useAutosave({
      delay: 10,
      save,
      onSaved,
      onStaleSaved,
      onFailed,
    });

    autosave.markDirty();
    const flushing = autosave.flush();
    autosave.reset();
    expect(autosave.state.value).toBe("idle");

    first.resolve(1);
    await Promise.resolve();
    await Promise.resolve();

    expect(onSaved).not.toHaveBeenCalled();
    expect(onStaleSaved).not.toHaveBeenCalled();
    expect(onFailed).not.toHaveBeenCalled();
    expect(autosave.state.value).toBe("idle");

    // Old flush should settle without changing state after reset.
    await expect(flushing).resolves.toBeUndefined();
    expect(autosave.state.value).toBe("idle");
  });

  it("ignores stale save failure after reset", async () => {
    const first = deferred<number>();
    const onSaved = vi.fn();
    const onStaleSaved = vi.fn();
    const onFailed = vi.fn();
    const save = vi.fn().mockReturnValueOnce(first.promise);
    const autosave = useAutosave({
      delay: 10,
      save,
      onSaved,
      onStaleSaved,
      onFailed,
    });

    autosave.markDirty();
    const flushing = autosave.flush();
    autosave.reset();

    first.reject(new Error("late failure"));
    await Promise.resolve();
    await Promise.resolve();

    expect(onSaved).not.toHaveBeenCalled();
    expect(onStaleSaved).not.toHaveBeenCalled();
    expect(onFailed).not.toHaveBeenCalled();
    expect(autosave.state.value).toBe("idle");
    expect(autosave.error.value).toBeNull();

    // Rejection from an outdated generation must not surface after reset.
    await expect(flushing).resolves.toBeUndefined();
  });

  it("ignores stale save completion after dispose", async () => {
    const first = deferred<number>();
    const onSaved = vi.fn();
    const onStaleSaved = vi.fn();
    const onFailed = vi.fn();
    const save = vi.fn().mockReturnValueOnce(first.promise);
    const autosave = useAutosave({
      delay: 10,
      save,
      onSaved,
      onStaleSaved,
      onFailed,
    });

    autosave.markDirty();
    const flushing = autosave.flush();
    autosave.dispose();
    expect(autosave.state.value).toBe("idle");

    first.resolve(1);
    await Promise.resolve();
    await Promise.resolve();

    expect(onSaved).not.toHaveBeenCalled();
    expect(onStaleSaved).not.toHaveBeenCalled();
    expect(onFailed).not.toHaveBeenCalled();
    expect(autosave.state.value).toBe("idle");

    await expect(flushing).resolves.toBeUndefined();
    expect(autosave.state.value).toBe("idle");
  });
});
