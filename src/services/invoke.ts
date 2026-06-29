import { invoke as tauriInvoke } from "@tauri-apps/api/core";

import type { AppErrorResponse } from "../types/generated";

export class AppCommandError extends Error {
  readonly code: string;
  readonly recoverable: boolean;

  constructor(error: AppErrorResponse) {
    super(error.message);
    this.name = "AppCommandError";
    this.code = error.code;
    this.recoverable = error.recoverable;
  }
}

export async function invokeCommand<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  try {
    return await tauriInvoke<T>(command, args);
  } catch (error) {
    if (isAppErrorResponse(error)) {
      throw new AppCommandError(error);
    }
    throw error;
  }
}

function isAppErrorResponse(value: unknown): value is AppErrorResponse {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "message" in value &&
    "recoverable" in value
  );
}
