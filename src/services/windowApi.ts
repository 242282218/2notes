import { invokeCommand } from "./invoke";

export function windowOpenQuickCapture(): Promise<void> {
  return invokeCommand("window_open_quick_capture");
}

export function windowHideQuickCapture(): Promise<void> {
  return invokeCommand("window_hide_quick_capture");
}

export function appQuitReady(requestId: string): Promise<void> {
  return invokeCommand("app_quit_ready", { requestId });
}
