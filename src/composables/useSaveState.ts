import { computed, type Ref } from "vue";
import type { SaveState } from "./useAutosave";

export function useSaveStateLabel(state: Ref<SaveState>) {
  return computed(() => {
    switch (state.value) {
      case "dirty":
        return "未保存";
      case "saving":
        return "保存中";
      case "saved":
        return "已保存";
      case "failed":
        return "保存失败";
      case "idle":
      default:
        return "空闲";
    }
  });
}
