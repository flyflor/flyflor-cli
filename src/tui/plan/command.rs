use serde_json::{Value, json};

use super::state::PlanAction;

pub fn plan_decide_payload(plan_id: &str, action: PlanAction, revision: Option<&str>) -> Value {
    let mut payload = json!({
        "planId": plan_id,
        "action": action.as_str()
    });
    if let Some(revision) = revision {
        payload["revision"] = json!(revision);
    }
    payload
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_plan_decide_payload() {
        let payload = plan_decide_payload("plan-1", PlanAction::Revise, Some("补充边界"));

        assert_eq!(
            payload.get("planId").and_then(Value::as_str),
            Some("plan-1")
        );
        assert_eq!(
            payload.get("action").and_then(Value::as_str),
            Some("revise")
        );
        assert_eq!(
            payload.get("revision").and_then(Value::as_str),
            Some("补充边界")
        );
    }
}
