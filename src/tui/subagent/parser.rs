use serde_json::Value;

use crate::tui::{
    run_timeline::parser::{arrays_at, compact_json, event_value, first_string, value_string},
    subagent::state::{
        SubagentBatch, SubagentChild, SubagentStatus, SubagentToolCall, SubagentTree,
    },
};

pub fn merge_event_publish(tree: &mut SubagentTree, event_type: &str, value: &Value) {
    let Some(event) = event_value(value) else {
        return;
    };
    let payload = event
        .get("payload")
        .or_else(|| event.get("data"))
        .unwrap_or(event);

    match event_type {
        "subagent.batch.start" | "subagent.batch.end" => {
            tree.upsert_batch(batch_from_value(
                payload,
                subagent_status(payload, event_type),
            ));
        }
        "subagent.child.start" | "subagent.child.end" => {
            tree.upsert_child(child_from_value(
                payload,
                subagent_status(payload, event_type),
            ));
        }
        "mcp.tool.call.executed" | "tool.call.executed" => merge_tool_call(tree, payload),
        event_type if event_type.starts_with("tool.") => merge_tool_call(tree, payload),
        "executive.loop.paused" => mark_needs_user(tree, payload),
        _ => {}
    }
}

fn subagent_status(payload: &Value, event_type: &str) -> SubagentStatus {
    let status = SubagentStatus::from_value(payload);
    if status == SubagentStatus::Unknown {
        SubagentStatus::from_event_type(event_type)
    } else {
        status
    }
}

pub fn merge_execution_job_snapshot(tree: &mut SubagentTree, value: &Value) {
    let data = value
        .get("payload")
        .and_then(|payload| payload.get("data"))
        .or_else(|| value.get("data"))
        .or_else(|| value.get("job"))
        .unwrap_or(value);

    for batches in arrays_at(data, &["batches", "subagentBatches", "subagents"]) {
        for batch_value in batches {
            let mut batch = batch_from_value(batch_value, SubagentStatus::from_value(batch_value));
            for children in arrays_at(batch_value, &["children", "subagentChildren", "agents"]) {
                for child in children {
                    batch
                        .children
                        .push(child_from_value(child, SubagentStatus::from_value(child)));
                }
            }
            tree.upsert_batch(batch);
        }
    }

    for children in arrays_at(data, &["children", "subagentChildren", "agents"]) {
        for child_value in children {
            tree.upsert_child(child_from_value(
                child_value,
                SubagentStatus::from_value(child_value),
            ));
        }
    }
}

fn batch_from_value(value: &Value, status: SubagentStatus) -> SubagentBatch {
    let id = value_string(value, "batchId")
        .or_else(|| value_string(value, "id"))
        .unwrap_or_else(|| compact_json(value));
    SubagentBatch {
        name: first_string(value, &["name", "title"]).unwrap_or_else(|| id.clone()),
        id,
        status,
        allowed_tools: string_list(value, &["allowedTools", "allowed_tools", "tools"]),
        children: Vec::new(),
    }
}

fn child_from_value(value: &Value, status: SubagentStatus) -> SubagentChild {
    let id = value_string(value, "childId")
        .or_else(|| value_string(value, "id"))
        .or_else(|| value_string(value, "agentId"))
        .unwrap_or_else(|| compact_json(value));
    let tool_calls = arrays_at(value, &["toolCalls", "tool_calls", "calls"])
        .into_iter()
        .flat_map(|calls| calls.iter())
        .map(tool_call_from_value)
        .collect();
    SubagentChild {
        batch_id: value_string(value, "batchId").or_else(|| value_string(value, "batch_id")),
        name: first_string(value, &["name", "title", "role"]).unwrap_or_else(|| id.clone()),
        task: first_string(value, &["task", "summary", "prompt"]),
        id,
        status,
        allowed_tools: string_list(value, &["allowedTools", "allowed_tools", "tools"]),
        tool_calls,
    }
}

fn tool_call_from_value(value: &Value) -> SubagentToolCall {
    let id = value_string(value, "id")
        .or_else(|| value_string(value, "callId"))
        .or_else(|| value_string(value, "toolCallId"))
        .unwrap_or_else(|| compact_json(value));
    SubagentToolCall {
        name: first_string(value, &["name", "toolName", "tool"]).unwrap_or_else(|| id.clone()),
        id,
        status: SubagentStatus::from_value(value),
        detail: first_string(value, &["summary", "command", "result", "error"]),
    }
}

fn merge_tool_call(tree: &mut SubagentTree, value: &Value) {
    let Some(child_id) = value_string(value, "childId")
        .or_else(|| value_string(value, "agentId"))
        .or_else(|| value_string(value, "subagentId"))
    else {
        return;
    };
    let child = SubagentChild {
        id: child_id,
        batch_id: value_string(value, "batchId").or_else(|| value_string(value, "batch_id")),
        name: first_string(value, &["childName", "agentName"]).unwrap_or_default(),
        task: None,
        status: SubagentStatus::Unknown,
        allowed_tools: Vec::new(),
        tool_calls: vec![tool_call_from_value(value)],
    };
    tree.upsert_child(child);
}

fn mark_needs_user(tree: &mut SubagentTree, value: &Value) {
    let Some(child_id) = value_string(value, "childId")
        .or_else(|| value_string(value, "agentId"))
        .or_else(|| value_string(value, "subagentId"))
    else {
        return;
    };
    tree.upsert_child(SubagentChild {
        id: child_id,
        batch_id: value_string(value, "batchId").or_else(|| value_string(value, "batch_id")),
        name: first_string(value, &["childName", "agentName"]).unwrap_or_default(),
        task: first_string(value, &["reason", "message", "askId"]),
        status: SubagentStatus::NeedsUser,
        allowed_tools: Vec::new(),
        tool_calls: Vec::new(),
    });
}

fn string_list(value: &Value, keys: &[&str]) -> Vec<String> {
    for key in keys {
        match value.get(key) {
            Some(Value::Array(items)) => {
                return items
                    .iter()
                    .filter_map(|item| match item {
                        Value::String(text) => Some(text.clone()),
                        Value::Object(_) => first_string(item, &["name", "toolName", "id"]),
                        _ => None,
                    })
                    .collect();
            }
            Some(Value::String(text)) => {
                return text
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(str::to_string)
                    .collect();
            }
            _ => {}
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn merges_snapshot_into_visible_tree() {
        let mut tree = SubagentTree::default();
        merge_execution_job_snapshot(
            &mut tree,
            &json!({
                "type": "execution.job.snapshot",
                "payload": {
                    "data": {
                        "batches": [{
                            "id": "batch-1",
                            "status": "running",
                            "allowedTools": ["rg", "sed"],
                            "children": [{
                                "id": "child-1",
                                "batchId": "batch-1",
                                "status": "running",
                                "task": "inspect parser",
                                "toolCalls": [{ "id": "call-1", "name": "rg", "status": "completed" }]
                            }]
                        }]
                    }
                }
            }),
        );

        assert_eq!(tree.batches.len(), 1);
        assert_eq!(tree.batches[0].allowed_tools, vec!["rg", "sed"]);
        assert_eq!(tree.batches[0].children.len(), 1);
        assert_eq!(tree.batches[0].children[0].tool_calls[0].name, "rg");
    }

    #[test]
    fn event_updates_snapshot_child_status() {
        let mut tree = SubagentTree::default();
        merge_execution_job_snapshot(
            &mut tree,
            &json!({
                "data": {
                    "batches": [{ "id": "batch-1" }],
                    "children": [{ "id": "child-1", "batchId": "batch-1", "status": "running" }]
                }
            }),
        );
        merge_event_publish(
            &mut tree,
            "executive.loop.paused",
            &json!({
                "type": "executive.loop.paused",
                "payload": {
                    "batchId": "batch-1",
                    "childId": "child-1",
                    "reason": "Need user approval"
                }
            }),
        );

        let child = &tree.batches[0].children[0];
        assert_eq!(child.status, SubagentStatus::NeedsUser);
        assert_eq!(child.task.as_deref(), Some("Need user approval"));
    }

    #[test]
    fn child_end_prefers_payload_status() {
        let mut tree = SubagentTree::default();

        merge_event_publish(
            &mut tree,
            "subagent.child.end",
            &json!({
                "type": "subagent.child.end",
                "payload": {
                    "batchId": "batch-1",
                    "childId": "child-1",
                    "status": "needs_user",
                    "summary": "Pick an option"
                }
            }),
        );

        assert_eq!(tree.loose_children[0].status, SubagentStatus::NeedsUser);
    }
}
