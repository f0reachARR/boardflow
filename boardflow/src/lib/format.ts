export function formatDateTime(date: string | Date): string {
  return new Date(date).toLocaleString();
}

export function formatDate(date: string | Date): string {
  return new Date(date).toLocaleDateString();
}

export function shortSha(sha: string): string {
  return sha.slice(0, 7);
}

export function shortId(id: string): string {
  return id.slice(0, 8);
}

export function formatBytes(bytes: number): string {
  return `${(bytes / 1024).toFixed(1)} KB`;
}
