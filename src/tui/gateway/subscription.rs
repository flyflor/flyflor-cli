use serde_json::{Value, json};

pub const SUBSCRIPTION_EVENT_TYPES: &[&str] = &[
    "memory.task_plan.written",
    "memory.task_plan.decided",
    "memory.task_plan.decision.failed",
    "memory.context_fork.written",
    "memory.ask.recorded",
    "memory.ask.answered",
    "memory.ask.chain.capped",
    "memory.ask.mutex.violation",
    "executive.loop.paused",
    "executive.loop.resumed",
    "executive.loop.guard.blocked",
    "route.escalated",
    "memory.recall.started",
    "memory.recall.item",
    "memory.recall.assembled",
    "memory.recall.completed",
    "scope.recall.started",
    "scope.recall.decided",
    "scope.recall.loaded",
    "scope.recall.ask",
    "blackboard.started",
    "blackboard.lease.acquired",
    "blackboard.lease.released",
    "blackboard.turn.start",
    "blackboard.round.started",
    "blackboard.worker.start",
    "blackboard.worker.done",
    "blackboard.worker.end",
    "blackboard.message.appended",
    "blackboard.turn.end",
    "blackboard.decision.requested",
    "blackboard.livelock.detected",
    "blackboard.completed",
    "mcp.tool.call.executed",
    "tool.started",
    "tool.progress",
    "tool.succeeded",
    "tool.failed",
    "tool.output.persisted",
    "tool.ask_required",
    "tool.budget.exhausted",
    "sandbox.tool.approval.requested",
    "sandbox.tool.approval.denied",
    "sandbox.tool.denied",
    "subagent.batch.start",
    "subagent.child.start",
    "subagent.child.end",
    "subagent.batch.end",
    "process.start",
    "process.output",
    "process.output.truncated",
    "process.exit",
    "process.restart.give_up",
    "worker.task.queued",
    "worker.task.start",
    "worker.task.end",
    "worker.task.failed",
];

#[cfg(test)]
pub const BOOTSTRAP_COMMAND_TYPES: &[&str] = &[
    "client.hello",
    "history.list",
    "task.list",
    "capability.catalog.get",
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
        assert!(types.contains(&"blackboard.lease.acquired"));
        assert!(types.contains(&"blackboard.worker.start"));
        assert!(types.contains(&"subagent.child.end"));
        assert!(types.contains(&"tool.output.persisted"));
        assert!(types.contains(&"tool.budget.exhausted"));
        assert!(types.contains(&"memory.ask.chain.capped"));
        assert!(types.contains(&"memory.task_plan.decision.failed"));
        assert!(types.contains(&"memory.recall.assembled"));
        assert!(types.contains(&"process.start"));
        assert!(types.contains(&"worker.task.start"));
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
                "capability.catalog.get",
                "gateway.status.get",
                "fork.memory.get",
                "event.subscribe",
            ]
        );
    }
}
