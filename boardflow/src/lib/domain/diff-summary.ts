import { isArtifactChanges, isBomChanges, isCheckEntry, isFileChanges, isRecord } from './guards';

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
  const obj = isRecord(raw) ? raw : {};
  return {
    fileChanges: isFileChanges(obj.file_changes) ? obj.file_changes : null,
    bomChanges: isBomChanges(obj.bom_changes) ? obj.bom_changes : null,
    checks: isRecord(obj.checks)
      ? Object.entries(obj.checks).filter((entry): entry is [string, CheckEntry] =>
          isCheckEntry(entry[1]),
        )
      : null,
    artifactChanges: isArtifactChanges(obj.artifacts) ? obj.artifacts : null,
  };
}
