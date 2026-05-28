use super::state::ActiveForkSession;

pub fn session_summary(active_fork: Option<&ActiveForkSession>) -> Option<&str> {
    active_fork.and_then(|fork| fork.summary.as_deref())
}
