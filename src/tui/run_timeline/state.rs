use serde_json::Value;

use crate::tui::subagent::state::SubagentTree;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunTimelineItemKind {
    Route,
    Recall,
    Tool,
    Blackboard,
    Subagent,
    Ask,
    Plan,
    Fork,
    Loop,
    Snapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunTimelineItemStatus {
    Pending,
    Running,
    NeedsUser,
    Completed,
    Failed,
    Info,
}

impl RunTimelineItemStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::NeedsUser => "needs_user",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Info => "info",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunTimelineSource {
    EventPublish,
    ExecutionJobSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunTimelineItem {
    pub id: String,
    pub kind: RunTimelineItemKind,
    pub status: RunTimelineItemStatus,
    pub title: String,
    pub detail: Option<String>,
    pub at: Option<String>,
    pub source: RunTimelineSource,
    pub raw: Option<Value>,
}

impl RunTimelineItem {
    pub fn new(
        id: impl Into<String>,
        kind: RunTimelineItemKind,
        status: RunTimelineItemStatus,
        title: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            status,
            title: title.into(),
            detail: None,
            at: None,
            source: RunTimelineSource::EventPublish,
            raw: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        let detail = detail.into();
        if !detail.is_empty() {
            self.detail = Some(detail);
        }
        self
    }

    pub fn with_at(mut self, at: Option<String>) -> Self {
        self.at = at;
        self
    }

    pub fn with_source(mut self, source: RunTimelineSource) -> Self {
        self.source = source;
        self
    }

    pub fn with_raw(mut self, raw: Option<Value>) -> Self {
        self.raw = raw;
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunTimeline {
    pub items: Vec<RunTimelineItem>,
    pub subagents: SubagentTree,
}

impl RunTimeline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply_event_publish(&mut self, value: &Value) -> Option<RunTimelineItem> {
        let parsed = crate::tui::run_timeline::parser::parse_event_publish(value)?;
        if let Some(event_type) = parsed.raw.as_ref().and_then(event_type_from_raw) {
            crate::tui::subagent::parser::merge_event_publish(
                &mut self.subagents,
                event_type,
                value,
            );
        }
        self.items.push(parsed.clone());
        Some(parsed)
    }

    pub fn apply_execution_job_snapshot(&mut self, value: &Value) -> Vec<RunTimelineItem> {
        let parsed = crate::tui::run_timeline::parser::parse_execution_job_snapshot(value);
        crate::tui::subagent::parser::merge_execution_job_snapshot(&mut self.subagents, value);
        self.items.extend(parsed.iter().cloned());
        parsed
    }

    pub fn apply_json(&mut self, value: &Value) -> Vec<RunTimelineItem> {
        let parsed = crate::tui::run_timeline::parser::parse_timeline_input(value);
        for item in &parsed {
            if item.source == RunTimelineSource::ExecutionJobSnapshot {
                crate::tui::subagent::parser::merge_execution_job_snapshot(
                    &mut self.subagents,
                    value,
                );
            } else if let Some(event_type) = item.raw.as_ref().and_then(event_type_from_raw) {
                crate::tui::subagent::parser::merge_event_publish(
                    &mut self.subagents,
                    event_type,
                    value,
                );
            }
        }
        self.items.extend(parsed.iter().cloned());
        parsed
    }
}

fn event_type_from_raw(raw: &Value) -> Option<&str> {
    raw.get("type")
        .or_else(|| raw.get("eventType"))
        .or_else(|| raw.get("name"))
        .and_then(Value::as_str)
        .or_else(|| {
            raw.get("event")
                .and_then(|event| event.get("type"))
                .and_then(Value::as_str)
        })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn timeline_applies_events_and_snapshots_to_items_and_tree() {
        let mut timeline = RunTimeline::new();
        let event = json!({
            "type": "subagent.child.start",
            "payload": {
                "batchId": "batch-1",
                "childId": "child-1",
                "task": "inspect"
            }
        });

        let item = timeline.apply_event_publish(&event).expect("event item");
        assert_eq!(item.status.as_str(), "running");
        assert_eq!(timeline.items.len(), 1);
        assert_eq!(timeline.subagents.loose_children[0].id, "child-1");

        let snapshot_items = timeline.apply_execution_job_snapshot(&json!({
            "data": {
                "batches": [{ "id": "batch-1", "status": "running" }],
                "children": [{ "id": "child-1", "batchId": "batch-1", "status": "completed" }]
            }
        }));
        assert!(snapshot_items.iter().any(|item| item.id == "batch:batch-1"));
        assert_eq!(
            timeline.subagents.batches[0].children[0].status.as_str(),
            "completed"
        );
    }

    #[test]
    fn timeline_apply_json_routes_by_message_type() {
        let mut timeline = RunTimeline::new();
        let items = timeline.apply_json(&json!({
            "type": "execution.job.snapshot",
            "data": { "jobId": "job-1", "status": "running" }
        }));

        assert_eq!(items[0].id, "job:job-1");
        assert_eq!(
            timeline.items[0].source,
            RunTimelineSource::ExecutionJobSnapshot
        );
    }
}
