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

const DEFAULT_QQBOT_API_BASE: &str = "https://api.sgroup.qq.com";
const DEFAULT_QQBOT_TOKEN_URL: &str = "https://bots.qq.com/app/getAppAccessToken";
const QQBOT_MAX_TEXT_LENGTH: usize = 4_000;
const QQBOT_TIMEOUT_MS: u64 = 15_000;

pub struct QqBotAdapter {
    api_base: String,
    token_url: String,
    app_id: String,
    client_secret: String,
    access_token: Option<String>,
    home_channel: Option<String>,
    allowed_users: HashSet<String>,
}

impl QqBotAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let app_id = env_first(&["QQBOT_APP_ID", "QQ_APP_ID", "FLYFLOR_QQBOT_APP_ID"]);
        if app_id.is_empty() {
            return Err(ChannelError::missing_config(
                "QQBOT_APP_ID is required for the qqbot channel",
            ));
        }
        let client_secret = env_first(&[
            "QQBOT_SECRET",
            "QQBOT_CLIENT_SECRET",
            "QQ_CLIENT_SECRET",
            "FLYFLOR_QQBOT_SECRET",
        ]);
        if client_secret.is_empty() {
            return Err(ChannelError::missing_config(
                "QQBOT_SECRET is required for the qqbot channel",
            ));
        }
        Ok(Self {
            api_base: env_first(&["QQBOT_API_BASE", "QQ_API_BASE", "FLYFLOR_QQBOT_API_BASE"])
                .if_empty(DEFAULT_QQBOT_API_BASE),
            token_url: env_first(&["QQBOT_TOKEN_URL", "QQ_TOKEN_URL", "FLYFLOR_QQBOT_TOKEN_URL"])
                .if_empty(DEFAULT_QQBOT_TOKEN_URL),
            app_id,
            client_secret,
            access_token: env_optional(&["QQBOT_TOKEN", "QQ_ACCESS_TOKEN", "FLYFLOR_QQBOT_TOKEN"]),
            home_channel: env_optional(&["QQBOT_HOME_CHANNEL", "QQ_HOME_CHANNEL"]),
            allowed_users: env_set_any(&["QQBOT_ALLOWED_USERS", "QQ_ALLOWED_USERS"]),
        })
    }

    fn access_token(&self) -> ChannelResult<String> {
        if let Some(token) = self.access_token.clone() {
            return Ok(token);
        }
        let response = qqbot_post(
            &self.token_url,
            None,
            json!({
                "appId": self.app_id,
                "clientSecret": self.client_secret
            }),
        )?;
        classify_qqbot_response(&response)?;
        value_string_any(&response, &["access_token", "accessToken"])
            .ok_or_else(|| ChannelError::retryable("qqbot access token missing from response"))
    }

    fn normalize_event(&self, value: &Value) -> Option<NormalizedInboundMessage> {
        let event = value
            .get("event")
            .or_else(|| value.get("d"))
            .or_else(|| value.get("payload"))
            .unwrap_or(value);
        let text = value_string_any(event, &["content", "text", "message"])
            .or_else(|| {
                event
                    .get("text")
                    .and_then(|text| value_string_any(text, &["content", "text"]))
            })?
            .trim()
            .to_string();
        if text.is_empty() {
            return None;
        }
        let author = event
            .get("author")
            .or_else(|| event.get("sender"))
            .unwrap_or(&Value::Null);
        let user_id = value_string_any(
            author,
            &["member_openid", "user_openid", "openid", "id", "user_id"],
        )
        .or_else(|| {
            value_string_any(
                event,
                &[
                    "member_openid",
                    "user_openid",
                    "openid",
                    "userId",
                    "user_id",
                ],
            )
        })
        .unwrap_or_else(|| "qqbot-user".to_string());
        if !self.allowed_users.is_empty() && !self.allowed_users.contains(&user_id) {
            return None;
        }
        let route_kind = qq_route_kind(event);
        let chat_id = match route_kind.as_str() {
            "group" => value_string_any(event, &["group_openid", "groupOpenid", "group_id"]),
            "guild" => value_string_any(event, &["channel_id", "channelId"]),
            _ => value_string_any(event, &["user_openid", "openid", "userId", "user_id"]),
        }
        .unwrap_or_else(|| user_id.clone());
        let message_id = value_string_any(event, &["id", "msg_id", "message_id", "messageId"])
            .unwrap_or_else(|| format!("qqbot-{}", now_millis()));
        let display_name = value_string_any(author, &["username", "nick", "name"])
            .or_else(|| value_string_any(event, &["username", "nick", "name"]))
            .unwrap_or_else(|| user_id.clone());
        let chat_type = if route_kind == "c2c" || chat_id == user_id {
            ChatType::Direct
        } else {
            ChatType::Group
        };
        let route = MessageRoute {
            platform: "qqbot".to_string(),
            chat_id: chat_id.clone(),
            chat_type,
            user_id: user_id.clone(),
            display_name,
            thread_id: chat_id.clone(),
        };
        let metadata = json!({
            "channel": {
                "platform": "qqbot",
                "adapter": "qq-official-v2",
                "chatId": chat_id,
                "chatType": route.chat_type.as_gateway_str(),
                "userId": user_id,
                "sourceMessageId": message_id,
                "qqRoute": route_kind
            }
        });
        Some(NormalizedInboundMessage {
            id: format!("qqbot-{message_id}"),
            text: strip_at_mention(&text),
            route,
            context: None,
            metadata,
        })
    }
}

impl PlatformAdapter for QqBotAdapter {
    fn name(&self) -> &'static str {
        "qqbot"
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
        let raw = env_first(&["QQBOT_INBOUND_EVENT", "QQ_INBOUND_EVENT"]);
        if raw.is_empty() {
            return Ok(Vec::new());
        }
        let value = serde_json::from_str::<Value>(&raw).map_err(|error| {
            ChannelError::fatal(format!("qqbot inbound event JSON is invalid: {error}"))
        })?;
        Ok(self.normalize_event(&value).into_iter().collect())
    }

    fn send_typing(&self, _route: &MessageRoute) -> ChannelResult<()> {
        Err(ChannelError::unavailable(
            "qqbot typing/input_notify is unavailable in the REST text adapter",
        ))
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal("qqbot message text must not be empty"));
        }
        if outbound_mentions_media(&message.text, message.metadata.as_ref()) {
            return self.send_media_unavailable("outbound");
        }
        let token = self.access_token()?;
        let route_kind = outbound_route_kind(&message);
        let target_id = if message.route.chat_id.trim().is_empty() {
            self.home_channel.clone().ok_or_else(|| {
                ChannelError::fatal("QQBOT_HOME_CHANNEL is required when route chat_id is empty")
            })?
        } else {
            message.route.chat_id.clone()
        };
        let mut last_id = None;
        for chunk in split_text_chunks(&message.text, QQBOT_MAX_TEXT_LENGTH) {
            let mut body = if route_kind == "guild" {
                json!({ "content": chunk })
            } else {
                json!({
                    "content": chunk,
                    "msg_type": 0,
                    "msg_seq": now_millis()
                })
            };
            if let Some(reply_to) = message.reply_to_message_id.as_ref() {
                body["msg_id"] = json!(reply_to);
            }
            let url = match route_kind.as_str() {
                "group" => format!(
                    "{}/v2/groups/{}/messages",
                    self.api_base,
                    path_segment(&target_id)
                ),
                "guild" => format!(
                    "{}/channels/{}/messages",
                    self.api_base,
                    path_segment(&target_id)
                ),
                _ => format!(
                    "{}/v2/users/{}/messages",
                    self.api_base,
                    path_segment(&target_id)
                ),
            };
            let response = qqbot_post(&url, Some(&token), body)?;
            classify_qqbot_response(&response)?;
            last_id = value_string_any(&response, &["id", "message_id", "messageId"])
                .or_else(|| Some(format!("qqbot-{}", now_millis())));
        }
        Ok(PlatformSendOutcome {
            message_id: last_id,
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "qqbot {media_kind} media delivery is explicitly unavailable in flyflor-cli"
        )))
    }
}

fn qqbot_post(url: &str, token: Option<&str>, payload: Value) -> ChannelResult<Value> {
    let body =
        serde_json::to_string(&payload).map_err(|error| ChannelError::fatal(error.to_string()))?;
    let mut args = vec![
        "-sS".to_string(),
        "--max-time".to_string(),
        seconds_arg(QQBOT_TIMEOUT_MS),
        "-X".to_string(),
        "POST".to_string(),
        url.to_string(),
        "-H".to_string(),
        "Content-Type: application/json".to_string(),
    ];
    if let Some(token) = token {
        args.push("-H".to_string());
        args.push(format!("Authorization: QQBot {token}"));
    }
    args.push("--data".to_string());
    args.push(body);
    let output = Command::new("curl")
        .args(args)
        .output()
        .map_err(|error| ChannelError::unavailable(format!("curl is required: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr.trim();
        if message.contains("401") || message.contains("403") {
            return Err(ChannelError::session_expired(format!(
                "qqbot authorization failed: {message}"
            )));
        }
        if message.contains("429") {
            return Err(ChannelError::rate_limited(format!(
                "qqbot rate limited: {message}"
            )));
        }
        return Err(ChannelError::retryable(format!(
            "qqbot curl failed with status {}: {}",
            output.status, message
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let text = stdout.trim();
    if text.is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str::<Value>(text).map_err(|error| {
        ChannelError::retryable(format!("qqbot returned invalid JSON: {error}; body={text}"))
    })
}

fn classify_qqbot_response(value: &Value) -> ChannelResult<()> {
    let Some(code) = value_i64_any(value, &["code", "errcode"]) else {
        return Ok(());
    };
    if code == 0 {
        return Ok(());
    }
    let message = value_string_any(value, &["message", "errmsg"])
        .unwrap_or_else(|| "unknown qqbot error".to_string());
    match code {
        11241 | 11246 | 11247 | 401 | 403 => Err(ChannelError::session_expired(format!(
            "QQBot authorization failed: code={code} message={message}"
        ))),
        304023 | 429 => Err(ChannelError::rate_limited(format!(
            "QQBot rate limited: code={code} message={message}"
        ))),
        400 | 404 => Err(ChannelError::fatal(format!(
            "QQBot bad request: code={code} message={message}"
        ))),
        _ => Err(ChannelError::retryable(format!(
            "QQBot error: code={code} message={message}"
        ))),
    }
}

fn qq_route_kind(event: &Value) -> String {
    value_string_any(event, &["qqRoute", "route", "chat_type", "type"])
        .map(|value| value.to_ascii_lowercase())
        .filter(|value| matches!(value.as_str(), "c2c" | "group" | "guild"))
        .unwrap_or_else(|| {
            if value_string_any(event, &["group_openid", "groupOpenid", "group_id"]).is_some() {
                "group".to_string()
            } else if value_string_any(event, &["channel_id", "channelId"]).is_some() {
                "guild".to_string()
            } else {
                "c2c".to_string()
            }
        })
}

fn outbound_route_kind(message: &OutboundMessage) -> String {
    message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("channel"))
        .and_then(|channel| value_string_any(channel, &["qqRoute", "route"]))
        .map(|value| value.to_ascii_lowercase())
        .filter(|value| matches!(value.as_str(), "c2c" | "group" | "guild"))
        .unwrap_or_else(|| {
            if message.route.chat_type == ChatType::Group {
                "group".to_string()
            } else {
                "c2c".to_string()
            }
        })
}

fn value_string_any(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    })
}

fn value_i64_any(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(|value| value.as_i64().or_else(|| value.as_str()?.parse().ok()))
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

fn strip_at_mention(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with('@') {
        trimmed
            .split_once(char::is_whitespace)
            .map(|(_, rest)| rest.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| trimmed.to_string())
    } else {
        trimmed.to_string()
    }
}

fn outbound_mentions_media(text: &str, metadata: Option<&Value>) -> bool {
    if text.contains("MEDIA:") {
        return true;
    }
    metadata
        .and_then(|value| value.get("media"))
        .is_some_and(|media| !media.is_null())
}

fn path_segment(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
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
    fn normalizes_qqbot_group_event() {
        let adapter = test_adapter();
        let message = adapter
            .normalize_event(&json!({
                "group_openid": "group-1",
                "id": "msg-1",
                "content": "@bot hello qq",
                "author": {
                    "member_openid": "member-1",
                    "username": "QQ User"
                }
            }))
            .unwrap();

        assert_eq!(message.id, "qqbot-msg-1");
        assert_eq!(message.text, "hello qq");
        assert_eq!(message.route.platform, "qqbot");
        assert_eq!(message.route.chat_id, "group-1");
        assert_eq!(message.route.chat_type, ChatType::Group);
        assert_eq!(message.route.user_id, "member-1");
        assert_eq!(message.metadata["channel"]["qqRoute"], "group");
    }

    #[test]
    fn allowlist_blocks_unknown_sender() {
        let mut adapter = test_adapter();
        adapter.allowed_users = HashSet::from(["member-2".to_string()]);

        assert!(
            adapter
                .normalize_event(&json!({
                    "group_openid": "group-1",
                    "id": "msg-1",
                    "content": "blocked",
                    "author": { "member_openid": "member-1" }
                }))
                .is_none()
        );
    }

    #[test]
    fn classifies_qqbot_error_codes() {
        assert_eq!(
            classify_qqbot_response(&json!({
                "code": 11241,
                "message": "invalid token"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::SessionExpired
        );
        assert_eq!(
            classify_qqbot_response(&json!({
                "code": 429,
                "message": "slow down"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::RateLimited
        );
        assert_eq!(
            classify_qqbot_response(&json!({
                "code": 404,
                "message": "missing"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::Fatal
        );
    }

    #[test]
    fn split_text_preserves_unicode() {
        assert_eq!(split_text_chunks("你好世界", 2), vec!["你好", "世界"]);
    }

    fn test_adapter() -> QqBotAdapter {
        QqBotAdapter {
            api_base: DEFAULT_QQBOT_API_BASE.to_string(),
            token_url: DEFAULT_QQBOT_TOKEN_URL.to_string(),
            app_id: "app-id".to_string(),
            client_secret: "secret".to_string(),
            access_token: Some("token".to_string()),
            home_channel: None,
            allowed_users: HashSet::new(),
        }
    }
}
