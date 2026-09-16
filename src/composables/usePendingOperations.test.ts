import { afterEach, describe, expect, it, vi } from "vitest";

import {
  trackPendingOperation,
  waitForPendingOperations,
} from "./usePendingOperations";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

afterEach(() => {
  vi.useRealTimers();
});

describe("usePendingOperations", () => {
  it("returns immediately when no operations are pending", async () => {
    await expect(waitForPendingOperations()).resolves.toBe(true);
  });

  it("waits for operations registered while the first batch is pending", async () => {
    const first = deferred<string>();
    const second = deferred<string>();
    trackPendingOperation(first.promise);
    const waiting = waitForPendingOperations();

    first.resolve("first");
    await Promise.resolve();
    trackPendingOperation(second.promise);

    let settled = false;
    void waiting.then(() => {
      settled = true;
    });
    await Promise.resolve();
    expect(settled).toBe(false);

    second.resolve("second");
    await expect(waiting).resolves.toBe(true);
  });

  it("reports a failed operation after waiting for the whole batch", async () => {
    const failed = trackPendingOperation(Promise.reject(new Error("failed")));
    await expect(waitForPendingOperations()).resolves.toBe(false);
    await expect(failed).rejects.toThrow("failed");
  });

  it("times out and reports failure when an operation never settles", async () => {
    vi.useFakeTimers();
    const hung = deferred<string>();
    trackPendingOperation(hung.promise);

    const waiting = waitForPendingOperations();
    const assertion = expect(waiting).resolves.toBe(false);
    await vi.advanceTimersByTimeAsync(10_001);
    await assertion;
  });
});
