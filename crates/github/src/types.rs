#[derive(Debug, Clone)]
pub struct CreatedIssue {
    pub number: u64,
    pub node_id: String,
    pub html_url: String,
}

#[derive(Debug, Clone)]
pub struct IssueInfo {
    pub number: u64,
    pub node_id: String,
    pub state: IssueState,
    pub html_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueState {
    Open,
    Closed,
}

#[derive(Debug, Clone)]
pub struct CreatedComment {
    pub id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallationRepoInfo {
    pub id: i64,
    pub owner: String,
    pub name: String,
}
