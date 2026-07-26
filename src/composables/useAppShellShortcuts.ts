import { onMounted, onUnmounted } from "vue";

export interface AppShellShortcutHandlers {
  focusSearch: () => void;
  closeTopmostOverlay: () => void;
  canDelete: () => boolean;
  requestDelete: () => void;
}

function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) {
    return false;
  }
  const editable = target.closest(
    "input, textarea, select, [contenteditable=''], [contenteditable='true']",
  );
  return editable instanceof HTMLElement;
}

export function useAppShellShortcuts(handlers: AppShellShortcutHandlers) {
  function onKeydown(event: KeyboardEvent) {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f") {
      event.preventDefault();
      handlers.focusSearch();
      return;
    }

    if (event.key === "Escape") {
      handlers.closeTopmostOverlay();
      return;
    }

    if (event.key !== "Delete") {
      return;
    }

    if (isEditableTarget(event.target) || isEditableTarget(document.activeElement)) {
      return;
    }

    if (!handlers.canDelete()) {
      return;
    }

    event.preventDefault();
    handlers.requestDelete();
  }

  onMounted(() => {
    window.addEventListener("keydown", onKeydown);
  });

  onUnmounted(() => {
    window.removeEventListener("keydown", onKeydown);
  });
}
