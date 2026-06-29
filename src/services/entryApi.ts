import { invokeCommand } from "./invoke";
import type {
  EntryDetail,
  EntryListFilter,
  EntryPage,
  EntryPatch,
  PageRequest,
} from "../types/generated";

export function entriesCreate(content: string): Promise<EntryDetail> {
  return invokeCommand("entries_create", { content });
}

export function entriesList(
  filter: EntryListFilter,
  page: PageRequest,
): Promise<EntryPage> {
  return invokeCommand("entries_list", { filter, page });
}

export function entriesGet(id: string): Promise<EntryDetail> {
  return invokeCommand("entries_get", { id });
}

export function entriesUpdate(
  id: string,
  patch: EntryPatch,
  expectedRevision: number,
): Promise<EntryDetail> {
  return invokeCommand("entries_update", {
    id,
    patch,
    expectedRevision,
  });
}

export function entriesMoveToTrash(id: string): Promise<EntryDetail> {
  return invokeCommand("entries_move_to_trash", { id });
}

export function entriesRestoreFromTrash(id: string): Promise<EntryDetail> {
  return invokeCommand("entries_restore_from_trash", { id });
}

export function entriesDeleteForever(id: string): Promise<void> {
  return invokeCommand("entries_delete_forever", { id });
}
