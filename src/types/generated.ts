export type EntryType = "unclear" | "idea" | "task" | "material" | "question";

export type EntryStatus = "pending" | "done" | "archived";

export type TitleSource = "auto" | "user";

export type Tag = {
  id: string;
  name: string;
  normalizedName: string;
  createdAt: string;
  entryCount: number;
};

export type EntryListFilter = {
  query: string | null;
  entryType: EntryType | null;
  status: EntryStatus | null;
  tag: string | null;
  includeDeleted: boolean;
  trashOnly: boolean;
};

export type PageRequest = { limit: number | null; offset: number | null };

export type EntryListItem = {
  id: string;
  title: string | null;
  summary: string;
  entryType: EntryType;
  status: EntryStatus;
  tags: Array<Tag>;
  revision: number;
  createdAt: string;
  updatedAt: string;
  deletedAt: string | null;
};

export type EntryPage = {
  items: Array<EntryListItem>;
  limit: number;
  offset: number;
  hasMore: boolean;
};

export type EntryDetail = {
  id: string;
  title: string | null;
  titleSource: TitleSource;
  originalContent: string;
  currentContent: string;
  entryType: EntryType;
  status: EntryStatus;
  tags: Array<Tag>;
  revision: number;
  createdAt: string;
  updatedAt: string;
  deletedAt: string | null;
};

export type EntryPatch = {
  title: string | null;
  currentContent: string | null;
  entryType: EntryType | null;
  status: EntryStatus | null;
  tags: Array<string> | null;
};

export type Draft = { content: string; revision: number; updatedAt: string };

export type AppSettings = {
  dataDir: string;
  logDir: string;
  backupDir: string;
  shortcut: string;
  shortcutRegistered: boolean;
  shortcutError: string | null;
  autostartEnabled: boolean;
};

export type SettingsPatch = { autostartEnabled: boolean | null };

export type ExportResult = { exportedCount: number; targetDir: string };

export type BackupInfo = {
  path: string;
  fileName: string;
  kind: string;
  createdAt: string;
  sizeBytes: number;
};

export type AppErrorResponse = {
  code: string;
  message: string;
  recoverable: boolean;
};
