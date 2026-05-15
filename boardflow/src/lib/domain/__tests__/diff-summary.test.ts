import { describe, expect, it } from 'vitest';
import { parseDiffSummary } from '../diff-summary';

describe('parseDiffSummary', () => {
  // --- checks parsing ---
  describe('checks', () => {
    it('returns parsed entries when all entries are valid', () => {
      const result = parseDiffSummary({
        checks: {
          erc: { status_change: 'pass→pass', error_delta: 0, warning_delta: -1 },
          drc: { status_change: 'fail→pass', error_delta: -2, warning_delta: 0 },
        },
      });
      expect(result.checks).toEqual([
        ['erc', { status_change: 'pass→pass', error_delta: 0, warning_delta: -1 }],
        ['drc', { status_change: 'fail→pass', error_delta: -2, warning_delta: 0 }],
      ]);
    });

    it('returns only valid entries when some entries are malformed', () => {
      const result = parseDiffSummary({
        checks: {
          erc: { status_change: 'pass→pass', error_delta: 0, warning_delta: -1 },
          drc: { bad: 'data' },
        },
      });
      expect(result.checks).toEqual([
        ['erc', { status_change: 'pass→pass', error_delta: 0, warning_delta: -1 }],
      ]);
    });

    it('returns null when all entries are malformed (silent drop fix)', () => {
      const result = parseDiffSummary({
        checks: {
          erc: { bad: 'data' },
          drc: 'not an object',
        },
      });
      expect(result.checks).toBeNull();
    });

    it('returns null when checks key is missing', () => {
      const result = parseDiffSummary({});
      expect(result.checks).toBeNull();
    });

    it('returns null when checks is not an object', () => {
      const result = parseDiffSummary({ checks: 'string' });
      expect(result.checks).toBeNull();
    });

    it('returns null when checks is an array', () => {
      const result = parseDiffSummary({ checks: [1, 2, 3] });
      expect(result.checks).toBeNull();
    });

    it('returns null when checks is null', () => {
      const result = parseDiffSummary({ checks: null });
      expect(result.checks).toBeNull();
    });

    it('returns empty array when checks is an empty object', () => {
      const result = parseDiffSummary({ checks: {} });
      expect(result.checks).toEqual([]);
    });

    it('uses result.data from safeParse (strips extra fields via schema)', () => {
      const result = parseDiffSummary({
        checks: {
          erc: { status_change: 'pass', error_delta: 0, warning_delta: 0, extra_field: 'ignored' },
        },
      });
      // zod strip mode removes extra fields
      expect(result.checks).toEqual([
        ['erc', { status_change: 'pass', error_delta: 0, warning_delta: 0 }],
      ]);
    });
  });

  // --- fileChanges parsing ---
  describe('fileChanges', () => {
    it('returns parsed data for valid input', () => {
      const result = parseDiffSummary({
        file_changes: { added: 1, removed: 2, changed: 3, unchanged: 4 },
      });
      expect(result.fileChanges).toEqual({ added: 1, removed: 2, changed: 3, unchanged: 4 });
    });

    it('returns null for malformed input', () => {
      const result = parseDiffSummary({ file_changes: { added: 'not a number' } });
      expect(result.fileChanges).toBeNull();
    });
  });

  // --- bomChanges parsing ---
  describe('bomChanges', () => {
    it('returns parsed data for valid input', () => {
      const result = parseDiffSummary({
        bom_changes: { added: 1, removed: 0, changed: 2 },
      });
      expect(result.bomChanges).toEqual({ added: 1, removed: 0, changed: 2 });
    });

    it('returns null for malformed input', () => {
      const result = parseDiffSummary({ bom_changes: 42 });
      expect(result.bomChanges).toBeNull();
    });
  });

  // --- artifactChanges parsing ---
  describe('artifactChanges', () => {
    it('returns parsed data for valid input', () => {
      const result = parseDiffSummary({
        artifacts: { added: 3, removed: 1, changed: 0 },
      });
      expect(result.artifactChanges).toEqual({ added: 3, removed: 1, changed: 0 });
    });

    it('returns null for malformed input', () => {
      const result = parseDiffSummary({ artifacts: null });
      expect(result.artifactChanges).toBeNull();
    });
  });

  // --- edge cases ---
  describe('edge cases', () => {
    it('handles non-object raw input gracefully', () => {
      const result = parseDiffSummary('not an object');
      expect(result.fileChanges).toBeNull();
      expect(result.bomChanges).toBeNull();
      expect(result.checks).toBeNull();
      expect(result.artifactChanges).toBeNull();
    });

    it('handles null raw input gracefully', () => {
      const result = parseDiffSummary(null);
      expect(result.fileChanges).toBeNull();
      expect(result.bomChanges).toBeNull();
      expect(result.checks).toBeNull();
      expect(result.artifactChanges).toBeNull();
    });

    it('handles array raw input gracefully', () => {
      const result = parseDiffSummary([1, 2, 3]);
      expect(result.checks).toBeNull();
    });
  });
});
