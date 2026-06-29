import { describe, expect, it, vi } from "vitest";

import { useAutosave } from "./useAutosave";

describe("useAutosave", () => {
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
    vi.useRealTimers();
  });
});
