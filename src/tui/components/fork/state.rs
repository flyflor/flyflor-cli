#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveForkSession {
    pub fork_id: String,
    pub parent_fork_id: Option<String>,
    pub root_id: Option<String>,
    pub summary: Option<String>,
}
