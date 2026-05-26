use serde_json::{Value, json};

pub const SUBSCRIPTION_EVENT_TYPES: &[&str] = &[
    "memory.task_plan.written",
    "memory.task_plan.decided",
    "memory.context_fork.written",
    "memory.ask.recorded",
    "memory.ask.answered",
    "executive.loop.paused",
    "executive.loop.resumed",
    "route.escalated",
    "scope.recall.started",
    "scope.recall.decided",
    "scope.recall.loaded",
    "scope.recall.ask",
    "blackboard.started",
    "blackboard.round.started",
    "blackboard.worker.done",
    "blackboard.message.appended",
    "blackboard.turn.end",
    "blackboard.completed",
    "mcp.tool.call.executed",
    "tool.started",
    "tool.progress",
    "tool.succeeded",
    "tool.failed",
    "tool.ask_required",
    "tool.budget.exhausted",
    "subagent.batch.start",
    "subagent.child.start",
    "subagent.child.end",
    "subagent.batch.end",
];

pub const BOOTSTRAP_COMMAND_TYPES: &[&str] = &[
    "client.hello",
    "history.list",
    "task.list",
    "gateway.status.get",
    "fork.memory.get",
    "event.subscribe",
];

pub fn subscription_payload() -> Value {
    json!({
        "types": SUBSCRIPTION_EVENT_TYPES
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscription_list_is_fixed_to_known_runtime_events() {
        let payload = subscription_payload();
        let types = payload
            .get("types")
            .and_then(Value::as_array)
            .expect("types array")
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();

        assert_eq!(types, SUBSCRIPTION_EVENT_TYPES);
        assert!(types.contains(&"memory.task_plan.written"));
        assert!(types.contains(&"memory.context_fork.written"));
        assert!(types.contains(&"executive.loop.paused"));
        assert!(types.contains(&"blackboard.message.appended"));
        assert!(types.contains(&"subagent.child.end"));
        assert!(types.contains(&"tool.budget.exhausted"));
        assert!(payload.get("classes").is_none());
        assert!(
            !types
                .iter()
                .any(|event_type| event_type.starts_with("fork.memory."))
        );
        assert!(
            types
                .iter()
                .any(|event_type| event_type.starts_with("blackboard."))
        );
    }

    #[test]
    fn bootstrap_order_is_wire_contract() {
        assert_eq!(
            BOOTSTRAP_COMMAND_TYPES,
            &[
                "client.hello",
                "history.list",
                "task.list",
                "gateway.status.get",
                "fork.memory.get",
                "event.subscribe",
            ]
        );
    }
}
