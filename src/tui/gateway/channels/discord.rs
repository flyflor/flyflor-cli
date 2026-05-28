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

const DEFAULT_DISCORD_API_BASE: &str = "https://discord.com";
const DISCORD_MAX_MESSAGE_LENGTH: usize = 1_900;
const DISCORD_TIMEOUT_MS: u64 = 15_000;

pub struct DiscordAdapter {
    api_base: String,
    bot_token: String,
    channel_id: String,
    bot_user_id: Option<String>,
    allowed_users: HashSet<String>,
    last_message_id: Mutex<Option<String>>,
}

impl DiscordAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let bot_token = env::var("DISCORD_BOT_TOKEN")
            .or_else(|_| env::var("DISCORD_TOKEN"))
            .or_else(|_| env::var("FLYFLOR_DISCORD_TOKEN"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if bot_token.is_empty() {
            return Err(ChannelError::missing_config(
                "DISCORD_BOT_TOKEN is required for the discord channel",
            ));
        }
        let channel_id = env::var("DISCORD_HOME_CHANNEL")
            .or_else(|_| env::var("DISCORD_CHANNEL_ID"))
            .or_else(|_| env::var("FLYFLOR_DISCORD_HOME_CHANNEL"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if channel_id.is_empty() {
            return Err(ChannelError::missing_config(
                "DISCORD_HOME_CHANNEL is required for the discord channel",
            ));
        }
        Ok(Self {
            api_base: env::var("DISCORD_API_BASE")
                .or_else(|_| env::var("FLYFLOR_DISCORD_API_BASE"))
                .unwrap_or_else(|_| DEFAULT_DISCORD_API_BASE.to_string())
                .trim()
                .trim_end_matches('/')
                .to_string(),
            bot_token,
            channel_id,
            bot_user_id: env::var("DISCORD_BOT_USER_ID")
                .or_else(|_| env::var("FLYFLOR_DISCORD_BOT_USER_ID"))
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            allowed_users: env_set("DISCORD_ALLOWED_USERS"),
            last_message_id: Mutex::new(
                env::var("DISCORD_SINCE_MESSAGE_ID")
                    .ok()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty()),
            ),
        })
    }

    fn messages_url(&self) -> String {
        format!(
            "{}/api/v10/channels/{}/messages?limit=20",
            self.api_base,
            url_encode_path(&self.channel_id)
        )
    }

    fn create_message_url(&self, channel_id: &str) -> String {
        format!(
            "{}/api/v10/channels/{}/messages",
            self.api_base,
            url_encode_path(channel_id)
        )
    }

    fn normalize_message(&self, value: &Value) -> Option<NormalizedInboundMessage> {
        if value.get("type").and_then(Value::as_i64).unwrap_or(0) != 0 {
            return None;
        }
        let id = value_string(value, "id")?;
        let author = value.get("author")?;
        let user_id = value_string(author, "id")?;
        if self.bot_user_id.as_deref() == Some(user_id.as_str()) {
            return None;
        }
        if author.get("bot").and_then(Value::as_bool).unwrap_or(false) {
            return None;
        }
        if !self.allowed_users.is_empty() && !self.allowed_users.contains(&user_id) {
            return None;
        }
        let text = value_string(value, "content")?.trim().to_string();
        if text.is_empty() {
            return None;
        }
        let channel_id =
            value_string(value, "channel_id").unwrap_or_else(|| self.channel_id.clone());
        let thread_id = value_string(value, "thread_id")
            .or_else(|| value_string(value, "guild_id"))
            .unwrap_or_else(|| channel_id.clone());
        let route = MessageRoute {
            platform: "discord".to_string(),
            chat_id: channel_id.clone(),
            chat_type: ChatType::Group,
            user_id: user_id.clone(),
            display_name: value_string(author, "global_name")
                .or_else(|| value_string(author, "username"))
                .unwrap_or_else(|| user_id.clone()),
            thread_id: thread_id.clone(),
        };
        let metadata = json!({
            "channel": {
                "platform": "discord",
                "adapter": "discord-rest",
                "chatId": channel_id,
                "chatType": route.chat_type.as_gateway_str(),
                "userId": user_id,
                "sourceMessageId": id,
                "threadId": thread_id,
                "guildId": value_string(value, "guild_id")
            }
        });
        Some(NormalizedInboundMessage {
            id: format!("discord-{id}"),
            text,
            route,
            context: None,
            metadata,
        })
    }
}

impl PlatformAdapter for DiscordAdapter {
    fn name(&self) -> &'static str {
        "discord"
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
        let response = discord_get(&self.messages_url(), &self.bot_token)?;
        classify_discord_response(&response)?;
        let mut items = response
            .as_array()
            .cloned()
            .ok_or_else(|| ChannelError::retryable("discord messages response was not an array"))?;
        items.sort_by_key(|item| value_string(item, "id").unwrap_or_default());
        let last_seen = self
            .last_message_id
            .lock()
            .ok()
            .and_then(|value| value.clone());
        let mut next_last_seen = last_seen.clone();
        let messages = items
            .into_iter()
            .filter(|item| {
                let id = value_string(item, "id").unwrap_or_default();
                if next_last_seen
                    .as_deref()
                    .is_none_or(|seen| id.as_str() > seen)
                {
                    next_last_seen = Some(id.clone());
                }
                last_seen.as_deref().is_none_or(|seen| id.as_str() > seen)
            })
            .filter_map(|item| self.normalize_message(&item))
            .collect::<Vec<_>>();
        if let Ok(mut last_message_id) = self.last_message_id.lock() {
            *last_message_id = next_last_seen;
        }
        Ok(messages)
    }

    fn send_typing(&self, _route: &MessageRoute) -> ChannelResult<()> {
        Err(ChannelError::unavailable(
            "discord typing indicator is unavailable in the REST adapter",
        ))
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal(
                "discord message text must not be empty",
            ));
        }
        if outbound_mentions_media(&message.text, message.metadata.as_ref()) {
            return self.send_media_unavailable("outbound");
        }
        let channel_id = if message.route.chat_id.trim().is_empty() {
            self.channel_id.clone()
        } else {
            message.route.chat_id.clone()
        };
        let mut last_id = None;
        for chunk in split_text_chunks(&message.text, DISCORD_MAX_MESSAGE_LENGTH) {
            let response = discord_post(
                &self.create_message_url(&channel_id),
                &self.bot_token,
                json!({
                    "content": chunk,
                    "message_reference": message.reply_to_message_id.as_ref().map(|id| json!({
                        "message_id": id.strip_prefix("discord-").unwrap_or(id)
                    })),
                    "allowed_mentions": { "parse": [] }
                }),
            )?;
            classify_discord_response(&response)?;
            last_id =
                value_string(&response, "id").or_else(|| Some(format!("discord-{}", now_millis())));
        }
        Ok(PlatformSendOutcome {
            message_id: last_id,
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "discord {media_kind} media delivery is explicitly unavailable in flyflor-cli"
        )))
    }
}

fn discord_get(url: &str, token: &str) -> ChannelResult<Value> {
    run_discord_curl(vec![
        "-sS".to_string(),
        "--max-time".to_string(),
        seconds_arg(DISCORD_TIMEOUT_MS),
        "-H".to_string(),
        format!("Authorization: Bot {token}"),
        url.to_string(),
    ])
}

fn discord_post(url: &str, token: &str, payload: Value) -> ChannelResult<Value> {
    let body =
        serde_json::to_string(&payload).map_err(|error| ChannelError::fatal(error.to_string()))?;
    run_discord_curl(vec![
        "-sS".to_string(),
        "--max-time".to_string(),
        seconds_arg(DISCORD_TIMEOUT_MS),
        "-X".to_string(),
        "POST".to_string(),
        url.to_string(),
        "-H".to_string(),
        format!("Authorization: Bot {token}"),
        "-H".to_string(),
        "Content-Type: application/json".to_string(),
        "--data".to_string(),
        body,
    ])
}

fn run_discord_curl(args: Vec<String>) -> ChannelResult<Value> {
    let output = Command::new("curl")
        .args(args)
        .output()
        .map_err(|error| ChannelError::unavailable(format!("curl is required: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr.trim();
        if message.contains("401") || message.contains("403") {
            return Err(ChannelError::session_expired(format!(
                "discord authorization failed: {message}"
            )));
        }
        if message.contains("429") {
            return Err(ChannelError::rate_limited(format!(
                "discord rate limited: {message}"
            )));
        }
        return Err(ChannelError::retryable(format!(
            "discord curl failed with status {}: {}",
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
            "discord returned invalid JSON: {error}; body={text}"
        ))
    })
}

fn classify_discord_response(value: &Value) -> ChannelResult<()> {
    let status = value
        .get("status")
        .or_else(|| value.get("status_code"))
        .or_else(|| value.get("statusCode"))
        .or_else(|| value.get("code"))
        .and_then(Value::as_i64);
    if status.is_none() || status.is_some_and(|status| status < 400) {
        return Ok(());
    }
    let status = status.unwrap_or_default();
    let message = value
        .get("message")
        .or_else(|| value.get("error"))
        .and_then(Value::as_str)
        .unwrap_or("unknown discord error");
    match status {
        401 | 403 => Err(ChannelError::session_expired(format!(
            "Discord authorization failed: status={status} message={message}"
        ))),
        429 => Err(ChannelError::rate_limited(format!(
            "Discord rate limited: status={status} message={message}"
        ))),
        400 | 404 => Err(ChannelError::fatal(format!(
            "Discord bad request: status={status} message={message}"
        ))),
        _ => Err(ChannelError::retryable(format!(
            "Discord error: status={status} message={message}"
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

fn url_encode_path(value: &str) -> String {
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
    fn normalizes_discord_message() {
        let adapter = test_adapter();
        let message = adapter
            .normalize_message(&json!({
                "id": "100",
                "type": 0,
                "channel_id": "channel-1",
                "guild_id": "guild-1",
                "content": "hello discord",
                "author": {
                    "id": "user-1",
                    "username": "User One",
                    "bot": false
                }
            }))
            .unwrap();

        assert_eq!(message.id, "discord-100");
        assert_eq!(message.text, "hello discord");
        assert_eq!(message.route.platform, "discord");
        assert_eq!(message.route.chat_id, "channel-1");
        assert_eq!(message.route.chat_type, ChatType::Group);
        assert_eq!(message.route.user_id, "user-1");
        assert_eq!(message.metadata["channel"]["guildId"], "guild-1");
    }

    #[test]
    fn filters_bots_self_non_text_and_disallowed_users() {
        let mut adapter = test_adapter();
        adapter.bot_user_id = Some("bot-1".to_string());
        adapter.allowed_users = HashSet::from(["allowed".to_string()]);

        assert!(
            adapter
                .normalize_message(&message("100", "bot-1"))
                .is_none()
        );
        assert!(
            adapter
                .normalize_message(&message("101", "blocked"))
                .is_none()
        );
        assert!(
            adapter
                .normalize_message(&json!({
                    "id": "102",
                    "type": 1,
                    "content": "ignored",
                    "author": { "id": "allowed", "bot": false }
                }))
                .is_none()
        );
        assert!(
            adapter
                .normalize_message(&json!({
                    "id": "103",
                    "type": 0,
                    "content": "ignored",
                    "author": { "id": "allowed", "bot": true }
                }))
                .is_none()
        );
    }

    #[test]
    fn classifies_discord_error_codes() {
        assert_eq!(
            classify_discord_response(&json!({
                "status": 401,
                "message": "unauthorized"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::SessionExpired
        );
        assert_eq!(
            classify_discord_response(&json!({
                "status_code": 429,
                "message": "slow down"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::RateLimited
        );
        assert_eq!(
            classify_discord_response(&json!({
                "statusCode": 400,
                "message": "bad"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::Fatal
        );
    }

    #[test]
    fn split_text_and_path_encoding_preserve_unicode() {
        assert_eq!(split_text_chunks("你好世界", 2), vec!["你好", "世界"]);
        assert_eq!(url_encode_path("channel/id"), "channel%2Fid");
    }

    fn message(id: &str, user_id: &str) -> Value {
        json!({
            "id": id,
            "type": 0,
            "channel_id": "channel-1",
            "content": "hello",
            "author": { "id": user_id, "bot": false }
        })
    }

    fn test_adapter() -> DiscordAdapter {
        DiscordAdapter {
            api_base: DEFAULT_DISCORD_API_BASE.to_string(),
            bot_token: "token".to_string(),
            channel_id: "channel-1".to_string(),
            bot_user_id: None,
            allowed_users: HashSet::new(),
            last_message_id: Mutex::new(None),
        }
    }
}
