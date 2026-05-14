import type {
  ArtifactStatus,
  BoardProjectState,
  BoardRunDiffStatus,
  BoardRunStatus,
  RunCheckStatus,
} from '@/lib/api/schema-types';

export function boardRunStatusColor(status: BoardRunStatus | string | null): string {
  switch (status) {
    case 'completed':
      return 'green';
    case 'failed':
      return 'red';
    case 'timed_out':
      return 'orange';
    case 'created':
    case 'uploading':
    case 'importing':
    case 'processing':
      return 'blue';
    default:
      return 'gray';
  }
}

export function checkStatusColor(status: RunCheckStatus | string): string {
  switch (status) {
    case 'passed':
      return 'green';
    case 'failed':
      return 'red';
    default:
      return 'gray';
  }
}

export function artifactStatusColor(status: ArtifactStatus | string): string {
  switch (status) {
    case 'available':
      return 'green';
    case 'missing':
      return 'orange';
    case 'failed':
      return 'red';
    case 'skipped':
      return 'gray';
    default:
      return 'gray';
  }
}

export function diffStatusColor(status: BoardRunDiffStatus | string): string {
  switch (status) {
    case 'ready':
      return 'green';
    case 'no_baseline':
      return 'gray';
    case 'unavailable':
      return 'orange';
    case 'failed':
      return 'red';
    default:
      return 'gray';
  }
}

export function projectStateColor(state: BoardProjectState | string): string {
  switch (state) {
    case 'completed':
      return 'green';
    case 'failed':
      return 'red';
    case 'timed_out':
      return 'orange';
    case 'processing':
      return 'blue';
    case 'detected':
      return 'gray';
    default:
      return 'gray';
  }
}

export function checkBadgeColor(status: RunCheckStatus | string): string {
  switch (status) {
    case 'passed':
      return 'green.solid';
    case 'failed':
      return 'red.solid';
    default:
      return 'gray.solid';
  }
}
