import { invokeCommand } from "./invoke";
import type { Draft } from "../types/generated";

export function draftGet(): Promise<Draft> {
  return invokeCommand("draft_get");
}

export function draftUpdate(
  content: string,
  expectedRevision: number,
): Promise<Draft> {
  return invokeCommand("draft_update", { content, expectedRevision });
}

export function draftClear(): Promise<Draft> {
  return invokeCommand("draft_clear");
}

export function quickCaptureSubmit(content: string): Promise<Draft> {
  return invokeCommand("quick_capture_submit", { content });
}
