import { AppCommandError } from "../services/invoke";

/** Extract a human-readable message from an unknown error value. */
export function toErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message) {
    return error.message;
  }
  return fallback;
}

/** True when the error is a structured backend error that is safe to retry. */
export function isRecoverableAppError(
  error: unknown,
): error is AppCommandError {
  return error instanceof AppCommandError && error.recoverable;
}

/** True only when the backend explicitly marked the failure as not retryable
 * (e.g. migration or config corruption); plain/unknown errors are assumed
 * retryable. */
export function isKnownNonRecoverable(error: unknown): boolean {
  return error instanceof AppCommandError && !error.recoverable;
}
