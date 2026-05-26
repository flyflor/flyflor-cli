#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionRowStatus {
    Pending,
    Running,
    NeedsUser,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionContextRow {
    pub summary: String,
    pub detail: String,
    pub status: ExecutionRowStatus,
    pub expanded: bool,
    pub identity: String,
}
