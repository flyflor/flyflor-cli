use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WsEnvelope {
    pub protocol: String,
    pub id: String,
    #[serde(rename = "type")]
    pub message_type: String,
    pub at: String,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub correlation_id: Option<String>,
    #[serde(default)]
    pub payload: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientHelloPayload {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub capabilities: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayMessageSendPayload {
    pub text: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub chat_id: Option<String>,
    #[serde(default)]
    pub thread_id: Option<String>,
    #[serde(default)]
    pub user: Option<GatewayUserPayload>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayUserPayload {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AckPayload {
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub received: Option<String>,
    #[serde(default)]
    pub subscriptions: Vec<SubscriptionSnapshot>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionSnapshot {
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub types: Option<Vec<String>>,
    #[serde(default)]
    pub classes: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerHelloPayload {
    pub client_id: String,
    pub connected_at: String,
    pub capabilities: Value,
    pub kits: Value,
    pub status: GatewayStatusPayload,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayStatusEnvelopePayload {
    pub status: GatewayStatusPayload,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityCatalogPayload {
    #[serde(default)]
    pub catalog: Option<Value>,
    #[serde(default)]
    pub kits: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GatewayStatusPayload {
    #[serde(default)]
    pub channels: Vec<Value>,
    pub connected_count: usize,
    pub degraded_count: usize,
    pub gateway_running: bool,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub started_at: Option<String>,
    pub streaming_count: usize,
    #[serde(default)]
    pub uptime_ms: Option<u64>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TurnDeltaPayload {
    pub delta: String,
    pub message_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TurnErrorPayload {
    pub message: String,
    pub message_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ControlErrorPayload {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub details: Option<Value>,
    #[serde(default)]
    pub retryable: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TurnFinalPayload {
    pub reply: ReplyPayload,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReplyPayload {
    pub message_id: String,
    pub text: String,
    #[serde(default)]
    pub metadata: Option<ReplyMetadata>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReplyMetadata {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub ask: Option<AskMetadata>,
    #[serde(default)]
    pub planning: Option<PlanningMetadata>,
    #[serde(default)]
    pub executive_tool_loop: Option<LoopMetadata>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AskMetadata {
    pub snapshot_id: String,
    pub reason: String,
    pub prompt: String,
    pub freeform: bool,
    pub choice_count: usize,
    pub question_count: usize,
    #[serde(default)]
    pub choices: Vec<AskChoice>,
    #[serde(default)]
    pub questions: Vec<AskQuestion>,
    #[serde(default)]
    pub executive_tool_loop: Option<LoopMetadata>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AskChoice {
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AskQuestion {
    pub key: String,
    pub prompt: String,
    #[serde(default)]
    pub placeholder: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PlanningMetadata {
    #[serde(default)]
    pub task_plans: Vec<TaskPlan>,
    #[serde(default)]
    pub context_forks: Vec<Value>,
    #[serde(default)]
    pub scenes: Vec<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TaskPlan {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub status: String,
    pub progress: f64,
    pub step_count: usize,
    pub completed_step_count: usize,
    #[serde(default)]
    pub steps: Vec<TaskStep>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TaskStep {
    pub id: String,
    pub title: String,
    pub status: String,
    pub order: usize,
    #[serde(default)]
    pub progress: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LoopMetadata {
    pub ask_id: String,
    pub message: String,
    pub step_count: usize,
    pub stop: String,
    #[serde(default)]
    pub loop_guard_reason: Option<String>,
    #[serde(default)]
    pub tool_budget_exhausted: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EventPublishPayload {
    pub event: RuntimeEvent,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub at: String,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub payload: Option<Value>,
}

pub const WS_PROTOCOL: &str = "flyflor.ws.v1";
pub const EVENT_PROTOCOL: &str = "flyflor.event.v1";
