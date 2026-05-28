use std::{
    collections::HashSet,
    env,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};

use super::platform::{
    ChannelCapabilityReport, ChannelCapabilityState, ChannelError, ChannelErrorKind, ChannelResult,
    ChatType, MessageRoute, NormalizedInboundMessage, OutboundMessage, PlatformAdapter,
    PlatformSendOutcome,
};

const DEFAULT_NTFY_SERVER_URL: &str = "https://ntfy.sh";
const NTFY_TIMEOUT_MS: u64 = 35_000;
const NTFY_MAX_MESSAGE_LENGTH: usize = 4_096;

pub struct NtfyAdapter {
    topic: String,
    publish_topic: String,
    server_url: String,
    token: Option<String>,
    allowed_users: HashSet<String>,
}

impl NtfyAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let topic = env::var("NTFY_TOPIC")
            .or_else(|_| env::var("FLYFLOR_NTFY_TOPIC"))
            .unwrap_or_default()
            .trim()
            .trim_matches('/')
            .to_string();
        if topic.is_empty() {
            return Err(ChannelError::missing_config(
                "NTFY_TOPIC is required for the ntfy channel",
            ));
        }
        let publish_topic = env::var("NTFY_PUBLISH_TOPIC")
            .unwrap_or_else(|_| topic.clone())
            .trim()
            .trim_matches('/')
            .to_string();
        Ok(Self {
            topic,
            publish_topic,
            server_url: env::var("NTFY_SERVER_URL")
                .or_else(|_| env::var("FLYFLOR_NTFY_SERVER_URL"))
                .unwrap_or_else(|_| DEFAULT_NTFY_SERVER_URL.to_string())
                .trim()
                .trim_end_matches('/')
                .to_string(),
            token: env::var("NTFY_TOKEN")
                .or_else(|_| env::var("FLYFLOR_NTFY_TOKEN"))
                .ok()
                .map(|token| token.trim().to_string())
                .filter(|token| !token.is_empty()),
            allowed_users: env_set("NTFY_ALLOWED_USERS"),
        })
    }

    fn updates_url(&self) -> String {
        format!("{}/{}/json?poll=1", self.server_url, self.topic)
    }

    fn publish_url(&self) -> String {
        format!("{}/{}", self.server_url, self.publish_topic)
    }

    fn normalize_event(&self, value: &Value) -> Option<NormalizedInboundMessage> {
        let event = value
            .get("event")
            .and_then(Value::as_str)
            .unwrap_or("message");
        if event != "message" {
            return None;
        }
        let text = value
            .get("message")
            .or_else(|| value.get("text"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())?
            .to_string();
        let id = value
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("ntfy-{}", now_millis()));
        let user_id = value
            .get("sender")
            .or_else(|| value.get("user"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| self.topic.clone());
        if !self.allowed_users.is_empty() && !self.allowed_users.contains(&user_id) {
            return None;
        }
        let route = MessageRoute {
            platform: "ntfy".to_string(),
            chat_id: self.topic.clone(),
            chat_type: ChatType::Group,
            user_id: user_id.clone(),
            display_name: user_id.clone(),
            thread_id: self.topic.clone(),
        };
        let metadata = json!({
            "channel": {
                "platform": "ntfy",
                "adapter": "ntfy-http",
                "topic": self.topic,
                "chatId": self.topic,
                "chatType": route.chat_type.as_gateway_str(),
                "userId": user_id,
                "sourceMessageId": id,
                "title": value.get("title").and_then(Value::as_str),
                "priority": value.get("priority").and_then(Value::as_i64)
            }
        });
        Some(NormalizedInboundMessage {
            id: format!("ntfy-{id}"),
            text,
            route,
            context: None,
            metadata,
        })
    }
}

impl PlatformAdapter for NtfyAdapter {
    fn name(&self) -> &'static str {
        "ntfy"
    }

    fn capabilities(&self) -> ChannelCapabilityReport {
        ChannelCapabilityReport {
            send: ChannelCapabilityState::Available,
            typing: ChannelCapabilityState::Unavailable,
            edit: ChannelCapabilityState::Unavailable,
            draft: ChannelCapabilityState::Unavailable,
            card: ChannelCapabilityState::Unavailable,
            media: ChannelCapabilityState::Unavailable,
        }
    }

    fn poll_updates(&self) -> ChannelResult<Vec<NormalizedInboundMessage>> {
        let body = ntfy_get(&self.updates_url(), self.token.as_deref())?;
        Ok(parse_ntfy_events(&body)
            .into_iter()
            .filter_map(|value| self.normalize_event(&value))
            .collect())
    }

    fn send_typing(&self, _route: &MessageRoute) -> ChannelResult<()> {
        Err(ChannelError::unavailable(
            "ntfy typing indicator is unavailable",
        ))
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal("ntfy message text must not be empty"));
        }
        if outbound_mentions_media(&message.text, message.metadata.as_ref()) {
            return self.send_media_unavailable("outbound");
        }
        let chunks = split_text_chunks(&message.text, NTFY_MAX_MESSAGE_LENGTH);
        let mut last_id = None;
        for chunk in chunks {
            let response = ntfy_post(&self.publish_url(), self.token.as_deref(), &chunk)?;
            last_id = response
                .as_ref()
                .and_then(|value| value.get("id").and_then(Value::as_str).map(str::to_string))
                .or_else(|| Some(format!("ntfy-{}", now_millis())));
        }
        Ok(PlatformSendOutcome {
            message_id: last_id,
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "ntfy {media_kind} media delivery is explicitly unavailable in flyflor-cli"
        )))
    }
}

fn ntfy_get(url: &str, token: Option<&str>) -> ChannelResult<String> {
    let mut args = vec![
        "-sS".to_string(),
        "--max-time".to_string(),
        seconds_arg(NTFY_TIMEOUT_MS),
        url.to_string(),
    ];
    if let Some(token) = token {
        args.extend(["-H".to_string(), format!("Authorization: Bearer {token}")]);
    }
    run_curl_text(args)
}

fn ntfy_post(url: &str, token: Option<&str>, text: &str) -> ChannelResult<Option<Value>> {
    let mut args = vec![
        "-sS".to_string(),
        "--max-time".to_string(),
        seconds_arg(NTFY_TIMEOUT_MS),
        "-X".to_string(),
        "POST".to_string(),
        url.to_string(),
        "--data-binary".to_string(),
        text.to_string(),
    ];
    if let Some(token) = token {
        args.extend(["-H".to_string(), format!("Authorization: Bearer {token}")]);
    }
    let body = run_curl_text(args)?;
    if body.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str::<Value>(body.trim())
        .map(Some)
        .map_err(|error| ChannelError::retryable(format!("ntfy returned invalid JSON: {error}")))
}

fn run_curl_text(args: Vec<String>) -> ChannelResult<String> {
    let output = Command::new("curl")
        .args(args)
        .output()
        .map_err(|error| ChannelError::unavailable(format!("curl is required: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr.trim();
        if message.contains("429") {
            return Err(ChannelError::rate_limited(format!(
                "ntfy rate limited: {message}"
            )));
        }
        if message.contains("401") || message.contains("403") {
            return Err(ChannelError::session_expired(format!(
                "ntfy authorization failed: {message}"
            )));
        }
        return Err(ChannelError::retryable(format!(
            "ntfy curl failed with status {}: {}",
            output.status, message
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn parse_ntfy_events(body: &str) -> Vec<Value> {
    if let Ok(Value::Array(values)) = serde_json::from_str::<Value>(body.trim()) {
        return values;
    }
    body.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect()
}

fn split_text_chunks(text: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if current.chars().count() >= max_chars {
            chunks.push(current);
            current = String::new();
        }
        current.push(ch);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn outbound_mentions_media(text: &str, metadata: Option<&Value>) -> bool {
    if text.contains("MEDIA:") {
        return true;
    }
    metadata
        .and_then(|value| value.get("media"))
        .is_some_and(|media| !media.is_null())
}

fn env_set(name: &str) -> HashSet<String> {
    env::var(name)
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn seconds_arg(timeout_ms: u64) -> String {
    let seconds = (timeout_ms as f64 / 1000.0).max(1.0);
    format!("{seconds:.3}")
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_jsonl_and_array_events() {
        let jsonl = r#"{"event":"open"}
{"event":"message","id":"m-1","message":"hello"}"#;
        assert_eq!(parse_ntfy_events(jsonl).len(), 2);
        assert_eq!(
            parse_ntfy_events(r#"[{"event":"message","message":"hi"}]"#).len(),
            1
        );
    }

    #[test]
    fn normalizes_message_event_and_filters_non_messages() {
        let adapter = test_adapter();
        assert!(
            adapter
                .normalize_event(&json!({ "event": "open", "id": "open-1" }))
                .is_none()
        );
        let message = adapter
            .normalize_event(&json!({
                "event": "message",
                "id": "m-1",
                "message": "hello",
                "sender": "user-1",
                "title": "Flyflor",
                "priority": 3
            }))
            .unwrap();

        assert_eq!(message.id, "ntfy-m-1");
        assert_eq!(message.text, "hello");
        assert_eq!(message.route.platform, "ntfy");
        assert_eq!(message.route.chat_id, "topic-1");
        assert_eq!(message.route.chat_type, ChatType::Group);
        assert_eq!(
            message
                .metadata
                .get("channel")
                .and_then(|channel| channel.get("sourceMessageId"))
                .and_then(Value::as_str),
            Some("m-1")
        );
    }

    #[test]
    fn allowlist_blocks_unknown_sender() {
        let mut adapter = test_adapter();
        adapter.allowed_users = HashSet::from(["allowed".to_string()]);

        assert!(
            adapter
                .normalize_event(&json!({
                    "event": "message",
                    "id": "m-1",
                    "message": "hello",
                    "sender": "blocked"
                }))
                .is_none()
        );
    }

    #[test]
    fn split_text_preserves_unicode_chunks() {
        assert_eq!(split_text_chunks("你好世界", 2), vec!["你好", "世界"]);
    }

    fn test_adapter() -> NtfyAdapter {
        NtfyAdapter {
            topic: "topic-1".to_string(),
            publish_topic: "topic-1".to_string(),
            server_url: DEFAULT_NTFY_SERVER_URL.to_string(),
            token: None,
            allowed_users: HashSet::new(),
        }
    }
}
