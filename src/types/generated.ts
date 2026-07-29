export type KnowledgeState = "capture" | "knowledge";

export type EntryType = "unclear" | "idea" | "task" | "material" | "question";

export type EntryStatus = "pending" | "done" | "archived";

export type TitleSource = "auto" | "user";

export type KnowledgeSuggestion = {
  id: string;
  title: string;
  matchedAlias: string | null;
};
export type RelatedEntry = {
  id: string;
  title: string | null;
  summary: string;
  occurrenceCount: number;
  deletedAt: string | null;
};
export type UnresolvedWikiLink = { rawTarget: string; occurrenceCount: number };
export type KnowledgeRelations = {
  outgoing: Array<RelatedEntry>;
  backlinks: Array<RelatedEntry>;
  unresolved: Array<UnresolvedWikiLink>;
};
export type SearchSnippetPart = { text: string; highlighted: boolean };
export type SearchSnippet = { parts: Array<SearchSnippetPart> };
export type KnowledgeIndexReport = {
  indexedSources: number;
  linkOccurrences: number;
  unresolvedOccurrences: number;
  searchIndexAvailable: boolean;
  projectedBlocks: number;
  repairedDocuments: number;
};
export type EntryTreeNode = {
  id: string;
  title: string;
  children: Array<EntryTreeNode>;
};
export type EntryBreadcrumb = { id: string; title: string };

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
  knowledgeState: KnowledgeState | null;
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
  knowledgeState: KnowledgeState;
  searchSnippet: SearchSnippet | null;
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
  document: BlockDocument;
  entryType: EntryType;
  status: EntryStatus;
  knowledgeState: KnowledgeState;
  knowledgePromotedAt: string | null;
  knowledgeAliases: Array<string>;
  tags: Array<Tag>;
  revision: number;
  createdAt: string;
  updatedAt: string;
  deletedAt: string | null;
};

export type EntryPatch = {
  title: string | null;
  document: BlockDocument | null;
  entryType: EntryType | null;
  status: EntryStatus | null;
  tags: Array<string> | null;
};

export type CreateEntrySpec = {
  title: string | null;
  titleSource: TitleSource;
  originalContent: string;
  document: BlockDocument;
  entryType: EntryType;
  status: EntryStatus;
  tags: Array<string>;
};

export type DocumentRepairReport = {
  rebuilt: number;
  synced: number;
};

export type Draft = { content: string; revision: number; updatedAt: string };

export type ThemeMode = "system" | "light" | "dark";

export type AppSettings = {
  dataDir: string;
  logDir: string;
  backupDir: string;
  shortcut: string;
  shortcutRegistered: boolean;
  shortcutError: string | null;
  autostartEnabled: boolean;
  themeMode: ThemeMode;
};

export type SettingsPatch = {
  autostartEnabled: boolean | null;
  themeMode: ThemeMode | null;
};

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

export type BlockKind =
  | "paragraph"
  | "heading"
  | "bulletList"
  | "orderedList"
  | "listItem"
  | "blockquote"
  | "codeBlock"
  | "horizontalRule";

export type BlockAttrs = {
  level: number | null;
  language: string | null;
  start: number | null;
};

export type InlineMark =
  | { type: "bold" }
  | { type: "italic" }
  | { type: "strike" }
  | { type: "code" }
  | { type: "link"; href: string };

export type InlineNode =
  | { type: "text"; text: string; marks: Array<InlineMark> }
  | { type: "hardBreak" };

export type BlockNode = {
  id: string;
  kind: BlockKind;
  attrs: BlockAttrs;
  content: Array<InlineNode>;
  children: Array<BlockNode>;
};

export type BlockDocument = {
  schemaVersion: number;
  blocks: Array<BlockNode>;
};

export type OutlineItem = { id: string; level: number; text: string };

export type BlockProjection = {
  id: string;
  parentBlockId: string | null;
  ordinal: number;
  depth: number;
  kind: BlockKind;
  textContent: string;
  attrs: BlockAttrs;
};
