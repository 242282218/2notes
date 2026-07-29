import { invokeCommand } from "./invoke";
import type {
  MarkdownImportPreview,
  MarkdownImportReport,
} from "../types/generated";

export function markdownImportPreview(): Promise<MarkdownImportPreview | null> {
  return invokeCommand("markdown_import_preview");
}

export function markdownImportCommit(
  sessionId: string,
): Promise<MarkdownImportReport> {
  return invokeCommand("markdown_import_commit", { sessionId });
}
