import type { SaveState } from "../../composables/useAutosave";

export type EntryDetailToolbarState = {
  saveState: SaveState;
  saveError: string | null;
  showPromote: boolean;
  canPromote: boolean;
  canDemote: boolean;
  deleted: boolean;
};
