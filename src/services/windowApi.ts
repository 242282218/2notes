import { invokeCommand } from "./invoke";

export function windowOpenMain(): Promise<void> {
  return invokeCommand("window_open_main");
}

export function windowOpenQuickCapture(): Promise<void> {
  return invokeCommand("window_open_quick_capture");
}

export function windowHideQuickCapture(): Promise<void> {
  return invokeCommand("window_hide_quick_capture");
}

export function windowHideMain(): Promise<void> {
  return invokeCommand("window_hide_main");
}

export function appQuit(): Promise<void> {
  return invokeCommand("app_quit");
}

export function appQuitReady(
  requestId: string,
  windowLabel: string,
): Promise<void> {
  return invokeCommand("app_quit_ready", { requestId, windowLabel });
}
