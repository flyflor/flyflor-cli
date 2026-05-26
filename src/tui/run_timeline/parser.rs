use serde_json::Value;

use crate::tui::run_timeline::state::{
    RunTimelineItem, RunTimelineItemKind, RunTimelineItemStatus, RunTimelineSource,
};

pub fn parse_timeline_input(value: &Value) -> Vec<RunTimelineItem> {
    let message_type = value
        .get("type")
        .or_else(|| value.get("messageType"))
        .and_then(Value::as_str);

    if message_type == Some("execution.job.snapshot") {
        return parse_execution_job_snapshot(value);
    }

    if let Some(payload) = value.get("payload") {
        let payload_type = payload
            .get("type")
            .or_else(|| payload.get("messageType"))
            .and_then(Value::as_str);
        if payload_type == Some("execution.job.snapshot")
            || message_type == Some("execution.job.snapshot")
        {
            return parse_execution_job_snapshot(payload);
        }
    }

    parse_event_publish(value).into_iter().collect()
}

pub fn parse_event_publish(value: &Value) -> Option<RunTimelineItem> {
    let event = event_value(value)?;
    let event_type = event_type(event)?;
    let payload = event
        .get("payload")
        .or_else(|| event.get("data"))
        .unwrap_or(event);
    let at = event
        .get("at")
        .or_else(|| value.get("at"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let id = value_string(event, "id")
        .or_else(|| value_string(event, "eventId"))
        .or_else(|| value_string(event, "requestId"))
        .or_else(|| value_string(value, "id"))
        .unwrap_or_else(|| stable_id(event_type, payload));

    let item = match event_type {
        "route.escalated" => RunTimelineItem::new(
            id,
            RunTimelineItemKind::Route,
            RunTimelineItemStatus::Info,
            "route escalated",
        )
        .with_detail(
            first_string(
                payload,
                &["reason", "route", "target", "model", "summary", "message"],
            )
            .unwrap_or_else(|| compact_json(payload)),
        ),
        event_type if event_type.starts_with("scope.recall.") => RunTimelineItem::new(
            id,
            RunTimelineItemKind::Recall,
            status_from_event_type(event_type),
            event_type.replace('.', " "),
        )
        .with_detail(first_string(payload, &["query", "summary", "content"]).unwrap_or_default()),
        event_type if event_type.starts_with("blackboard.") => RunTimelineItem::new(
            id,
            RunTimelineItemKind::Blackboard,
            status_from_event_type(event_type),
            event_type.replace('.', " "),
        )
        .with_detail(
            first_string(payload, &["summary", "text", "message", "content"])
                .unwrap_or_else(|| compact_json(payload)),
        ),
        "mcp.tool.call.executed" => tool_item(id, "mcp tool", payload),
        event_type if event_type.starts_with("tool.") => tool_item(id, event_type, payload),
        "subagent.batch.start" | "subagent.batch.end" => RunTimelineItem::new(
            id,
            RunTimelineItemKind::Subagent,
            status_from_event_type(event_type),
            format!(
                "subagent batch {}",
                if event_type.ends_with(".start") {
                    "started"
                } else {
                    "ended"
                }
            ),
        )
        .with_detail(
            first_string(payload, &["batchId", "id", "name", "summary"]).unwrap_or_default(),
        ),
        "subagent.child.start" | "subagent.child.end" => RunTimelineItem::new(
            id,
            RunTimelineItemKind::Subagent,
            status_from_event_type(event_type),
            format!(
                "subagent child {}",
                if event_type.ends_with(".start") {
                    "started"
                } else {
                    "ended"
                }
            ),
        )
        .with_detail(
            first_string(payload, &["childId", "id", "name", "task", "summary"])
                .unwrap_or_default(),
        ),
        "executive.loop.paused" | "executive.loop.resumed" => RunTimelineItem::new(
            id,
            RunTimelineItemKind::Loop,
            if event_type.ends_with(".paused") {
                RunTimelineItemStatus::NeedsUser
            } else {
                RunTimelineItemStatus::Running
            },
            if event_type.ends_with(".paused") {
                "executive loop paused"
            } else {
                "executive loop resumed"
            },
        )
        .with_detail(first_string(payload, &["reason", "message", "askId"]).unwrap_or_default()),
        event_type if event_type.starts_with("ask.") || event_type.contains(".ask.") => {
            RunTimelineItem::new(
                id,
                RunTimelineItemKind::Ask,
                status_from_event_type(event_type),
                event_type.replace('.', " "),
            )
            .with_detail(
                first_string(payload, &["askId", "question", "message"]).unwrap_or_default(),
            )
        }
        event_type
            if event_type.starts_with("plan.")
                || event_type.contains(".plan.")
                || event_type.starts_with("memory.task_plan.") =>
        {
            RunTimelineItem::new(
                id,
                RunTimelineItemKind::Plan,
                status_from_event_type(event_type),
                event_type.replace('.', " "),
            )
            .with_detail(
                first_string(payload, &["planId", "summary", "message"]).unwrap_or_default(),
            )
        }
        event_type
            if event_type == "memory.context_fork.written"
                || event_type.starts_with("fork.")
                || event_type.contains(".fork.") =>
        {
            RunTimelineItem::new(
                id,
                RunTimelineItemKind::Fork,
                status_from_event_type(event_type),
                event_type.replace('.', " "),
            )
            .with_detail(
                first_string(payload, &["forkId", "summary", "message"]).unwrap_or_default(),
            )
        }
        _ => return None,
    };

    Some(
        item.with_at(at)
            .with_source(RunTimelineSource::EventPublish)
            .with_raw(Some(event.clone())),
    )
}

pub fn parse_execution_job_snapshot(value: &Value) -> Vec<RunTimelineItem> {
    let data = value
        .get("payload")
        .and_then(|payload| payload.get("data"))
        .or_else(|| value.get("data"))
        .or_else(|| value.get("job"))
        .unwrap_or(value);

    let mut items = Vec::new();
    if let Some(job_id) = value_string(data, "id").or_else(|| value_string(data, "jobId")) {
        let mut item = RunTimelineItem::new(
            format!("job:{job_id}"),
            RunTimelineItemKind::Snapshot,
            status_from_value(data),
            format!("execution job {job_id}"),
        )
        .with_source(RunTimelineSource::ExecutionJobSnapshot)
        .with_raw(Some(data.clone()));
        if let Some(detail) = first_string(data, &["summary", "title", "message"]) {
            item = item.with_detail(detail);
        }
        items.push(item);
    }

    for batch in arrays_at(data, &["batches", "subagentBatches", "subagents"]) {
        for entry in batch {
            let batch_id = value_string(entry, "batchId")
                .or_else(|| value_string(entry, "id"))
                .unwrap_or_else(|| stable_id("batch", entry));
            items.push(
                RunTimelineItem::new(
                    format!("batch:{batch_id}"),
                    RunTimelineItemKind::Subagent,
                    status_from_value(entry),
                    format!("subagent batch {batch_id}"),
                )
                .with_detail(first_string(entry, &["name", "summary", "task"]).unwrap_or_default())
                .with_source(RunTimelineSource::ExecutionJobSnapshot)
                .with_raw(Some(entry.clone())),
            );
        }
    }

    for child in arrays_at(data, &["children", "subagentChildren", "agents"]) {
        for entry in child {
            let child_id = value_string(entry, "childId")
                .or_else(|| value_string(entry, "id"))
                .unwrap_or_else(|| stable_id("child", entry));
            items.push(
                RunTimelineItem::new(
                    format!("child:{child_id}"),
                    RunTimelineItemKind::Subagent,
                    status_from_value(entry),
                    format!("subagent child {child_id}"),
                )
                .with_detail(first_string(entry, &["name", "task", "summary"]).unwrap_or_default())
                .with_source(RunTimelineSource::ExecutionJobSnapshot)
                .with_raw(Some(entry.clone())),
            );
        }
    }

    items
}

fn tool_item(id: String, event_type: &str, payload: &Value) -> RunTimelineItem {
    let name = first_string(payload, &["toolName", "name", "tool", "server"])
        .unwrap_or_else(|| event_type.replace('.', " "));
    RunTimelineItem::new(
        id,
        RunTimelineItemKind::Tool,
        status_from_value(payload),
        format!("tool {name}"),
    )
    .with_detail(
        first_string(payload, &["summary", "command", "result", "error"]).unwrap_or_default(),
    )
}

pub(crate) fn event_value(value: &Value) -> Option<&Value> {
    if value
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|event_type| event_type != "event.publish")
    {
        return Some(value);
    }

    value
        .get("payload")
        .and_then(|payload| payload.get("event").or(Some(payload)))
        .or_else(|| value.get("event"))
}

pub(crate) fn event_type(event: &Value) -> Option<&str> {
    event
        .get("type")
        .or_else(|| event.get("eventType"))
        .or_else(|| event.get("name"))
        .and_then(Value::as_str)
}

pub(crate) fn value_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(|value| match value {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    })
}

pub(crate) fn first_string(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(text) = value_string(value, key) {
            return Some(text);
        }
    }
    None
}

pub(crate) fn status_from_event_type(event_type: &str) -> RunTimelineItemStatus {
    if event_type.ends_with(".start")
        || event_type.ends_with(".started")
        || event_type.ends_with(".resumed")
    {
        RunTimelineItemStatus::Running
    } else if event_type.ends_with(".end")
        || event_type.ends_with(".ended")
        || event_type.ends_with(".completed")
        || event_type.ends_with(".written")
    {
        RunTimelineItemStatus::Completed
    } else if event_type.ends_with(".failed") || event_type.ends_with(".error") {
        RunTimelineItemStatus::Failed
    } else if event_type.ends_with(".paused") || event_type.ends_with(".needs_user") {
        RunTimelineItemStatus::NeedsUser
    } else {
        RunTimelineItemStatus::Info
    }
}

pub(crate) fn status_from_value(value: &Value) -> RunTimelineItemStatus {
    if value
        .get("needsUser")
        .or_else(|| value.get("needs_user"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return RunTimelineItemStatus::NeedsUser;
    }

    let status = value
        .get("status")
        .or_else(|| value.get("state"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    match status.as_str() {
        "pending" | "queued" => RunTimelineItemStatus::Pending,
        "running" | "started" | "in_progress" | "in-progress" => RunTimelineItemStatus::Running,
        "needs_user" | "needs-user" | "paused" | "waiting_for_user" => {
            RunTimelineItemStatus::NeedsUser
        }
        "completed" | "complete" | "succeeded" | "success" | "done" => {
            RunTimelineItemStatus::Completed
        }
        "failed" | "error" | "cancelled" | "canceled" => RunTimelineItemStatus::Failed,
        _ => RunTimelineItemStatus::Info,
    }
}

pub(crate) fn arrays_at<'a>(value: &'a Value, keys: &[&str]) -> Vec<&'a Vec<Value>> {
    let mut arrays = Vec::new();
    for key in keys {
        if let Some(array) = value.get(key).and_then(Value::as_array) {
            arrays.push(array);
        }
    }
    arrays
}

pub(crate) fn stable_id(prefix: &str, value: &Value) -> String {
    format!("{prefix}:{}", compact_json(value))
}

pub(crate) fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn parses_event_publish_timeline_items() {
        let event = json!({
            "type": "event.publish",
            "payload": {
                "event": {
                    "type": "route.escalated",
                    "at": "2026-05-26T01:02:03Z",
                    "payload": { "reason": "needs stronger model" }
                }
            }
        });

        let item = parse_event_publish(&event).expect("route event");
        assert_eq!(item.kind, RunTimelineItemKind::Route);
        assert_eq!(item.status, RunTimelineItemStatus::Info);
        assert_eq!(item.title, "route escalated");
        assert_eq!(item.detail.as_deref(), Some("needs stronger model"));
        assert_eq!(item.at.as_deref(), Some("2026-05-26T01:02:03Z"));
    }

    #[test]
    fn parses_required_event_families() {
        let cases = [
            ("scope.recall.started", RunTimelineItemKind::Recall),
            (
                "blackboard.message.appended",
                RunTimelineItemKind::Blackboard,
            ),
            ("mcp.tool.call.executed", RunTimelineItemKind::Tool),
            ("tool.shell.completed", RunTimelineItemKind::Tool),
            ("subagent.batch.start", RunTimelineItemKind::Subagent),
            ("subagent.child.end", RunTimelineItemKind::Subagent),
            ("executive.loop.paused", RunTimelineItemKind::Loop),
            ("executive.loop.resumed", RunTimelineItemKind::Loop),
            ("memory.task_plan.written", RunTimelineItemKind::Plan),
            ("memory.context_fork.written", RunTimelineItemKind::Fork),
        ];

        for (event_type, kind) in cases {
            let item = parse_event_publish(&json!({
                "type": event_type,
                "payload": { "summary": "visible" }
            }))
            .unwrap_or_else(|| panic!("parse {event_type}"));
            assert_eq!(item.kind, kind);
        }
    }

    #[test]
    fn parses_execution_job_snapshot_items() {
        let items = parse_execution_job_snapshot(&json!({
            "type": "execution.job.snapshot",
            "payload": {
                "data": {
                    "jobId": "job-1",
                    "status": "running",
                    "batches": [{ "id": "batch-1", "status": "running" }],
                    "children": [{ "id": "child-1", "status": "needs_user", "task": "confirm" }]
                }
            }
        }));

        assert!(items.iter().any(|item| item.id == "job:job-1"));
        assert!(items.iter().any(|item| item.id == "batch:batch-1"));
        let child = items
            .iter()
            .find(|item| item.id == "child:child-1")
            .expect("child item");
        assert_eq!(child.status, RunTimelineItemStatus::NeedsUser);
    }
}
