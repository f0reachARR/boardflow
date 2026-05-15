/**
 * Extract an error message from an unknown API error response.
 * Returns `null` when the shape is not recognised, letting callers
 * fall back to their own default message via `??`.
 */
export function parseApiErrorMessage(err: unknown): string | null {
  if (typeof err !== 'object' || err === null) return null;
  const obj = err as Record<string, unknown>;
  if (typeof obj.error !== 'object' || obj.error === null) return null;
  const inner = obj.error as Record<string, unknown>;
  return typeof inner.message === 'string' ? inner.message : null;
}
