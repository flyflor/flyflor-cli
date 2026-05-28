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

const BLUEBUBBLES_MAX_MESSAGE_LENGTH: usize = 4_000;
const BLUEBUBBLES_TIMEOUT_MS: u64 = 15_000;

pub struct BlueBubblesAdapter {
    server_url: String,
    password: String,
    home_chat_guid: Option<String>,
    allowed_users: HashSet<String>,
    preferred_method: String,
}

impl BlueBubblesAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let server_url = env::var("BLUEBUBBLES_SERVER_URL")
            .or_else(|_| env::var("FLYFLOR_BLUEBUBBLES_SERVER_URL"))
            .unwrap_or_default()
            .trim()
            .trim_end_matches('/')
            .to_string();
        if server_url.is_empty() {
            return Err(ChannelError::missing_config(
                "BLUEBUBBLES_SERVER_URL is required for the bluebubbles channel",
            ));
        }
        let password = env::var("BLUEBUBBLES_PASSWORD")
            .or_else(|_| env::var("FLYFLOR_BLUEBUBBLES_PASSWORD"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if password.is_empty() {
            return Err(ChannelError::missing_config(
                "BLUEBUBBLES_PASSWORD is required for the bluebubbles channel",
            ));
        }
        Ok(Self {
            server_url,
            password,
            home_chat_guid: env::var("BLUEBUBBLES_HOME_CHAT_GUID")
                .or_else(|_| env::var("FLYFLOR_BLUEBUBBLES_HOME_CHAT_GUID"))
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            allowed_users: env_set("BLUEBUBBLES_ALLOWED_USERS"),
            preferred_method: env::var("BLUEBUBBLES_SEND_METHOD")
                .or_else(|_| env::var("FLYFLOR_BLUEBUBBLES_SEND_METHOD"))
                .unwrap_or_else(|_| "apple-script".to_string())
                .trim()
                .to_string(),
        })
    }

    fn send_text_url(&self) -> String {
        format!(
            "{}/api/v1/message/text?password={}",
            self.server_url,
            url_encode_query(&self.password)
        )
    }

    fn normalize_webhook(&self, value: &Value) -> Vec<NormalizedInboundMessage> {
        value
            .get("messages")
            .and_then(Value::as_array)
            .or_else(|| value.get("data").and_then(Value::as_array))
            .into_iter()
            .flatten()
            .filter_map(|message| self.normalize_message(message))
            .collect()
    }

    fn normalize_message(&self, message: &Value) -> Option<NormalizedInboundMessage> {
        let text = value_string_any(message, &["text", "message", "body"])
            .or_else(|| value_string_at(message, &["message", "text"]))
            .or_else(|| value_string_at(message, &["data", "text"]))?
            .trim()
            .to_string();
        if text.is_empty() {
            return None;
        }
        let user_id = value_string_any(message, &["handle", "handleAddress", "address", "sender"])
            .or_else(|| value_string_at(message, &["handle", "address"]))
            .or_else(|| value_string_at(message, &["sender", "address"]))
            .unwrap_or_else(|| "bluebubbles-user".to_string());
        if !self.allowed_users.is_empty() && !self.allowed_users.contains(&user_id) {
            return None;
        }
        let chat_guid = value_string_any(message, &["chatGuid", "chat_guid", "chatId", "chat_id"])
            .or_else(|| value_string_at(message, &["chat", "guid"]))
            .or_else(|| value_string_at(message, &["chat", "id"]))
            .unwrap_or_else(|| user_id.clone());
        let message_guid = value_string_any(message, &["guid", "id", "messageGuid", "message_id"])
            .or_else(|| value_string_at(message, &["message", "guid"]))
            .unwrap_or_else(|| format!("bluebubbles-{}", now_millis()));
        let display_name = value_string_any(message, &["displayName", "display_name", "name"])
            .or_else(|| value_string_at(message, &["handle", "displayName"]))
            .unwrap_or_else(|| user_id.clone());
        let is_group = value_bool_any(message, &["isGroup", "is_group", "group"])
            .or_else(|| value_bool_at(message, &["chat", "isGroup"]))
            .unwrap_or(false);
        let route = MessageRoute {
            platform: "bluebubbles".to_string(),
            chat_id: chat_guid.clone(),
            chat_type: if is_group {
                ChatType::Group
            } else {
                ChatType::Direct
            },
            user_id: user_id.clone(),
            display_name,
            thread_id: chat_guid.clone(),
        };
        let metadata = json!({
            "channel": {
                "platform": "bluebubbles",
                "adapter": "bluebubbles-rest",
                "chatId": chat_guid,
                "chatType": route.chat_type.as_gateway_str(),
                "userId": user_id,
                "sourceMessageId": message_guid,
                "sendMethod": self.preferred_method
            },
            "bluebubbles": {
                "chatGuid": route.chat_id,
                "messageGuid": message_guid,
                "isGroup": is_group
            }
        });
        Some(NormalizedInboundMessage {
            id: format!("bluebubbles-{message_guid}"),
            text,
            route,
            context: None,
            metadata,
        })
    }
}

impl PlatformAdapter for BlueBubblesAdapter {
    fn name(&self) -> &'static str {
        "bluebubbles"
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
        let raw = env::var("BLUEBUBBLES_INBOUND_WEBHOOK")
            .or_else(|_| env::var("FLYFLOR_BLUEBUBBLES_INBOUND_WEBHOOK"))
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let Some(raw) = raw else {
            return Ok(Vec::new());
        };
        let value = serde_json::from_str::<Value>(&raw).map_err(|error| {
            ChannelError::fatal(format!("bluebubbles webhook JSON parse failed: {error}"))
        })?;
        Ok(self.normalize_webhook(&value))
    }

    fn send_typing(&self, _route: &MessageRoute) -> ChannelResult<()> {
        Err(ChannelError::unavailable(
            "bluebubbles typing indicator is unavailable in flyflor-cli",
        ))
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal(
                "bluebubbles message text must not be empty",
            ));
        }
        if outbound_mentions_media(&message.text, message.metadata.as_ref()) {
            return self.send_media_unavailable("outbound");
        }
        let chat_guid = if message.route.chat_id.trim().is_empty() {
            self.home_chat_guid.clone().ok_or_else(|| {
                ChannelError::fatal(
                    "BLUEBUBBLES_HOME_CHAT_GUID is required when route chat_id is empty",
                )
            })?
        } else {
            message.route.chat_id.clone()
        };
        let mut last_id = None;
        for chunk in split_text_chunks(&message.text, BLUEBUBBLES_MAX_MESSAGE_LENGTH) {
            let temp_guid = format!("flyflor-bluebubbles-{}", now_millis());
            let response = bluebubbles_post(
                &self.send_text_url(),
                json!({
                    "chatGuid": chat_guid,
                    "tempGuid": temp_guid,
                    "message": chunk,
                    "method": self.preferred_method,
                    "selectedMessageGuid": message.reply_to_message_id
                }),
            )?;
            classify_bluebubbles_response(&response)?;
            last_id = value_string(&response, "guid")
                .or_else(|| value_string(&response, "id"))
                .or_else(|| value_string_at(&response, &["data", "guid"]))
                .or_else(|| Some(temp_guid));
        }
        Ok(PlatformSendOutcome {
            message_id: last_id,
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "bluebubbles {media_kind} media delivery is explicitly unavailable in flyflor-cli"
        )))
    }
}

fn bluebubbles_post(url: &str, payload: Value) -> ChannelResult<Value> {
    let body =
        serde_json::to_string(&payload).map_err(|error| ChannelError::fatal(error.to_string()))?;
    let output = Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            &seconds_arg(BLUEBUBBLES_TIMEOUT_MS),
            "-X",
            "POST",
            url,
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
                "bluebubbles authorization failed: {message}"
            )));
        }
        if message.contains("429") {
            return Err(ChannelError::rate_limited(format!(
                "bluebubbles rate limited: {message}"
            )));
        }
        return Err(ChannelError::retryable(format!(
            "bluebubbles curl failed with status {}: {}",
            output.status, message
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let text = stdout.trim();
    if text.is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str::<Value>(text).map_err(|error| {
        ChannelError::retryable(format!(
            "bluebubbles returned invalid JSON: {error}; body={text}"
        ))
    })
}

fn classify_bluebubbles_response(value: &Value) -> ChannelResult<()> {
    let status = value
        .get("status")
        .or_else(|| value.get("status_code"))
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
        .unwrap_or("unknown bluebubbles error");
    match status {
        401 | 403 => Err(ChannelError::session_expired(format!(
            "BlueBubbles authorization failed: status={status} message={message}"
        ))),
        429 => Err(ChannelError::rate_limited(format!(
            "BlueBubbles rate limited: status={status} message={message}"
        ))),
        400 | 404 => Err(ChannelError::fatal(format!(
            "BlueBubbles bad request: status={status} message={message}"
        ))),
        _ => Err(ChannelError::retryable(format!(
            "BlueBubbles error: status={status} message={message}"
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

fn value_string_any(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| value_string(value, key))
}

fn value_string_at(value: &Value, path: &[&str]) -> Option<String> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    cursor
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn value_bool_any(value: &Value, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_bool))
}

fn value_bool_at(value: &Value, path: &[&str]) -> Option<bool> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    cursor.as_bool()
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

fn url_encode_query(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => {
                let hex = format!("%{byte:02X}");
                hex.chars().collect::<Vec<_>>()
            }
        })
        .collect()
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
    fn normalizes_bluebubbles_webhook_payload() {
        let adapter = test_adapter();
        let messages = adapter.normalize_webhook(&json!({
            "messages": [{
                "guid": "message-1",
                "text": "hello bluebubbles",
                "handle": { "address": "+15550001", "displayName": "User One" },
                "chat": { "guid": "chat-1", "isGroup": true }
            }]
        }));

        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert_eq!(message.id, "bluebubbles-message-1");
        assert_eq!(message.text, "hello bluebubbles");
        assert_eq!(message.route.platform, "bluebubbles");
        assert_eq!(message.route.chat_id, "chat-1");
        assert_eq!(message.route.chat_type, ChatType::Group);
        assert_eq!(message.route.user_id, "+15550001");
        assert_eq!(
            message.metadata["channel"]["sourceMessageId"],
            Value::String("message-1".to_string())
        );
    }

    #[test]
    fn supports_data_array_payload_shape() {
        let adapter = test_adapter();
        let messages = adapter.normalize_webhook(&json!({
            "data": [{
                "id": "message-2",
                "body": "nested hello",
                "sender": { "address": "user-2" },
                "chat_guid": "chat-2"
            }]
        }));

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id, "bluebubbles-message-2");
        assert_eq!(messages[0].route.user_id, "user-2");
        assert_eq!(messages[0].route.chat_id, "chat-2");
    }

    #[test]
    fn allowlist_blocks_unknown_user() {
        let mut adapter = test_adapter();
        adapter.allowed_users = HashSet::from(["allowed".to_string()]);

        assert!(
            adapter
                .normalize_message(&json!({
                    "guid": "message-1",
                    "text": "blocked",
                    "handle": { "address": "blocked" }
                }))
                .is_none()
        );
    }

    #[test]
    fn classifies_bluebubbles_error_codes() {
        assert_eq!(
            classify_bluebubbles_response(&json!({
                "status": 401,
                "message": "unauthorized"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::SessionExpired
        );
        assert_eq!(
            classify_bluebubbles_response(&json!({
                "status_code": 429,
                "message": "slow down"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::RateLimited
        );
        assert_eq!(
            classify_bluebubbles_response(&json!({
                "statusCode": 400,
                "message": "bad"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::Fatal
        );
    }

    #[test]
    fn split_text_and_query_encoding_preserve_unicode() {
        assert_eq!(split_text_chunks("你好世界", 2), vec!["你好", "世界"]);
        assert_eq!(url_encode_query("p@ss word"), "p%40ss%20word");
    }

    fn test_adapter() -> BlueBubblesAdapter {
        BlueBubblesAdapter {
            server_url: "http://127.0.0.1:1234".to_string(),
            password: "password".to_string(),
            home_chat_guid: None,
            allowed_users: HashSet::new(),
            preferred_method: "apple-script".to_string(),
        }
    }
}
