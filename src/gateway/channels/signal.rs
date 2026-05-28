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

const DEFAULT_SIGNAL_HTTP_URL: &str = "http://127.0.0.1:8080";
const SIGNAL_MAX_TEXT_LENGTH: usize = 8_000;
const SIGNAL_TIMEOUT_MS: u64 = 30_000;

pub struct SignalAdapter {
    http_url: String,
    account: String,
    home_channel: Option<String>,
    allowed_users: HashSet<String>,
    group_allowed: HashSet<String>,
}

impl SignalAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let account = env_first(&[
            "SIGNAL_PHONE_NUMBER",
            "SIGNAL_ACCOUNT",
            "FLYFLOR_SIGNAL_PHONE_NUMBER",
        ]);
        if account.is_empty() {
            return Err(ChannelError::missing_config(
                "SIGNAL_PHONE_NUMBER is required for the signal channel",
            ));
        }
        Ok(Self {
            http_url: env_first(&[
                "SIGNAL_CLI_REST_API",
                "SIGNAL_HTTP_URL",
                "FLYFLOR_SIGNAL_CLI_REST_API",
            ])
            .if_empty(DEFAULT_SIGNAL_HTTP_URL),
            account,
            home_channel: env_optional(&["SIGNAL_HOME_CHANNEL"]),
            allowed_users: env_set_any(&["SIGNAL_ALLOWED_USERS"]),
            group_allowed: env_set_any(&["SIGNAL_GROUP_ALLOWED_USERS"]),
        })
    }

    fn rpc_url(&self) -> String {
        format!("{}/api/v1/rpc", self.http_url)
    }

    fn normalize_envelope(&self, value: &Value) -> Option<NormalizedInboundMessage> {
        let envelope = value.get("envelope").unwrap_or(value);
        let data_message = envelope.get("dataMessage").or_else(|| {
            envelope
                .get("editMessage")
                .and_then(|edit| edit.get("dataMessage"))
        })?;
        let text = value_string_any(data_message, &["message", "text", "body"])?
            .trim()
            .to_string();
        if text.is_empty() {
            return None;
        }
        let sender = value_string_any(envelope, &["sourceNumber", "sourceUuid", "source"])
            .unwrap_or_else(|| "signal-user".to_string());
        if sender == self.account {
            return None;
        }
        if !self.allowed_users.is_empty()
            && !self.allowed_users.contains("*")
            && !self.allowed_users.contains(&sender)
        {
            return None;
        }
        let group_id = data_message
            .get("groupV2")
            .and_then(|group| value_string_any(group, &["id", "groupId"]))
            .or_else(|| {
                data_message
                    .get("groupInfo")
                    .and_then(|group| value_string_any(group, &["groupId", "id"]))
            });
        if let Some(group_id) = group_id.as_ref()
            && !self.group_allowed.is_empty()
            && !self.group_allowed.contains("*")
            && !self.group_allowed.contains(group_id)
        {
            return None;
        }
        let chat_id = group_id
            .as_ref()
            .map(|group_id| format!("group:{group_id}"))
            .unwrap_or_else(|| sender.clone());
        let message_id = value_string_any(envelope, &["timestamp", "serverTimestamp", "id"])
            .unwrap_or_else(|| format!("signal-{}", now_millis()));
        let display_name = value_string_any(envelope, &["sourceName", "name"])
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| sender.clone());
        let chat_type = if group_id.is_some() {
            ChatType::Group
        } else {
            ChatType::Direct
        };
        let route = MessageRoute {
            platform: "signal".to_string(),
            chat_id: chat_id.clone(),
            chat_type,
            user_id: sender.clone(),
            display_name,
            thread_id: chat_id.clone(),
        };
        let metadata = json!({
            "channel": {
                "platform": "signal",
                "adapter": "signal-cli-rest-jsonrpc",
                "chatId": chat_id,
                "chatType": route.chat_type.as_gateway_str(),
                "userId": sender,
                "sourceMessageId": message_id,
                "groupId": group_id
            }
        });
        Some(NormalizedInboundMessage {
            id: format!("signal-{message_id}"),
            text,
            route,
            context: None,
            metadata,
        })
    }
}

impl PlatformAdapter for SignalAdapter {
    fn name(&self) -> &'static str {
        "signal"
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
        let raw = env_first(&["SIGNAL_INBOUND_ENVELOPE", "SIGNAL_INBOUND_EVENT"]);
        if raw.is_empty() {
            return Ok(Vec::new());
        }
        let value = serde_json::from_str::<Value>(&raw).map_err(|error| {
            ChannelError::fatal(format!("signal inbound envelope JSON is invalid: {error}"))
        })?;
        Ok(self.normalize_envelope(&value).into_iter().collect())
    }

    fn send_typing(&self, _route: &MessageRoute) -> ChannelResult<()> {
        Err(ChannelError::unavailable(
            "signal typing is unavailable in the env/jsonrpc text adapter",
        ))
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal("signal message text must not be empty"));
        }
        if outbound_mentions_media(&message.text, message.metadata.as_ref()) {
            return self.send_media_unavailable("outbound");
        }
        let target = if message.route.chat_id.trim().is_empty() {
            self.home_channel.clone().ok_or_else(|| {
                ChannelError::fatal("SIGNAL_HOME_CHANNEL is required when route chat_id is empty")
            })?
        } else {
            message.route.chat_id.clone()
        };
        let mut last_id = None;
        for chunk in split_text_chunks(&message.text, SIGNAL_MAX_TEXT_LENGTH) {
            let mut params = json!({
                "account": self.account,
                "message": chunk
            });
            if let Some(group_id) = target.strip_prefix("group:") {
                params["groupId"] = json!(group_id);
            } else {
                params["recipient"] = json!([target]);
            }
            let response = signal_rpc(&self.rpc_url(), "send", params)?;
            classify_signal_response(&response)?;
            last_id = response
                .get("result")
                .and_then(|result| value_string_any(result, &["timestamp", "id", "messageId"]))
                .or_else(|| Some(format!("signal-{}", now_millis())));
        }
        Ok(PlatformSendOutcome {
            message_id: last_id,
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "signal {media_kind} media delivery is explicitly unavailable in flyflor-cli"
        )))
    }
}

fn signal_rpc(url: &str, method: &str, params: Value) -> ChannelResult<Value> {
    let payload = json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": format!("signal-{}", now_millis())
    });
    let body =
        serde_json::to_string(&payload).map_err(|error| ChannelError::fatal(error.to_string()))?;
    let output = Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            &seconds_arg(SIGNAL_TIMEOUT_MS),
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
        if message.contains("429") || message.to_ascii_lowercase().contains("rate") {
            return Err(ChannelError::rate_limited(format!(
                "signal rate limited: {message}"
            )));
        }
        return Err(ChannelError::retryable(format!(
            "signal curl failed with status {}: {}",
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
            "signal returned invalid JSON: {error}; body={text}"
        ))
    })
}

fn classify_signal_response(value: &Value) -> ChannelResult<()> {
    let Some(error) = value.get("error") else {
        return Ok(());
    };
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("unknown signal error");
    let code = error.get("code").and_then(Value::as_i64).unwrap_or(0);
    if code == 429 || message.contains("[429]") || message.to_ascii_lowercase().contains("rate") {
        return Err(ChannelError::rate_limited(format!(
            "Signal rate limited: code={code} message={message}"
        )));
    }
    if code == 401 || code == 403 || message.to_ascii_lowercase().contains("forbidden") {
        return Err(ChannelError::session_expired(format!(
            "Signal authorization failed: code={code} message={message}"
        )));
    }
    Err(ChannelError::retryable(format!(
        "Signal error: code={code} message={message}"
    )))
}

fn value_string_any(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(|value| {
                value
                    .as_str()
                    .map(ToString::to_string)
                    .or_else(|| value.as_i64().map(|number| number.to_string()))
            })
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

fn env_first(names: &[&str]) -> String {
    names
        .iter()
        .find_map(|name| env::var(name).ok())
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_default()
}

fn env_optional(names: &[&str]) -> Option<String> {
    let value = env_first(names);
    if value.is_empty() { None } else { Some(value) }
}

fn env_set_any(names: &[&str]) -> HashSet<String> {
    names
        .iter()
        .find_map(|name| env::var(name).ok())
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

trait IfEmpty {
    fn if_empty(self, fallback: &str) -> String;
}

impl IfEmpty for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
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
    fn normalizes_signal_direct_envelope() {
        let adapter = test_adapter();
        let message = adapter
            .normalize_envelope(&json!({
                "envelope": {
                    "sourceName": "Signal User",
                    "sourceNumber": "+15550001",
                    "timestamp": 12345,
                    "dataMessage": { "message": "hello signal" }
                }
            }))
            .unwrap();

        assert_eq!(message.id, "signal-12345");
        assert_eq!(message.text, "hello signal");
        assert_eq!(message.route.platform, "signal");
        assert_eq!(message.route.chat_id, "+15550001");
        assert_eq!(message.route.chat_type, ChatType::Direct);
        assert_eq!(message.route.user_id, "+15550001");
        assert_eq!(message.metadata["channel"]["sourceMessageId"], "12345");
    }

    #[test]
    fn normalizes_signal_group_envelope() {
        let adapter = test_adapter();
        let message = adapter
            .normalize_envelope(&json!({
                "sourceNumber": "+15550001",
                "timestamp": 12345,
                "dataMessage": {
                    "message": "hello group",
                    "groupV2": { "id": "group-1" }
                }
            }))
            .unwrap();

        assert_eq!(message.route.chat_id, "group:group-1");
        assert_eq!(message.route.chat_type, ChatType::Group);
        assert_eq!(message.metadata["channel"]["groupId"], "group-1");
    }

    #[test]
    fn allowlist_blocks_unknown_sender() {
        let mut adapter = test_adapter();
        adapter.allowed_users = HashSet::from(["+15550002".to_string()]);

        assert!(
            adapter
                .normalize_envelope(&json!({
                    "sourceNumber": "+15550001",
                    "timestamp": 12345,
                    "dataMessage": { "message": "blocked" }
                }))
                .is_none()
        );
    }

    #[test]
    fn classifies_signal_rpc_errors() {
        assert_eq!(
            classify_signal_response(&json!({
                "error": { "code": 429, "message": "rate limited" }
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::RateLimited
        );
        assert_eq!(
            classify_signal_response(&json!({
                "error": { "code": 403, "message": "forbidden" }
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::SessionExpired
        );
    }

    fn test_adapter() -> SignalAdapter {
        SignalAdapter {
            http_url: DEFAULT_SIGNAL_HTTP_URL.to_string(),
            account: "+15550000".to_string(),
            home_channel: None,
            allowed_users: HashSet::new(),
            group_allowed: HashSet::new(),
        }
    }
}
