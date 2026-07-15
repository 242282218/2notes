import { invokeCommand } from "./invoke";
import type { BackupInfo } from "../types/generated";

export function backupsCreate(kind: string): Promise<BackupInfo> {
  return invokeCommand("backups_create", { kind });
}

export function backupsList(): Promise<BackupInfo[]> {
  return invokeCommand("backups_list");
}

export function backupsRestore(path: string): Promise<BackupInfo> {
  return invokeCommand("backups_restore", { path });
}
export function databaseRestoreReady(requestId: string): Promise<void> {
  return invokeCommand("database_restore_ready", { requestId });
}
