import {
  ArtifactChangesSchema,
  BomChangesSchema,
  CheckEntrySchema,
  FileChangesSchema,
} from './diff-summary-schema';

export interface FileChanges {
  added: number;
  removed: number;
  changed: number;
  unchanged: number;
}

export interface BomChanges {
  added: number;
  removed: number;
  changed: number;
}

export interface CheckEntry {
  status_change: string;
  error_delta: number;
  warning_delta: number;
}

export interface ArtifactChanges {
  added: number;
  removed: number;
  changed: number;
}

export interface ParsedDiffSummary {
  fileChanges: FileChanges | null;
  bomChanges: BomChanges | null;
  checks: [string, CheckEntry][] | null;
  artifactChanges: ArtifactChanges | null;
}

export function parseDiffSummary(raw: unknown): ParsedDiffSummary {
  const obj =
    typeof raw === 'object' && raw !== null && !Array.isArray(raw)
      ? (raw as Record<string, unknown>)
      : {};

  const fileResult = FileChangesSchema.safeParse(obj.file_changes);
  const bomResult = BomChangesSchema.safeParse(obj.bom_changes);
  const artifactResult = ArtifactChangesSchema.safeParse(obj.artifacts);

  let checks: [string, CheckEntry][] | null = null;
  if (typeof obj.checks === 'object' && obj.checks !== null && !Array.isArray(obj.checks)) {
    const entries = Object.entries(obj.checks as Record<string, unknown>).filter(
      (entry): entry is [string, CheckEntry] => CheckEntrySchema.safeParse(entry[1]).success,
    );
    checks = entries;
  }

  return {
    fileChanges: fileResult.success ? fileResult.data : null,
    bomChanges: bomResult.success ? bomResult.data : null,
    checks,
    artifactChanges: artifactResult.success ? artifactResult.data : null,
  };
}
