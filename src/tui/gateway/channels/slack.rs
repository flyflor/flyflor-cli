use std::{
    collections::HashSet,
    env,
    process::Command,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};

use super::platform::{
    ChannelCapabilityReport, ChannelCapabilityState, ChannelError, ChannelErrorKind, ChannelResult,
    ChatType, MessageRoute, NormalizedInboundMessage, OutboundMessage, PlatformAdapter,
    PlatformSendOutcome,
};

const DEFAULT_SLACK_API_BASE: &str = "https://slack.com";
const SLACK_MAX_MESSAGE_LENGTH: usize = 3_900;
const SLACK_TIMEOUT_MS: u64 = 15_000;

pub struct SlackAdapter {
    api_base: String,
    bot_token: String,
    channel_id: String,
    bot_user_id: Option<String>,
    allowed_users: HashSet<String>,
    reply_in_thread: bool,
    last_ts: Mutex<Option<String>>,
}

impl SlackAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let bot_token = env::var("SLACK_BOT_TOKEN")
            .or_else(|_| env::var("FLYFLOR_SLACK_BOT_TOKEN"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if bot_token.is_empty() {
            return Err(ChannelError::missing_config(
                "SLACK_BOT_TOKEN is required for the slack channel",
            ));
        }
        let channel_id = env::var("SLACK_HOME_CHANNEL")
            .or_else(|_| env::var("SLACK_CHANNEL_ID"))
            .or_else(|_| env::var("FLYFLOR_SLACK_HOME_CHANNEL"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if channel_id.is_empty() {
            return Err(ChannelError::missing_config(
                "SLACK_HOME_CHANNEL is required for the slack channel",
            ));
        }
        Ok(Self {
            api_base: env::var("SLACK_API_BASE")
                .or_else(|_| env::var("FLYFLOR_SLACK_API_BASE"))
                .unwrap_or_else(|_| DEFAULT_SLACK_API_BASE.to_string())
                .trim()
                .trim_end_matches('/')
                .to_string(),
            bot_token,
            channel_id,
            bot_user_id: env::var("SLACK_BOT_USER_ID")
                .or_else(|_| env::var("FLYFLOR_SLACK_BOT_USER_ID"))
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            allowed_users: env_set("SLACK_ALLOWED_USERS"),
            reply_in_thread: env_bool("SLACK_REPLY_IN_THREAD", true),
            last_ts: Mutex::new(
                env::var("SLACK_SINCE_TS")
                    .ok()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty()),
            ),
        })
    }

    fn history_url(&self) -> String {
        format!(
            "{}/api/conversations.history?channel={}&limit=20",
            self.api_base,
            url_encode_component(&self.channel_id)
        )
    }

    fn post_message_url(&self) -> String {
        format!("{}/api/chat.postMessage", self.api_base)
    }

    fn normalize_message(&self, value: &Value) -> Option<NormalizedInboundMessage> {
        if value_string(value, "type").as_deref() != Some("message") {
            return None;
        }
        if value_string(value, "subtype").is_some() {
            return None;
        }
        let ts = value_string(value, "ts")?;
        let user_id = value_string(value, "user")?;
        if self.bot_user_id.as_deref() == Some(user_id.as_str()) {
            return None;
        }
        if value_string(value, "bot_id").is_some() {
            return None;
        }
        if !self.allowed_users.is_empty() && !self.allowed_users.contains(&user_id) {
            return None;
        }
        let text = value_string(value, "text")?.trim().to_string();
        if text.is_empty() {
            return None;
        }
        let thread_ts = value_string(value, "thread_ts").unwrap_or_else(|| ts.clone());
        let display_name = value
            .get("user_profile")
            .and_then(|profile| {
                value_string(profile, "real_name").or_else(|| value_string(profile, "name"))
            })
            .unwrap_or_else(|| user_id.clone());
        let route = MessageRoute {
            platform: "slack".to_string(),
            chat_id: self.channel_id.clone(),
            chat_type: ChatType::Group,
            user_id: user_id.clone(),
            display_name,
            thread_id: thread_ts.clone(),
        };
        let metadata = json!({
            "channel": {
                "platform": "slack",
                "adapter": "slack-web-api",
                "chatId": self.channel_id,
                "chatType": route.chat_type.as_gateway_str(),
                "userId": user_id,
                "sourceMessageId": ts,
                "sourceMessageTs": ts,
                "threadTs": thread_ts
            }
        });
        Some(NormalizedInboundMessage {
            id: format!("slack-{ts}"),
            text,
            route,
            context: None,
            metadata,
        })
    }
}

impl PlatformAdapter for SlackAdapter {
    fn name(&self) -> &'static str {
        "slack"
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
        let response = slack_get(&self.history_url(), &self.bot_token)?;
        classify_slack_response(&response)?;
        let mut items = response
            .get("messages")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| ChannelError::retryable("slack history response missed messages"))?;
        items.sort_by(|left, right| {
            ts_number(value_string(left, "ts").as_deref())
                .partial_cmp(&ts_number(value_string(right, "ts").as_deref()))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let last_seen = self.last_ts.lock().ok().and_then(|value| value.clone());
        let mut next_last_seen = last_seen.clone();
        let messages = items
            .into_iter()
            .filter(|item| {
                let ts = value_string(item, "ts").unwrap_or_default();
                if next_last_seen
                    .as_deref()
                    .is_none_or(|seen| slack_ts_is_after(&ts, seen))
                {
                    next_last_seen = Some(ts.clone());
                }
                last_seen
                    .as_deref()
                    .is_none_or(|seen| slack_ts_is_after(&ts, seen))
            })
            .filter_map(|item| self.normalize_message(&item))
            .collect::<Vec<_>>();
        if let Ok(mut last_ts) = self.last_ts.lock() {
            *last_ts = next_last_seen;
        }
        Ok(messages)
    }

    fn send_typing(&self, _route: &MessageRoute) -> ChannelResult<()> {
        Err(ChannelError::unavailable(
            "slack typing indicator is unavailable in the Web API adapter",
        ))
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal("slack message text must not be empty"));
        }
        if outbound_mentions_media(&message.text, message.metadata.as_ref()) {
            return self.send_media_unavailable("outbound");
        }
        let channel_id = if message.route.chat_id.trim().is_empty() {
            self.channel_id.clone()
        } else {
            message.route.chat_id.clone()
        };
        let thread_ts = if self.reply_in_thread {
            outbound_thread_ts(&message)
        } else {
            None
        };
        let mut last_id = None;
        for chunk in split_text_chunks(&message.text, SLACK_MAX_MESSAGE_LENGTH) {
            let mut payload = json!({
                "channel": channel_id,
                "text": chunk,
                "unfurl_links": false,
                "unfurl_media": false
            });
            if let Some(thread_ts) = thread_ts.as_ref()
                && let Some(payload) = payload.as_object_mut()
            {
                payload.insert("thread_ts".to_string(), json!(thread_ts));
            }
            let response = slack_post(&self.post_message_url(), &self.bot_token, payload)?;
            classify_slack_response(&response)?;
            last_id =
                value_string(&response, "ts").or_else(|| Some(format!("slack-{}", now_millis())));
        }
        Ok(PlatformSendOutcome {
            message_id: last_id,
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "slack {media_kind} media delivery is explicitly unavailable in flyflor-cli"
        )))
    }
}

fn slack_get(url: &str, token: &str) -> ChannelResult<Value> {
    run_slack_curl(vec![
        "-sS".to_string(),
        "--max-time".to_string(),
        seconds_arg(SLACK_TIMEOUT_MS),
        "-H".to_string(),
        format!("Authorization: Bearer {token}"),
        url.to_string(),
    ])
}

fn slack_post(url: &str, token: &str, payload: Value) -> ChannelResult<Value> {
    let body =
        serde_json::to_string(&payload).map_err(|error| ChannelError::fatal(error.to_string()))?;
    run_slack_curl(vec![
        "-sS".to_string(),
        "--max-time".to_string(),
        seconds_arg(SLACK_TIMEOUT_MS),
        "-X".to_string(),
        "POST".to_string(),
        url.to_string(),
        "-H".to_string(),
        format!("Authorization: Bearer {token}"),
        "-H".to_string(),
        "Content-Type: application/json; charset=utf-8".to_string(),
        "--data".to_string(),
        body,
    ])
}

fn run_slack_curl(args: Vec<String>) -> ChannelResult<Value> {
    let output = Command::new("curl")
        .args(args)
        .output()
        .map_err(|error| ChannelError::unavailable(format!("curl is required: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr.trim();
        if message.contains("401") || message.contains("403") {
            return Err(ChannelError::session_expired(format!(
                "slack authorization failed: {message}"
            )));
        }
        if message.contains("429") {
            return Err(ChannelError::rate_limited(format!(
                "slack rate limited: {message}"
            )));
        }
        return Err(ChannelError::retryable(format!(
            "slack curl failed with status {}: {}",
            output.status, message
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let text = stdout.trim();
    if text.is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str::<Value>(text).map_err(|error| {
        ChannelError::retryable(format!("slack returned invalid JSON: {error}; body={text}"))
    })
}

fn classify_slack_response(value: &Value) -> ChannelResult<()> {
    if value.get("ok").and_then(Value::as_bool).unwrap_or(true) {
        return Ok(());
    }
    let error = value
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("unknown_error");
    match error {
        "invalid_auth" | "not_authed" | "token_revoked" | "account_inactive" => Err(
            ChannelError::session_expired(format!("Slack authorization failed: {error}")),
        ),
        "ratelimited" => Err(ChannelError::rate_limited(format!(
            "Slack rate limited: {error}"
        ))),
        "channel_not_found" | "is_archived" | "msg_too_long" | "no_text" => {
            Err(ChannelError::fatal(format!("Slack bad request: {error}")))
        }
        _ => Err(ChannelError::retryable(format!("Slack error: {error}"))),
    }
}

fn outbound_thread_ts(message: &OutboundMessage) -> Option<String> {
    message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("channel"))
        .and_then(|channel| {
            value_string(channel, "threadTs").or_else(|| value_string(channel, "sourceMessageTs"))
        })
        .or_else(|| message.reply_to_message_id.clone())
        .map(|id| id.strip_prefix("slack-").unwrap_or(&id).to_string())
        .filter(|id| !id.is_empty())
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

fn env_bool(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default)
}

fn seconds_arg(timeout_ms: u64) -> String {
    let seconds = (timeout_ms as f64 / 1000.0).max(1.0);
    format!("{seconds:.3}")
}

fn url_encode_component(value: &str) -> String {
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

fn slack_ts_is_after(candidate: &str, seen: &str) -> bool {
    ts_number(Some(candidate)) > ts_number(Some(seen))
}

fn ts_number(value: Option<&str>) -> f64 {
    value
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(0.0)
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
    fn normalizes_slack_message() {
        let adapter = test_adapter();
        let message = adapter
            .normalize_message(&json!({
                "type": "message",
                "user": "U1",
                "text": "hello slack",
                "ts": "1710000000.000001",
                "thread_ts": "1710000000.000000",
                "user_profile": { "real_name": "User One" }
            }))
            .unwrap();

        assert_eq!(message.id, "slack-1710000000.000001");
        assert_eq!(message.text, "hello slack");
        assert_eq!(message.route.platform, "slack");
        assert_eq!(message.route.chat_id, "C1");
        assert_eq!(message.route.chat_type, ChatType::Group);
        assert_eq!(message.route.user_id, "U1");
        assert_eq!(message.route.thread_id, "1710000000.000000");
        assert_eq!(message.metadata["channel"]["threadTs"], "1710000000.000000");
    }

    #[test]
    fn filters_bot_subtype_empty_and_disallowed_users() {
        let mut adapter = test_adapter();
        adapter.bot_user_id = Some("UBOT".to_string());
        adapter.allowed_users = HashSet::from(["UOK".to_string()]);

        assert!(adapter.normalize_message(&message("UBOT")).is_none());
        assert!(adapter.normalize_message(&message("UBLOCKED")).is_none());
        assert!(
            adapter
                .normalize_message(&json!({
                    "type": "message",
                    "subtype": "bot_message",
                    "user": "UOK",
                    "text": "ignored",
                    "ts": "1710000000.000002"
                }))
                .is_none()
        );
        assert!(
            adapter
                .normalize_message(&json!({
                    "type": "message",
                    "bot_id": "B1",
                    "user": "UOK",
                    "text": "ignored",
                    "ts": "1710000000.000003"
                }))
                .is_none()
        );
        assert!(
            adapter
                .normalize_message(&json!({
                    "type": "message",
                    "user": "UOK",
                    "text": "   ",
                    "ts": "1710000000.000004"
                }))
                .is_none()
        );
    }

    #[test]
    fn classifies_slack_error_codes() {
        assert_eq!(
            classify_slack_response(&json!({
                "ok": false,
                "error": "invalid_auth"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::SessionExpired
        );
        assert_eq!(
            classify_slack_response(&json!({
                "ok": false,
                "error": "ratelimited"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::RateLimited
        );
        assert_eq!(
            classify_slack_response(&json!({
                "ok": false,
                "error": "channel_not_found"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::Fatal
        );
    }

    #[test]
    fn outbound_thread_prefers_channel_anchor_metadata() {
        let message = OutboundMessage {
            route: MessageRoute {
                platform: "slack".to_string(),
                chat_id: "C1".to_string(),
                chat_type: ChatType::Group,
                user_id: "U1".to_string(),
                display_name: "User".to_string(),
                thread_id: "thread".to_string(),
            },
            text: "reply".to_string(),
            reply_to_message_id: Some("slack-1710000000.000009".to_string()),
            metadata: Some(json!({
                "channel": {
                    "threadTs": "1710000000.000001"
                }
            })),
        };

        assert_eq!(
            outbound_thread_ts(&message),
            Some("1710000000.000001".to_string())
        );
    }

    #[test]
    fn split_text_and_url_encoding_preserve_unicode() {
        assert_eq!(split_text_chunks("你好世界", 2), vec!["你好", "世界"]);
        assert_eq!(url_encode_component("C/1"), "C%2F1");
    }

    fn message(user_id: &str) -> Value {
        json!({
            "type": "message",
            "user": user_id,
            "text": "hello",
            "ts": "1710000000.000001"
        })
    }

    fn test_adapter() -> SlackAdapter {
        SlackAdapter {
            api_base: DEFAULT_SLACK_API_BASE.to_string(),
            bot_token: "token".to_string(),
            channel_id: "C1".to_string(),
            bot_user_id: None,
            allowed_users: HashSet::new(),
            reply_in_thread: true,
            last_ts: Mutex::new(None),
        }
    }
}
