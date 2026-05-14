export const routes = {
  login: () => '/login',
  repositories: () => '/repositories',
  repository: (repositoryId: string | number) => `/repositories/${repositoryId}`,
  repositoryTokens: (repositoryId: string | number) =>
    `/repositories/${repositoryId}/settings/tokens`,
  board: (repositoryId: string | number, boardProjectId: string) =>
    `/repositories/${repositoryId}/boards/${boardProjectId}`,
  runs: (repositoryId: string | number, boardProjectId: string) =>
    `/repositories/${repositoryId}/boards/${boardProjectId}/runs`,
  run: (repositoryId: string | number, boardProjectId: string, boardRunId: string) =>
    `/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}`,
  runChecks: (
    repositoryId: string | number,
    boardProjectId: string,
    boardRunId: string,
    checkKind: string,
  ) =>
    `/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}/checks/${checkKind}`,
  runDiff: (repositoryId: string | number, boardProjectId: string, boardRunId: string) =>
    `/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}/diff`,
};
