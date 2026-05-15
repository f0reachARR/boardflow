import { z } from 'zod';

export const FileChangesSchema = z.object({
  added: z.number(),
  removed: z.number(),
  changed: z.number(),
  unchanged: z.number(),
});

export const BomChangesSchema = z.object({
  added: z.number(),
  removed: z.number(),
  changed: z.number(),
});

export const CheckEntrySchema = z.object({
  status_change: z.string(),
  error_delta: z.number(),
  warning_delta: z.number(),
});

export const ArtifactChangesSchema = z.object({
  added: z.number(),
  removed: z.number(),
  changed: z.number(),
});
