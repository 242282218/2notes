import { computed, ref } from "vue";

export type SaveState = "idle" | "dirty" | "saving" | "saved" | "failed";

interface AutosaveOptions<T> {
  delay: number;
  save: () => Promise<T>;
  onSaved?: (value: T) => void;
  onStaleSaved?: (value: T) => void;
  onFailed?: (error: unknown) => void;
}

export function useAutosave<T>(options: AutosaveOptions<T>) {
  const state = ref<SaveState>("idle");
  const error = ref<string | null>(null);

  const isDirty = computed(
    () => state.value === "dirty" || state.value === "failed",
  );

  let timer: number | undefined;
  /** Content version: whether a save result matches the latest input. */
  let version = 0;
  /** Lifecycle generation: whether a save still belongs to the current entry cycle. */
  let generation = 0;
  let pending = false;
  let disposed = false;
  let drainPromise: Promise<void> | null = null;

  function clearTimer() {
    if (timer) {
      window.clearTimeout(timer);
      timer = undefined;
    }
  }

  function isActive(requestGeneration: number) {
    return !disposed && requestGeneration === generation;
  }

  function needsSave() {
    return state.value === "dirty" || state.value === "failed" || pending;
  }

  function markDirty() {
    if (disposed) return;
    version += 1;
    state.value = "dirty";
    error.value = null;
    schedule();
  }

  function schedule() {
    if (disposed) return;
    clearTimer();
    timer = window.setTimeout(() => {
      void flush().catch(() => {});
    }, options.delay);
  }

  async function flush(): Promise<void> {
    if (disposed) return;
    clearTimer();

    if (drainPromise) {
      return drainPromise;
    }

    if (!needsSave()) {
      return;
    }

    const requestGeneration = generation;
    drainPromise = drain(requestGeneration).finally(() => {
      if (requestGeneration === generation) {
        drainPromise = null;
      }
    });

    return drainPromise;
  }

  async function drain(requestGeneration: number): Promise<void> {
    // Loop until this generation is clean. Continue after each save in the same
    // microtask so callers awaiting one Promise.resolve() observe chained saves.
    while (isActive(requestGeneration) && needsSave()) {
      pending = false;
      const requestVersion = version;
      state.value = "saving";

      try {
        const value = await options.save();
        if (!isActive(requestGeneration)) return;

        if (requestVersion === version) {
          state.value = "saved";
          options.onSaved?.(value);
        } else {
          options.onStaleSaved?.(value);
          pending = true;
        }
      } catch (saveError) {
        if (!isActive(requestGeneration)) return;

        if (requestVersion === version) {
          state.value = "failed";
          error.value =
            saveError instanceof Error ? saveError.message : "保存失败";
          options.onFailed?.(saveError);
          throw saveError;
        }
        pending = true;
      }
    }
  }

  function retry() {
    if (state.value === "failed" || state.value === "dirty") {
      void flush().catch(() => {});
    }
  }

  function reset() {
    generation += 1;
    clearTimer();
    pending = false;
    drainPromise = null;
    state.value = "idle";
    error.value = null;
  }

  function dispose() {
    disposed = true;
    reset();
  }

  return {
    state,
    error,
    isDirty,
    markDirty,
    flush,
    retry,
    reset,
    dispose,
  };
}
