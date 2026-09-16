import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { onUnmounted } from "vue";

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

  if (isTauri()) {
    void listen<QuitRequestPayload>("app-quit-requested", async (event) => {
      try {
        if (!(await prepareQuit())) {
          await revealCurrentWindow();
          return;
        }
        await appQuitReady(event.payload.requestId);
      } catch {
        await revealCurrentWindow();
      }
    }).then((stop) => {
      if (disposed) {
        stop();
      } else {
        unlisten = stop;
      }
    });
  }

  onUnmounted(() => {
    disposed = true;
    unlisten?.();
  });
}
