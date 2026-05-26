use serde_json::{Map, Value, json};

use crate::tui::plan::{command::plan_decide_payload, state::PlanAction};

use super::{
    envelope::{EnvelopeFactory, GatewayEnvelope},
    subscription,
};

#[derive(Clone, Debug)]
pub struct GatewayCommandBuilder {
    factory: EnvelopeFactory,
}

impl GatewayCommandBuilder {
    pub fn new(factory: EnvelopeFactory) -> Self {
        Self { factory }
    }

    pub fn client_hello(&self, sequence: u64, version: &str) -> GatewayEnvelope {
        self.factory.build(
            "client.hello",
            sequence,
            json!({
                "clientId": "flyflor-cli",
                "name": "flyflor-cli",
                "version": version,
                "capabilities": { "ui": "ratatui" }
            }),
        )
    }

    pub fn history_list_with_before(
        &self,
        sequence: u64,
        limit: u64,
        context_fork_id: Option<&str>,
        before_ts: Option<u64>,
    ) -> GatewayEnvelope {
        let mut payload = json!({ "limit": limit });
        if let Some(before_ts) = before_ts {
            payload["beforeTs"] = json!(before_ts);
        }
        if let Some(context_fork_id) = context_fork_id {
            payload["contextForkId"] = json!(context_fork_id);
        }
        self.factory.build("history.list", sequence, payload)
    }

    pub fn task_list(&self, sequence: u64) -> GatewayEnvelope {
        self.factory.build("task.list", sequence, json!({}))
    }

    pub fn gateway_status_get(&self, sequence: u64) -> GatewayEnvelope {
        self.factory
            .build("gateway.status.get", sequence, json!({}))
    }

    pub fn fork_memory_get(&self, sequence: u64, limit: u64) -> GatewayEnvelope {
        self.factory
            .build("fork.memory.get", sequence, json!({ "limit": limit }))
    }

    pub fn event_subscribe(&self, sequence: u64) -> GatewayEnvelope {
        self.factory.build(
            "event.subscribe",
            sequence,
            subscription::subscription_payload(),
        )
    }

    pub fn gateway_message_send(
        &self,
        sequence: u64,
        payload: GatewayMessagePayload,
    ) -> GatewayEnvelope {
        self.factory.build_with_request(
            "gateway.message.send",
            sequence,
            Some(format!("flyflor-cli-turn-{sequence}")),
            payload.into_value(),
        )
    }

    pub fn task_plan_decide(
        &self,
        sequence: u64,
        plan_id: &str,
        action: PlanAction,
        revision: Option<&str>,
    ) -> GatewayEnvelope {
        let payload = plan_decide_payload(plan_id, action, revision);
        self.factory.build("task.plan.decide", sequence, payload)
    }

    pub fn fork_create(&self, request_id: &str, sequence: u64, payload: Value) -> GatewayEnvelope {
        self.factory.build_with_request(
            "fork.create",
            sequence,
            Some(request_id.to_string()),
            payload,
        )
    }

    pub fn execution_job_detail_get(&self, sequence: u64, job_id: &str) -> GatewayEnvelope {
        self.factory.build(
            "execution.job.detail.get",
            sequence,
            json!({ "jobId": job_id }),
        )
    }
}

#[derive(Clone, Debug)]
pub struct GatewayMessagePayload {
    message_id: String,
    text: String,
    context_fork_id: Option<String>,
    metadata: Option<Value>,
    mode: &'static str,
    yolo: bool,
    conversation_key: String,
    thread_id: String,
    user_id: String,
    display_name: String,
}

impl GatewayMessagePayload {
    pub fn new(message_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            message_id: message_id.into(),
            text: text.into(),
            context_fork_id: None,
            metadata: None,
            mode: "act",
            yolo: false,
            conversation_key: "flyflor-cli".to_string(),
            thread_id: "flyflor-cli".to_string(),
            user_id: "flyflor-cli-user".to_string(),
            display_name: "Flyflor CLI User".to_string(),
        }
    }

    pub fn context_fork_id(mut self, context_fork_id: impl Into<String>) -> Self {
        self.context_fork_id = Some(context_fork_id.into());
        self
    }

    pub fn metadata(mut self, metadata: Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn mode(mut self, mode: &'static str, yolo: bool) -> Self {
        self.mode = mode;
        self.yolo = yolo;
        self
    }

    pub fn identity(
        mut self,
        conversation_key: impl Into<String>,
        thread_id: impl Into<String>,
        user_id: impl Into<String>,
        display_name: impl Into<String>,
    ) -> Self {
        self.conversation_key = conversation_key.into();
        self.thread_id = thread_id.into();
        self.user_id = user_id.into();
        self.display_name = display_name.into();
        self
    }

    fn into_value(self) -> Value {
        let mut payload = json!({
            "id": self.message_id,
            "text": self.text,
            "conversationKey": self.conversation_key,
            "chatType": "direct",
            "threadId": self.thread_id,
            "user": {
                "id": self.user_id,
                "displayName": self.display_name
            }
        });
        if let Some(context_fork_id) = self.context_fork_id {
            payload["context"] = json!({ "contextForkId": context_fork_id });
        }
        if let Some(metadata) = self.metadata {
            payload["metadata"] = metadata;
        }
        apply_message_mode(&mut payload, self.mode, self.yolo);
        payload
    }
}

fn apply_message_mode(payload: &mut Value, mode: &'static str, yolo: bool) {
    if !payload.get("metadata").is_some_and(Value::is_object) {
        payload["metadata"] = json!({});
    }
    if let Some(metadata) = payload.get_mut("metadata").and_then(Value::as_object_mut) {
        let tui = metadata
            .entry("tui".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(tui) = tui.as_object_mut() {
            tui.insert("mode".to_string(), json!(mode));
            tui.insert("yolo".to_string(), json!(yolo));
        }
        metadata.insert(
            "interaction".to_string(),
            json!({
                "source": "flyflor-cli",
                "mode": mode,
                "yolo": yolo
            }),
        );
        metadata.insert(
            "uiMode".to_string(),
            json!({
                "source": "flyflor-cli",
                "mode": mode,
                "yolo": yolo
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builder() -> GatewayCommandBuilder {
        GatewayCommandBuilder::new(EnvelopeFactory::new("flyflor-cli"))
    }

    fn payload_string<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
        value
            .get("payload")
            .and_then(|payload| payload.get(field))
            .and_then(Value::as_str)
    }

    #[test]
    fn builds_subscription_envelope_from_fixed_list() {
        let envelope = builder().event_subscribe(42).into_value();
        let types = envelope
            .get("payload")
            .and_then(|payload| payload.get("types"))
            .and_then(Value::as_array)
            .expect("types array")
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();

        assert_eq!(
            envelope.get("type").and_then(Value::as_str),
            Some("event.subscribe")
        );
        assert_eq!(types, subscription::SUBSCRIPTION_EVENT_TYPES);
        assert!(types.contains(&"executive.loop.paused"));
        assert!(types.contains(&"subagent.batch.start"));
        assert!(
            envelope
                .get("payload")
                .and_then(|payload| payload.get("classes"))
                .is_none()
        );
    }

    #[test]
    fn builds_snapshot_commands() {
        let commands = vec![
            builder()
                .history_list_with_before(1, 20, Some("fork-1"), None)
                .into_value(),
            builder().task_list(2).into_value(),
            builder().gateway_status_get(3).into_value(),
            builder().fork_memory_get(4, 5).into_value(),
            builder().execution_job_detail_get(5, "job-1").into_value(),
        ];

        let types = commands
            .iter()
            .filter_map(|command| command.get("type").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(
            types,
            vec![
                "history.list",
                "task.list",
                "gateway.status.get",
                "fork.memory.get",
                "execution.job.detail.get",
            ]
        );
        assert_eq!(
            payload_string(&commands[0], "contextForkId"),
            Some("fork-1")
        );
        assert_eq!(
            commands[4]
                .get("payload")
                .and_then(|payload| payload.get("jobId"))
                .and_then(Value::as_str),
            Some("job-1")
        );
    }

    #[test]
    fn builds_message_plan_and_fork_commands() {
        let message = builder()
            .gateway_message_send(
                10,
                GatewayMessagePayload::new("message-1", "continue")
                    .context_fork_id("fork-1")
                    .metadata(json!({ "continuation": { "snapshotId": "ask-1" } }))
                    .mode("plan", true)
                    .identity("conversation-1", "thread-1", "user-1", "User One"),
            )
            .into_value();
        assert_eq!(
            message.get("type").and_then(Value::as_str),
            Some("gateway.message.send")
        );
        assert_eq!(payload_string(&message, "text"), Some("continue"));
        assert_eq!(
            message
                .get("payload")
                .and_then(|payload| payload.get("context"))
                .and_then(|context| context.get("contextForkId"))
                .and_then(Value::as_str),
            Some("fork-1")
        );
        assert_eq!(
            message
                .get("payload")
                .and_then(|payload| payload.get("metadata"))
                .and_then(|metadata| metadata.get("tui"))
                .and_then(|tui| tui.get("mode"))
                .and_then(Value::as_str),
            Some("plan")
        );
        assert_eq!(
            payload_string(&message, "conversationKey"),
            Some("conversation-1")
        );
        assert_eq!(payload_string(&message, "threadId"), Some("thread-1"));
        assert_eq!(
            message
                .get("payload")
                .and_then(|payload| payload.get("user"))
                .and_then(|user| user.get("displayName"))
                .and_then(Value::as_str),
            Some("User One")
        );

        let plan = builder()
            .task_plan_decide(11, "plan-1", PlanAction::Revise, Some("more detail"))
            .into_value();
        assert_eq!(
            plan.get("type").and_then(Value::as_str),
            Some("task.plan.decide")
        );
        assert_eq!(payload_string(&plan, "planId"), Some("plan-1"));
        assert_eq!(payload_string(&plan, "action"), Some("revise"));
        assert_eq!(payload_string(&plan, "revision"), Some("more detail"));

        let fork = builder()
            .fork_create("fork-request-1", 12, json!({ "sourceEventId": "event-1" }))
            .into_value();
        assert_eq!(
            fork.get("type").and_then(Value::as_str),
            Some("fork.create")
        );
        assert_eq!(
            fork.get("requestId").and_then(Value::as_str),
            Some("fork-request-1")
        );
        assert_eq!(payload_string(&fork, "sourceEventId"), Some("event-1"));
    }

    #[test]
    fn plan_decision_values_are_wire_actions() {
        assert_eq!(PlanAction::Confirm.as_str(), "confirm");
        assert_eq!(PlanAction::Revise.as_str(), "revise");
        assert_eq!(PlanAction::Abandon.as_str(), "abandon");
    }
}
