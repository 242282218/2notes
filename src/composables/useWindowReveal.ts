import { getCurrentWindow } from "@tauri-apps/api/window";

export async function revealCurrentWindow(): Promise<void> {
  const window = getCurrentWindow();
  await window.show();
  await window.unminimize();
  await window.setFocus();
}
