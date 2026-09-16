const pendingOperations = new Set<Promise<unknown>>();

// A hung IPC (deadlocked backend, stuck file handle) must not block app quit or
// database restore forever; time out and report failure instead.
const PENDING_OPERATION_TIMEOUT_MS = 10_000;

export function trackPendingOperation<T>(operation: Promise<T>): Promise<T> {
  const tracked = operation.finally(() => {
    pendingOperations.delete(tracked);
  });
  pendingOperations.add(tracked);
  return tracked;
}

export async function waitForPendingOperations(): Promise<boolean> {
  let succeeded = true;
  while (pendingOperations.size > 0) {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const settled = await Promise.race([
      Promise.allSettled([...pendingOperations]),
      new Promise<"timeout">((resolve) => {
        timer = setTimeout(
          () => resolve("timeout"),
          PENDING_OPERATION_TIMEOUT_MS,
        );
      }),
    ]);
    if (timer) {
      clearTimeout(timer);
    }
    if (settled === "timeout") {
      return false;
    }
    if (settled.some((result) => result.status === "rejected")) {
      succeeded = false;
    }
  }
  return succeeded;
}
