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

const DEFAULT_LINE_API_BASE: &str = "https://api.line.me";
const LINE_MAX_MESSAGE_LENGTH: usize = 4_900;
const LINE_TIMEOUT_MS: u64 = 15_000;

pub struct LineAdapter {
    access_token: String,
    _channel_secret: String,
    api_base: String,
    home_channel: Option<String>,
    allowed_users: HashSet<String>,
}

impl LineAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let access_token = env::var("LINE_CHANNEL_ACCESS_TOKEN")
            .or_else(|_| env::var("FLYFLOR_LINE_CHANNEL_ACCESS_TOKEN"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if access_token.is_empty() {
            return Err(ChannelError::missing_config(
                "LINE_CHANNEL_ACCESS_TOKEN is required for the line channel",
            ));
        }
        let channel_secret = env::var("LINE_CHANNEL_SECRET")
            .or_else(|_| env::var("FLYFLOR_LINE_CHANNEL_SECRET"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if channel_secret.is_empty() {
            return Err(ChannelError::missing_config(
                "LINE_CHANNEL_SECRET is required for the line channel",
            ));
        }
        Ok(Self {
            access_token,
            _channel_secret: channel_secret,
            api_base: env::var("LINE_API_BASE")
                .or_else(|_| env::var("FLYFLOR_LINE_API_BASE"))
                .unwrap_or_else(|_| DEFAULT_LINE_API_BASE.to_string())
                .trim()
                .trim_end_matches('/')
                .to_string(),
            home_channel: env::var("LINE_HOME_CHANNEL")
                .or_else(|_| env::var("FLYFLOR_LINE_HOME_CHANNEL"))
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            allowed_users: env_set("LINE_ALLOWED_USERS"),
        })
    }

    fn reply_url(&self) -> String {
        format!("{}/v2/bot/message/reply", self.api_base)
    }

    fn push_url(&self) -> String {
        format!("{}/v2/bot/message/push", self.api_base)
    }

    fn normalize_webhook(&self, value: &Value) -> Vec<NormalizedInboundMessage> {
        value
            .get("events")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|event| self.normalize_event(event))
            .collect()
    }

    fn normalize_event(&self, event: &Value) -> Option<NormalizedInboundMessage> {
        if event.get("type").and_then(Value::as_str) != Some("message") {
            return None;
        }
        let message = event.get("message")?;
        if message.get("type").and_then(Value::as_str) != Some("text") {
            return None;
        }
        let text = message
            .get("text")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())?
            .to_string();
        let source = event.get("source").unwrap_or(&Value::Null);
        let user_id = value_string(source, "userId")
            .or_else(|| value_string(source, "user_id"))
            .unwrap_or_else(|| "line-user".to_string());
        if !self.allowed_users.is_empty() && !self.allowed_users.contains(&user_id) {
            return None;
        }
        let source_type = source.get("type").and_then(Value::as_str).unwrap_or("user");
        let chat_id = match source_type {
            "group" => value_string(source, "groupId").unwrap_or_else(|| user_id.clone()),
            "room" => value_string(source, "roomId").unwrap_or_else(|| user_id.clone()),
            _ => user_id.clone(),
        };
        let chat_type = match source_type {
            "group" | "room" => ChatType::Group,
            _ => ChatType::Direct,
        };
        let message_id =
            value_string(message, "id").unwrap_or_else(|| format!("line-{}", now_millis()));
        let reply_token = value_string(event, "replyToken");
        let route = MessageRoute {
            platform: "line".to_string(),
            chat_id: chat_id.clone(),
            chat_type,
            user_id: user_id.clone(),
            display_name: user_id.clone(),
            thread_id: chat_id.clone(),
        };
        let metadata = json!({
            "channel": {
                "platform": "line",
                "adapter": "line-messaging-api",
                "chatId": chat_id,
                "chatType": route.chat_type.as_gateway_str(),
                "userId": user_id,
                "sourceMessageId": message_id,
                "replyToken": reply_token,
                "sourceType": source_type
            }
        });
        Some(NormalizedInboundMessage {
            id: format!("line-{message_id}"),
            text,
            route,
            context: None,
            metadata,
        })
    }
}

impl PlatformAdapter for LineAdapter {
    fn name(&self) -> &'static str {
        "line"
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
        let raw = env::var("LINE_INBOUND_WEBHOOK")
            .or_else(|_| env::var("FLYFLOR_LINE_INBOUND_WEBHOOK"))
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let Some(raw) = raw else {
            return Ok(Vec::new());
        };
        let value = serde_json::from_str::<Value>(&raw).map_err(|error| {
            ChannelError::fatal(format!("line webhook JSON parse failed: {error}"))
        })?;
        Ok(self.normalize_webhook(&value))
    }

    fn send_typing(&self, _route: &MessageRoute) -> ChannelResult<()> {
        Err(ChannelError::unavailable(
            "line typing indicator is unavailable",
        ))
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal("line message text must not be empty"));
        }
        if outbound_mentions_media(&message.text, message.metadata.as_ref()) {
            return self.send_media_unavailable("outbound");
        }
        let chunks = split_text_chunks(&message.text, LINE_MAX_MESSAGE_LENGTH);
        let reply_token = message
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("channel"))
            .and_then(|channel| channel.get("replyToken"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let response = if let Some(reply_token) = reply_token.filter(|token| !token.is_empty()) {
            line_post(
                &self.reply_url(),
                &self.access_token,
                json!({
                    "replyToken": reply_token,
                    "messages": chunks.iter().map(|text| json!({
                        "type": "text",
                        "text": text
                    })).collect::<Vec<_>>()
                }),
            )?
        } else {
            let to = if message.route.chat_id.trim().is_empty() {
                self.home_channel.clone().ok_or_else(|| {
                    ChannelError::fatal("LINE_HOME_CHANNEL is required when route chat_id is empty")
                })?
            } else {
                message.route.chat_id.clone()
            };
            line_post(
                &self.push_url(),
                &self.access_token,
                json!({
                    "to": to,
                    "messages": chunks.iter().map(|text| json!({
                        "type": "text",
                        "text": text
                    })).collect::<Vec<_>>()
                }),
            )?
        };
        classify_line_response(&response)?;
        Ok(PlatformSendOutcome {
            message_id: value_string(&response, "messageId")
                .or_else(|| value_string(&response, "id"))
                .or_else(|| Some(format!("line-{}", now_millis()))),
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "line {media_kind} media delivery is explicitly unavailable in flyflor-cli"
        )))
    }
}

fn line_post(url: &str, access_token: &str, payload: Value) -> ChannelResult<Value> {
    let body =
        serde_json::to_string(&payload).map_err(|error| ChannelError::fatal(error.to_string()))?;
    let output = Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            &seconds_arg(LINE_TIMEOUT_MS),
            "-X",
            "POST",
            url,
            "-H",
            &format!("Authorization: Bearer {access_token}"),
            "-H",
            "Content-Type: application/json",
            "--data",
            &body,
        ])
        .output()
        .map_err(|error| ChannelError::unavailable(format!("curl is required: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr.trim();
        if message.contains("401") || message.contains("403") {
            return Err(ChannelError::session_expired(format!(
                "line authorization failed: {message}"
            )));
        }
        if message.contains("429") {
            return Err(ChannelError::rate_limited(format!(
                "line rate limited: {message}"
            )));
        }
        return Err(ChannelError::retryable(format!(
            "line curl failed with status {}: {}",
            output.status, message
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let text = stdout.trim();
    if text.is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str::<Value>(text).map_err(|error| {
        ChannelError::retryable(format!("line returned invalid JSON: {error}; body={text}"))
    })
}

fn classify_line_response(value: &Value) -> ChannelResult<()> {
    let status = value
        .get("status_code")
        .or_else(|| value.get("statusCode"))
        .and_then(Value::as_i64);
    if status.is_none() || status.is_some_and(|status| status < 400) {
        return Ok(());
    }
    let status = status.unwrap_or_default();
    let message = value
        .get("message")
        .or_else(|| value.get("error"))
        .and_then(Value::as_str)
        .unwrap_or("unknown line error");
    match status {
        401 | 403 => Err(ChannelError::session_expired(format!(
            "LINE authorization failed: status={status} message={message}"
        ))),
        429 => Err(ChannelError::rate_limited(format!(
            "LINE rate limited: status={status} message={message}"
        ))),
        400 | 404 => Err(ChannelError::fatal(format!(
            "LINE bad request: status={status} message={message}"
        ))),
        _ => Err(ChannelError::retryable(format!(
            "LINE error: status={status} message={message}"
        ))),
    }
}

fn value_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
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
    fn normalizes_line_text_webhook_event() {
        let adapter = test_adapter();
        let messages = adapter.normalize_webhook(&json!({
            "events": [{
                "type": "message",
                "replyToken": "reply-token",
                "source": { "type": "user", "userId": "user-1" },
                "message": { "type": "text", "id": "line-message-1", "text": "hello line" }
            }]
        }));

        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert_eq!(message.id, "line-line-message-1");
        assert_eq!(message.text, "hello line");
        assert_eq!(message.route.platform, "line");
        assert_eq!(message.route.chat_id, "user-1");
        assert_eq!(message.route.chat_type, ChatType::Direct);
        assert_eq!(message.metadata["channel"]["replyToken"], "reply-token");
    }

    #[test]
    fn normalizes_group_chat_route() {
        let adapter = test_adapter();
        let message = adapter
            .normalize_event(&json!({
                "type": "message",
                "source": { "type": "group", "groupId": "group-1", "userId": "user-1" },
                "message": { "type": "text", "id": "m-1", "text": "group hello" }
            }))
            .unwrap();

        assert_eq!(message.route.chat_id, "group-1");
        assert_eq!(message.route.chat_type, ChatType::Group);
        assert_eq!(message.route.user_id, "user-1");
    }

    #[test]
    fn allowlist_blocks_unknown_user() {
        let mut adapter = test_adapter();
        adapter.allowed_users = HashSet::from(["allowed".to_string()]);

        assert!(
            adapter
                .normalize_event(&json!({
                    "type": "message",
                    "source": { "type": "user", "userId": "blocked" },
                    "message": { "type": "text", "id": "m-1", "text": "blocked" }
                }))
                .is_none()
        );
    }

    #[test]
    fn filters_non_text_or_non_message_events() {
        let adapter = test_adapter();
        assert!(
            adapter
                .normalize_event(&json!({
                    "type": "follow",
                    "source": { "type": "user", "userId": "user-1" }
                }))
                .is_none()
        );
        assert!(
            adapter
                .normalize_event(&json!({
                    "type": "message",
                    "source": { "type": "user", "userId": "user-1" },
                    "message": { "type": "image", "id": "image-1" }
                }))
                .is_none()
        );
    }

    #[test]
    fn classifies_line_error_codes() {
        assert_eq!(
            classify_line_response(&json!({
                "status_code": 401,
                "message": "unauthorized"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::SessionExpired
        );
        assert_eq!(
            classify_line_response(&json!({
                "status_code": 429,
                "message": "slow down"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::RateLimited
        );
        assert_eq!(
            classify_line_response(&json!({
                "status_code": 400,
                "message": "bad"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::Fatal
        );
    }

    #[test]
    fn split_text_preserves_unicode_chunks() {
        assert_eq!(split_text_chunks("你好世界", 2), vec!["你好", "世界"]);
    }

    fn test_adapter() -> LineAdapter {
        LineAdapter {
            access_token: "token".to_string(),
            _channel_secret: "secret".to_string(),
            api_base: DEFAULT_LINE_API_BASE.to_string(),
            home_channel: None,
            allowed_users: HashSet::new(),
        }
    }
}
