import type { SaveState } from "./useAutosave";

const labels: Record<SaveState, string> = {
  dirty: "未保存",
  saving: "保存中",
  saved: "已保存",
  failed: "保存失败",
  idle: "空闲",
};

export function getSaveStateLabel(state: SaveState): string {
  return labels[state];
}
