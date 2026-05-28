use serde_json::Value;

use crate::tui::{
    run_timeline::parser::{
        arrays_at, compact_json, event_value, failure_detail, first_string, first_string_nested,
        first_text_nested, value_at, value_string, value_text_at, value_u64,
    },
    subagent::state::{
        ModelAllocation, SubagentAskPause, SubagentBatch, SubagentChild, SubagentProcess,
        SubagentStatus, SubagentToolCall, SubagentTree,
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
        "model.allocation.selected" => tree.upsert_model(model_from_value(payload)),
        "subagent.batch.start" | "subagent.batch.end" => {
            tree.upsert_batch(batch_from_value(
                payload,
                SubagentStatus::from_event_type(event_type),
            ));
        }
        "subagent.child.start" | "subagent.child.end" => {
            tree.upsert_child(child_from_value(
                payload,
                SubagentStatus::from_event_type(event_type),
            ));
        }
        "mcp.tool.call.executed" | "tool.call.executed" => {
            merge_tool_call_with_status(tree, payload, SubagentStatus::Completed);
        }
        event_type if event_type.starts_with("sandbox.tool.") => {
            merge_tool_call_with_status(tree, payload, SubagentStatus::from_event_type(event_type));
        }
        event_type if event_type.starts_with("tool.") => {
            merge_tool_call_with_status(tree, payload, SubagentStatus::from_event_type(event_type));
        }
        event_type if event_type.starts_with("process.") => {
            merge_process(tree, payload, SubagentStatus::from_event_type(event_type));
        }
        "executive.loop.paused" => mark_needs_user(tree, payload),
        "memory.ask.answered" | "confirm.answered" => mark_ask_answered(tree, payload),
        _ => {}
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

    for calls in arrays_at(data, &["toolExecutions", "toolCalls", "tools"]) {
        for call in calls {
            tree.upsert_tool_call(tool_call_from_value(call));
        }
    }

    for events in arrays_at(data, &["events"]) {
        for event in events {
            merge_snapshot_event(tree, event);
        }
    }

    for processes in arrays_at(data, &["processes", "subprocesses"]) {
        for process in processes {
            tree.upsert_process(process_from_value(
                process,
                SubagentStatus::from_value(process),
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
        job_id: value_string(value, "jobId").or_else(|| value_string(value, "job_id")),
        status,
        allowed_tools: string_list(value, &["allowedTools", "allowed_tools", "tools"]),
        children: Vec::new(),
    }
}

fn child_from_value(value: &Value, status: SubagentStatus) -> SubagentChild {
    let id = value_string(value, "childId")
        .or_else(|| value_string(value, "id"))
        .or_else(|| value_string(value, "agentId"))
        .or_else(|| value_string(value, "subagentId"))
        .or_else(|| value_string(value, "childJobId"))
        .unwrap_or_else(|| compact_json(value));
    let tool_calls = arrays_at(value, &["toolCalls", "tool_calls", "calls"])
        .into_iter()
        .flat_map(|calls| calls.iter())
        .map(tool_call_from_value)
        .collect();
    let processes = arrays_at(value, &["processes", "subprocesses"])
        .into_iter()
        .flat_map(|processes| processes.iter())
        .map(|process| process_from_value(process, SubagentStatus::from_value(process)))
        .collect();
    SubagentChild {
        batch_id: value_string(value, "batchId").or_else(|| value_string(value, "batch_id")),
        job_id: value_string(value, "jobId")
            .or_else(|| value_string(value, "job_id"))
            .or_else(|| value_string(value, "childJobId")),
        name: first_string(value, &["name", "title", "role"]).unwrap_or_else(|| id.clone()),
        task: first_string(value, &["task", "summary", "prompt"]),
        id,
        status,
        limited: value_bool(value, "limited"),
        limit_reason: first_string(value, &["limitReason", "limit_reason"]),
        suppressed_ask_required: value_bool(value, "suppressedAskRequired")
            || value_bool(value, "suppressed_ask_required"),
        model: model_from_nested(value),
        allowed_tools: string_list(value, &["allowedTools", "allowed_tools", "tools"]),
        tool_calls,
        processes,
        ask: ask_from_value(value),
        crystal: first_string(value, &["crystalCandidate", "crystal_candidate", "crystal"]),
    }
}

fn tool_call_from_value(value: &Value) -> SubagentToolCall {
    let envelope = value;
    let value = tool_payload_value(value);
    let id = value_string(value, "id")
        .or_else(|| value_string(envelope, "id"))
        .or_else(|| value_string(value, "callId"))
        .or_else(|| value_string(envelope, "callId"))
        .or_else(|| value_string(value, "toolCallId"))
        .or_else(|| value_string(envelope, "toolCallId"))
        .or_else(|| value_text_at(value, &["call", "id"]))
        .or_else(|| value_text_at(value, &["call", "callId"]))
        .unwrap_or_else(|| compact_json(value));
    let server = first_string_nested(
        value,
        &[
            &["call", "server"],
            &["server"],
            &["serverName"],
            &["server_name"],
        ],
    );
    let tool = first_string_nested(
        value,
        &[
            &["call", "tool"],
            &["content", "call", "tool"],
            &["payload", "call", "tool"],
            &["tool", "key"],
            &["content", "tool", "key"],
            &["payload", "tool", "key"],
            &["tool"],
            &["content", "tool"],
            &["payload", "tool"],
            &["toolName"],
            &["name"],
        ],
    );
    let name = tool
        .clone()
        .or_else(|| server.clone())
        .or_else(|| first_string(value, &["command", "cmd"]))
        .unwrap_or_else(|| id.clone());
    let input_preview = first_string_nested(
        value,
        &[
            &["metadata", "preview"],
            &["call", "inputPreview"],
            &["call", "argsPreview"],
            &["argsPreview"],
            &["args_preview"],
            &["argumentsPreview"],
        ],
    )
    .or_else(|| first_text_nested(value, &[&["call", "input"], &["input"], &["args"]]));
    let result_preview = first_string_nested(
        value,
        &[
            &["result", "preview"],
            &["result", "summary"],
            &["result", "outputPath"],
        ],
    )
    .or_else(|| first_text_nested(value, &[&["result", "raw"], &["result"]]));
    SubagentToolCall {
        name,
        id,
        status: match SubagentStatus::from_value(value) {
            SubagentStatus::Unknown => SubagentStatus::from_value(envelope),
            status => status,
        },
        call_id: value_string(value, "callId")
            .or_else(|| value_string(envelope, "callId"))
            .or_else(|| value_string(value, "toolCallId"))
            .or_else(|| value_string(envelope, "toolCallId"))
            .or_else(|| value_text_at(value, &["call", "id"]))
            .or_else(|| value_text_at(value, &["call", "callId"])),
        job_id: value_string(value, "jobId")
            .or_else(|| value_string(value, "job_id"))
            .or_else(|| value_string(value, "childJobId"))
            .or_else(|| value_string(envelope, "jobId"))
            .or_else(|| value_string(envelope, "job_id"))
            .or_else(|| value_string(envelope, "childJobId")),
        child_id: value_string(value, "childId")
            .or_else(|| value_string(value, "agentId"))
            .or_else(|| value_string(value, "subagentId"))
            .or_else(|| value_string(envelope, "childId"))
            .or_else(|| value_string(envelope, "agentId"))
            .or_else(|| value_string(envelope, "subagentId")),
        server,
        tool,
        command: first_string(value, &["command", "cmd"]),
        args_preview: input_preview,
        output_path: first_string_nested(
            value,
            &[
                &["outputPath"],
                &["output_path"],
                &["path"],
                &["result", "outputPath"],
                &["result", "path"],
            ],
        ),
        error: failure_detail(value),
        duration_ms: value_u64(value, "durationMs").or_else(|| value_u64(value, "duration_ms")),
        detail: failure_detail(value)
            .or_else(|| first_string(value, &["summary", "command", "error"]))
            .or_else(|| first_string_nested(value, &[&["state", "title"]]))
            .or(result_preview),
        output_tail: output_tail_lines(value),
        processes: arrays_at(value, &["processes", "subprocesses"])
            .into_iter()
            .flat_map(|processes| processes.iter())
            .map(|process| process_from_value(process, SubagentStatus::from_value(process)))
            .collect(),
    }
}

fn merge_snapshot_event(tree: &mut SubagentTree, event: &Value) {
    let event = event_value(event).unwrap_or(event);
    if let Some(event_type) = crate::tui::run_timeline::parser::event_type(event) {
        merge_event_publish(tree, event_type, event);
        if event_type == "mcp.tool.call.executed"
            || event_type.starts_with("sandbox.tool.")
            || event_type.starts_with("tool.")
        {
            return;
        }
    }
    let payload = event
        .get("content")
        .or_else(|| event.get("payload"))
        .or_else(|| event.get("data"))
        .unwrap_or(event);
    if looks_like_tool_call(payload) {
        tree.upsert_tool_call(tool_call_from_value(event));
    }
}

fn tool_payload_value(value: &Value) -> &Value {
    value
        .get("content")
        .or_else(|| value.get("payload"))
        .or_else(|| value.get("data"))
        .unwrap_or(value)
}

fn looks_like_tool_call(value: &Value) -> bool {
    value.get("tool").is_some()
        || value.get("toolName").is_some()
        || value.get("toolCallId").is_some()
        || value.get("call").is_some()
        || value.get("content").is_some_and(looks_like_tool_call)
        || value.get("payload").is_some_and(looks_like_tool_call)
}

fn merge_tool_call_with_status(tree: &mut SubagentTree, value: &Value, status: SubagentStatus) {
    let mut call = tool_call_from_value(value);
    if status != SubagentStatus::Unknown {
        call.status = status;
    }
    tree.upsert_tool_call(call);
}

fn merge_process(tree: &mut SubagentTree, value: &Value, status: SubagentStatus) {
    tree.upsert_process(process_from_value(value, status));
}

fn mark_needs_user(tree: &mut SubagentTree, value: &Value) {
    let child_id = value_string(value, "childId")
        .or_else(|| value_string(value, "agentId"))
        .or_else(|| value_string(value, "subagentId"));
    let ask = ask_from_value(value).unwrap_or_else(|| SubagentAskPause {
        id: first_string(value, &["askId", "id"]).unwrap_or_else(|| compact_json(value)),
        status: SubagentStatus::NeedsUser,
        reason: first_string(value, &["reason", "message", "askId"]),
        crystal_candidate: first_string(value, &["crystalCandidate", "crystal_candidate"]),
        answered: false,
    });
    if let Some(child_id) = child_id {
        tree.upsert_child(SubagentChild {
            id: child_id,
            batch_id: value_string(value, "batchId").or_else(|| value_string(value, "batch_id")),
            job_id: value_string(value, "jobId").or_else(|| value_string(value, "job_id")),
            name: first_string(value, &["childName", "agentName"]).unwrap_or_default(),
            task: first_string(value, &["reason", "message", "askId"]),
            status: SubagentStatus::NeedsUser,
            limited: false,
            limit_reason: None,
            suppressed_ask_required: false,
            model: None,
            allowed_tools: Vec::new(),
            tool_calls: Vec::new(),
            processes: Vec::new(),
            ask: Some(ask),
            crystal: first_string(value, &["crystalCandidate", "crystal_candidate"]),
        });
    } else {
        tree.upsert_ask(ask, None);
    }
}

fn mark_ask_answered(tree: &mut SubagentTree, value: &Value) {
    let ask = SubagentAskPause {
        id: first_string(value, &["askId", "id"]).unwrap_or_else(|| compact_json(value)),
        status: SubagentStatus::Completed,
        reason: first_string(value, &["answerText", "text", "message"]),
        crystal_candidate: first_string(value, &["crystalCandidate", "crystal_candidate"]),
        answered: true,
    };
    let child_id = value_string(value, "childId")
        .or_else(|| value_string(value, "agentId"))
        .or_else(|| value_string(value, "subagentId"));
    tree.upsert_ask(ask, child_id);
}

fn model_from_nested(value: &Value) -> Option<ModelAllocation> {
    if let Some(model) = value
        .get("modelAllocation")
        .or_else(|| value.get("model_allocation"))
    {
        return Some(model_from_value(model));
    }
    if value.get("modelId").is_some()
        || value.get("model_id").is_some()
        || value.get("providerId").is_some()
        || value.get("provider_id").is_some()
    {
        return Some(model_from_value(value));
    }
    None
}

fn model_from_value(value: &Value) -> ModelAllocation {
    let id = first_string(value, &["allocationId", "allocation_id", "id"])
        .or_else(|| {
            let child = value_string(value, "childId").or_else(|| value_string(value, "child_id"));
            let model = first_string(value, &["modelId", "model_id", "model"]);
            match (child, model) {
                (Some(child), Some(model)) => Some(format!("{child}:{model}")),
                (_, Some(model)) => Some(model),
                _ => None,
            }
        })
        .unwrap_or_else(|| compact_json(value));
    ModelAllocation {
        id,
        request_id: value_string(value, "requestId").or_else(|| value_string(value, "request_id")),
        job_id: value_string(value, "jobId").or_else(|| value_string(value, "job_id")),
        child_id: value_string(value, "childId")
            .or_else(|| value_string(value, "child_id"))
            .or_else(|| value_string(value, "agentId"))
            .or_else(|| value_string(value, "subagentId")),
        scope: first_string(value, &["scope", "modelScope"]),
        agent_role: first_string(value, &["agentRole", "agent_role", "role"]),
        provider_id: first_string(value, &["providerId", "provider_id", "provider"]),
        model_id: first_string(value, &["modelId", "model_id", "model"]),
        reason: first_string(value, &["reason", "summary", "message"]),
        source: first_string(value, &["source", "origin"]),
    }
}

fn process_from_value(value: &Value, status: SubagentStatus) -> SubagentProcess {
    let id = value_string(value, "processId")
        .or_else(|| value_string(value, "id"))
        .or_else(|| value_string(value, "pid"))
        .or_else(|| value_string(value, "callId"))
        .unwrap_or_else(|| compact_json(value));
    SubagentProcess {
        id,
        status: if status == SubagentStatus::Unknown {
            SubagentStatus::from_value(value)
        } else {
            status
        },
        call_id: value_string(value, "callId")
            .or_else(|| value_string(value, "toolCallId"))
            .or_else(|| value_string(value, "tool_call_id"))
            .or_else(|| value_text_at(value, &["call", "id"]))
            .or_else(|| value_text_at(value, &["call", "callId"])),
        job_id: value_string(value, "jobId")
            .or_else(|| value_string(value, "job_id"))
            .or_else(|| value_string(value, "childJobId")),
        child_id: value_string(value, "childId")
            .or_else(|| value_string(value, "agentId"))
            .or_else(|| value_string(value, "subagentId")),
        command: first_string_nested(
            value,
            &[
                &["command"],
                &["cmd"],
                &["call", "tool"],
                &["tool", "key"],
                &["tool"],
            ],
        ),
        output_preview: first_string_nested(
            value,
            &[
                &["outputPreview"],
                &["output"],
                &["stdout"],
                &["stderr"],
                &["metadata", "preview"],
                &["result", "preview"],
            ],
        )
        .or_else(|| first_text_nested(value, &[&["result", "raw"]])),
        output_path: first_string_nested(
            value,
            &[
                &["outputPath"],
                &["output_path"],
                &["path"],
                &["result", "outputPath"],
                &["result", "path"],
            ],
        ),
        error: failure_detail(value),
        duration_ms: value_u64(value, "durationMs").or_else(|| value_u64(value, "duration_ms")),
        output_tail: output_tail_lines(value),
    }
}

fn output_tail_lines(value: &Value) -> Vec<String> {
    if let Some(lines) = value
        .get("resultTailLines")
        .or_else(|| value.get("outputTail"))
        .or_else(|| value.get("tailLines"))
        .and_then(Value::as_array)
    {
        return lines
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim_end)
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.chars().take(240).collect::<String>())
            .rev()
            .take(3)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
    }

    let text = first_string_nested(
        value,
        &[
            &["outputPreview"],
            &["output"],
            &["stdout"],
            &["stderr"],
            &["result", "raw", "stdout"],
            &["result", "raw", "stderr"],
        ],
    );
    text.map(|text| {
        text.lines()
            .map(str::trim_end)
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.chars().take(240).collect::<String>())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .take(3)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    })
    .unwrap_or_default()
}

fn ask_from_value(value: &Value) -> Option<SubagentAskPause> {
    if !(value.get("askId").is_some()
        || value.get("ask").is_some()
        || value.get("crystalCandidate").is_some()
        || value.get("crystal_candidate").is_some()
        || value
            .get("needsUser")
            .or_else(|| value.get("needs_user"))
            .and_then(Value::as_bool)
            .unwrap_or(false))
    {
        return None;
    }

    Some(SubagentAskPause {
        id: first_string(value, &["askId", "id"])
            .or_else(|| {
                value
                    .get("ask")
                    .and_then(|ask| first_string(ask, &["id", "askId"]))
            })
            .unwrap_or_else(|| compact_json(value)),
        status: if value
            .get("answered")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            SubagentStatus::Completed
        } else {
            SubagentStatus::NeedsUser
        },
        reason: first_string(value, &["reason", "message", "question", "askId"]),
        crystal_candidate: first_string(value, &["crystalCandidate", "crystal_candidate"]),
        answered: value
            .get("answered")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
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

fn value_bool(value: &Value, key: &str) -> bool {
    value_at(value, &[key])
        .and_then(Value::as_bool)
        .or_else(|| value_at(value, &["metadata", key]).and_then(Value::as_bool))
        .unwrap_or(false)
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
    fn model_allocation_attaches_to_subagent_child() {
        let mut tree = SubagentTree::default();
        merge_event_publish(
            &mut tree,
            "model.allocation.selected",
            &json!({
                "type": "model.allocation.selected",
                "payload": {
                    "childId": "child-1",
                    "providerId": "openai",
                    "modelId": "gpt-5",
                    "scope": "subagent-child",
                    "agentRole": "run-inspector",
                    "reason": "tool-heavy branch"
                }
            }),
        );
        merge_event_publish(
            &mut tree,
            "subagent.child.start",
            &json!({
                "type": "subagent.child.start",
                "payload": {
                    "batchId": "batch-1",
                    "childId": "child-1",
                    "task": "trace tools"
                }
            }),
        );

        let child = &tree.loose_children[0];
        let model = child.model.as_ref().expect("child model allocation");
        assert_eq!(model.provider_id.as_deref(), Some("openai"));
        assert_eq!(model.model_id.as_deref(), Some("gpt-5"));
        assert_eq!(model.agent_role.as_deref(), Some("run-inspector"));
    }

    #[test]
    fn links_tool_and_process_to_subagent_child() {
        let mut tree = SubagentTree::default();
        merge_event_publish(
            &mut tree,
            "subagent.child.start",
            &json!({
                "type": "subagent.child.start",
                "payload": { "childId": "child-1", "batchId": "batch-1" }
            }),
        );
        merge_event_publish(
            &mut tree,
            "tool.started",
            &json!({
                "type": "tool.started",
                "payload": {
                    "childId": "child-1",
                    "callId": "call-1",
                    "server": "shell",
                    "tool": "exec",
                    "argsPreview": "rg ASK src"
                }
            }),
        );
        merge_event_publish(
            &mut tree,
            "process.output",
            &json!({
                "type": "process.output",
                "payload": {
                    "childId": "child-1",
                    "callId": "call-1",
                    "processId": "proc-1",
                    "command": "rg ASK src",
                    "outputPreview": "src/main.rs"
                }
            }),
        );

        let child = &tree.loose_children[0];
        assert_eq!(child.tool_calls.len(), 1);
        assert_eq!(child.tool_calls[0].server.as_deref(), Some("shell"));
        assert_eq!(child.tool_calls[0].processes[0].id, "proc-1");
        assert_eq!(
            child.tool_calls[0].processes[0].output_preview.as_deref(),
            Some("src/main.rs")
        );
    }

    #[test]
    fn ask_pause_and_answer_preserve_crystal_closure() {
        let mut tree = SubagentTree::default();
        merge_event_publish(
            &mut tree,
            "executive.loop.paused",
            &json!({
                "type": "executive.loop.paused",
                "payload": {
                    "childId": "child-1",
                    "askId": "ask-1",
                    "reason": "tool budget exhausted",
                    "crystalCandidate": "record retry policy"
                }
            }),
        );
        merge_event_publish(
            &mut tree,
            "memory.ask.answered",
            &json!({
                "type": "memory.ask.answered",
                "payload": {
                    "childId": "child-1",
                    "askId": "ask-1",
                    "answerText": "continue",
                    "crystalCandidate": "record retry policy"
                }
            }),
        );

        let ask = tree.loose_children[0].ask.as_ref().expect("ask pause");
        assert!(ask.answered);
        assert_eq!(ask.status, SubagentStatus::Completed);
        assert_eq!(
            ask.crystal_candidate.as_deref(),
            Some("record retry policy")
        );

        merge_event_publish(
            &mut tree,
            "confirm.answered",
            &json!({
                "type": "confirm.answered",
                "payload": {
                    "childId": "child-1",
                    "askEventId": "ask-1"
                }
            }),
        );
        let ask = tree.loose_children[0].ask.as_ref().expect("confirm answer");
        assert!(ask.answered);
        assert_eq!(ask.status, SubagentStatus::Completed);
    }

    #[test]
    fn parses_limited_child_without_needs_user() {
        let mut tree = SubagentTree::default();
        merge_event_publish(
            &mut tree,
            "subagent.child.end",
            &json!({
                "type": "subagent.child.end",
                "payload": {
                    "childId": "reader",
                    "status": "completed",
                    "limited": true,
                    "limitReason": "tool-budget-exhausted",
                    "suppressedAskRequired": true,
                    "toolCalls": [{ "id": "read-1", "name": "workspace.read", "status": "completed" }]
                }
            }),
        );

        let child = &tree.loose_children[0];
        assert_eq!(child.status, SubagentStatus::Completed);
        assert!(child.limited);
        assert!(child.suppressed_ask_required);
        assert_eq!(child.limit_reason.as_deref(), Some("tool-budget-exhausted"));
    }

    #[test]
    fn parses_nested_tool_call_shape_and_links_by_child_job_id() {
        let mut tree = SubagentTree::default();
        merge_event_publish(
            &mut tree,
            "tool.call.executed",
            &json!({
                "type": "tool.call.executed",
                "payload": {
                    "id": "part-1",
                    "childJobId": "job-child-1",
                    "tool": { "key": "read" },
                    "call": {
                        "id": "call-1",
                        "server": "workspace",
                        "tool": "read",
                        "input": { "filePath": "src/main.rs" }
                    },
                    "result": { "raw": "fn main() {}" },
                    "metadata": { "preview": "src/main.rs" }
                }
            }),
        );
        merge_event_publish(
            &mut tree,
            "subagent.child.start",
            &json!({
                "type": "subagent.child.start",
                "payload": {
                    "childId": "child-1",
                    "childJobId": "job-child-1",
                    "task": "Read file"
                }
            }),
        );

        assert!(tree.loose_tool_calls.is_empty());
        let child = &tree.loose_children[0];
        assert_eq!(child.job_id.as_deref(), Some("job-child-1"));
        assert_eq!(child.tool_calls.len(), 1);
        let call = &child.tool_calls[0];
        assert_eq!(call.server.as_deref(), Some("workspace"));
        assert_eq!(call.tool.as_deref(), Some("read"));
        assert_eq!(call.args_preview.as_deref(), Some("src/main.rs"));
        assert_eq!(call.detail.as_deref(), Some("fn main() {}"));
        assert_eq!(call.status, SubagentStatus::Completed);
    }

    #[test]
    fn surfaces_nested_external_tool_failure_on_subagent_tree() {
        let mut tree = SubagentTree::default();
        merge_event_publish(
            &mut tree,
            "tool.failed",
            &json!({
                "type": "tool.failed",
                "payload": {
                    "childId": "child-1",
                    "tool": "computer.use",
                    "status": "failed",
                    "result": {
                        "response": {
                            "ok": false,
                            "code": "unavailable",
                            "error": "computer delegate unavailable",
                            "reason": "cua-driver command not found"
                        }
                    }
                }
            }),
        );
        merge_event_publish(
            &mut tree,
            "subagent.child.start",
            &json!({
                "type": "subagent.child.start",
                "payload": { "childId": "child-1", "task": "probe desktop" }
            }),
        );

        let call = &tree.loose_children[0].tool_calls[0];
        let detail = call.detail.as_deref().expect("visible failure detail");
        assert_eq!(call.status, SubagentStatus::Failed);
        assert_eq!(call.error.as_deref(), Some(detail));
        assert!(detail.contains("computer delegate unavailable"));
        assert!(detail.contains("cua-driver command not found"));
        assert!(!detail.contains('{'));
        assert!(!detail.contains("unknown"));
    }

    #[test]
    fn child_job_id_only_snapshot_does_not_render_json_child_title() {
        let mut tree = SubagentTree::default();
        merge_execution_job_snapshot(
            &mut tree,
            &json!({
                "type": "execution.job.snapshot",
                "payload": {
                    "data": {
                        "children": [{
                            "childJobId": "job-child-1",
                            "status": "running"
                        }]
                    }
                }
            }),
        );

        let child = &tree.loose_children[0];
        assert_eq!(child.id, "job-child-1");
        assert_eq!(child.name, "job-child-1");
        assert_eq!(child.job_id.as_deref(), Some("job-child-1"));
        assert!(child.task.is_none());
        assert!(!child.name.starts_with('{'));
    }

    #[test]
    fn parses_tool_executions_and_event_content_tools_from_snapshot() {
        let mut tree = SubagentTree::default();
        merge_execution_job_snapshot(
            &mut tree,
            &json!({
                "type": "execution.job.snapshot",
                "payload": {
                    "data": {
                        "children": [{
                            "childId": "child-1",
                            "childJobId": "job-child-1",
                            "task": "inspect workspace"
                        }],
                        "toolExecutions": [{
                            "id": "call-1",
                            "childJobId": "job-child-1",
                            "call": {
                                "server": "workspace",
                                "tool": "tree",
                                "input": { "path": "src/tui" }
                            },
                            "metadata": { "preview": "src/tui" },
                            "status": "completed"
                        }],
                        "events": [{
                            "childJobId": "job-child-1",
                            "content": {
                                "id": "call-2",
                                "tool": "glob",
                                "call": {
                                    "server": "workspace",
                                    "tool": "glob",
                                    "input": { "pattern": "src/**/*.rs" }
                                },
                                "metadata": { "preview": "src/**/*.rs" },
                                "status": "completed"
                            }
                        }]
                    }
                }
            }),
        );

        let child = &tree.loose_children[0];
        assert_eq!(child.tool_calls.len(), 2);
        assert_eq!(child.tool_calls[0].server.as_deref(), Some("workspace"));
        assert_eq!(child.tool_calls[0].tool.as_deref(), Some("tree"));
        assert_eq!(child.tool_calls[1].server.as_deref(), Some("workspace"));
        assert_eq!(child.tool_calls[1].tool.as_deref(), Some("glob"));
    }
}
