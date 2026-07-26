import { invokeCommand } from "./invoke";
import type { ExportResult } from "../types/generated";

export function exportMarkdown(): Promise<ExportResult | null> {
  return invokeCommand("export_markdown");
}
