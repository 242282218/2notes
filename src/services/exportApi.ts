import { invokeCommand } from "./invoke";
import type { ExportResult } from "../types/generated";

export function exportMarkdown(targetDir: string): Promise<ExportResult> {
  return invokeCommand("export_markdown", { targetDir });
}
