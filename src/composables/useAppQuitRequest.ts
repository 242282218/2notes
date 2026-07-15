import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { onMounted, onUnmounted } from "vue";

import { appQuitReady } from "../services/windowApi";
import { revealCurrentWindow } from "./useWindowReveal";

type QuitRequestPayload = {
  requestId: string;
};

export function useAppQuitRequest(
  prepareQuit: () => boolean | Promise<boolean>,
) {
  let disposed = false;
  let unlisten: (() => void) | null = null;

  onMounted(async () => {
    if (!isTauri()) {
      return;
    }
    const stop = await listen<QuitRequestPayload>(
      "app-quit-requested",
      async (event) => {
        if (!(await prepareQuit())) {
          await revealCurrentWindow();
          return;
        }
        await appQuitReady(event.payload.requestId);
      },
    );

    if (disposed) {
      stop();
      return;
    }
    unlisten = stop;
  });

  onUnmounted(() => {
    disposed = true;
    unlisten?.();
  });
}
