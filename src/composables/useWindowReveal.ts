import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

export async function revealCurrentWindow(): Promise<void> {
  if (!isTauri()) {
    return;
  }
  const window = getCurrentWindow();
  await window.show();
  await window.unminimize();
  await window.setFocus();
}
