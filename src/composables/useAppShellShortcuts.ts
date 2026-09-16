import { onMounted, onUnmounted } from "vue";

export interface AppShellShortcutHandlers {
  focusSearch: () => void;
  closeTopmostOverlay: () => void;
  canDelete: () => boolean;
  requestDelete: () => void;
  clarify?: (key: string) => boolean;
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

/** True while a modal dialog owns the screen. Its trap only holds if focus stays
 * inside, so shell shortcuts must not move focus or open a second dialog. */
function isModalOpen(): boolean {
  return document.querySelector('[role="dialog"][aria-modal="true"]') !== null;
}

export function useAppShellShortcuts(handlers: AppShellShortcutHandlers) {
  function onKeydown(event: KeyboardEvent) {
    // ConfirmDialog consumes Escape itself; never let the window listener act on an
    // event a focused child control has already handled.
    if (event.defaultPrevented || isModalOpen()) {
      return;
    }

    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f") {
      event.preventDefault();
      handlers.focusSearch();
      return;
    }

    if (event.key === "Escape") {
      handlers.closeTopmostOverlay();
      return;
    }

    const key = event.key.toLowerCase();
    if (
      handlers.clarify &&
      ["j", "k", "t", "l", "m", "a", "d", "e"].includes(key)
    ) {
      if (
        isEditableTarget(event.target) ||
        isEditableTarget(document.activeElement)
      ) {
        return;
      }
      if (handlers.clarify(key)) {
        event.preventDefault();
      }
      return;
    }

    if (event.key !== "Delete") {
      return;
    }

    if (
      isEditableTarget(event.target) ||
      isEditableTarget(document.activeElement)
    ) {
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
