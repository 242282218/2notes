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
  let version = 0;
  let inFlight = false;
  let pending = false;
  let activeFlush: Promise<void> | null = null;

  function markDirty() {
    version += 1;
    state.value = "dirty";
    error.value = null;
    schedule();
  }

  function schedule() {
    if (timer) {
      window.clearTimeout(timer);
    }
    timer = window.setTimeout(() => {
      void flush().catch(() => {});
    }, options.delay);
  }

  async function flush(): Promise<void> {
    if (timer) {
      window.clearTimeout(timer);
      timer = undefined;
    }
    if (inFlight) {
      pending = true;
      await activeFlush;
      return;
    }
    if (state.value !== "dirty" && state.value !== "failed") {
      return;
    }
    activeFlush = runFlush();
    try {
      await activeFlush;
    } finally {
      activeFlush = null;
    }
  }

  async function runFlush(): Promise<void> {
    const requestVersion = version;
    inFlight = true;
    state.value = "saving";
    try {
      const value = await options.save();
      if (requestVersion === version) {
        state.value = "saved";
        options.onSaved?.(value);
      } else {
        options.onStaleSaved?.(value);
        pending = true;
      }
    } catch (saveError) {
      if (requestVersion === version) {
        state.value = "failed";
        error.value =
          saveError instanceof Error ? saveError.message : "保存失败";
        options.onFailed?.(saveError);
        throw saveError;
      }
      pending = true;
    } finally {
      inFlight = false;
      if (pending) {
        pending = false;
        await flush();
      }
    }
  }

  function retry() {
    if (state.value === "failed" || state.value === "dirty") {
      void flush().catch(() => {});
    }
  }

  function reset() {
    if (timer) {
      window.clearTimeout(timer);
    }
    version += 1;
    pending = false;
    state.value = "idle";
    error.value = null;
  }

  return {
    state,
    error,
    isDirty,
    markDirty,
    flush,
    retry,
    reset,
  };
}
