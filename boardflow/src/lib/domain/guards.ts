export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function isFileChanges(
  v: unknown,
): v is { added: number; removed: number; changed: number; unchanged: number } {
  return (
    isRecord(v) &&
    typeof v.added === 'number' &&
    typeof v.removed === 'number' &&
    typeof v.changed === 'number' &&
    typeof v.unchanged === 'number'
  );
}

export function isBomChanges(v: unknown): v is { added: number; removed: number; changed: number } {
  return (
    isRecord(v) &&
    typeof v.added === 'number' &&
    typeof v.removed === 'number' &&
    typeof v.changed === 'number'
  );
}

export function isCheckEntry(
  v: unknown,
): v is { status_change: string; error_delta: number; warning_delta: number } {
  return (
    isRecord(v) &&
    typeof v.status_change === 'string' &&
    typeof v.error_delta === 'number' &&
    typeof v.warning_delta === 'number'
  );
}

export function isArtifactChanges(
  v: unknown,
): v is { added: number; removed: number; changed: number } {
  return (
    isRecord(v) &&
    typeof v.added === 'number' &&
    typeof v.removed === 'number' &&
    typeof v.changed === 'number'
  );
}
