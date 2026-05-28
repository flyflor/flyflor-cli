use std::{
    collections::{HashMap, HashSet},
    env,
    process::Command,
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde_json::{Map, Value, json};

use super::platform::{
    ChannelCapabilityReport, ChannelCapabilityState, ChannelError, ChannelErrorKind, ChannelResult,
    ChatType, MessageRoute, NormalizedInboundMessage, OutboundMessage, OutboundStreamUpdate,
    PlatformAdapter, PlatformSendOutcome, StreamDeliveryMode,
};

const TELEGRAM_API_BASE_URL: &str = "https://api.telegram.org";
const GET_UPDATES_TIMEOUT_MS: u64 = 35_000;
const API_TIMEOUT_MS: u64 = 15_000;
const MESSAGE_DEDUP_TTL_SECONDS: u64 = 300;
const MAX_MESSAGE_LENGTH: usize = 4_000;

pub struct TelegramBotAdapter {
    token: String,
    base_url: String,
    allowed_users: HashSet<String>,
    allow_all_users: bool,
    offset: Mutex<i64>,
    dedup: Mutex<TtlDedup>,
}

impl TelegramBotAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let token = env::var("TELEGRAM_BOT_TOKEN")
            .or_else(|_| env::var("TELEGRAM_TOKEN"))
            .or_else(|_| env::var("FLYFLOR_TELEGRAM_BOT_TOKEN"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if token.is_empty() {
            return Err(ChannelError::missing_config(
                "TELEGRAM_BOT_TOKEN is required for the telegram channel",
            ));
        }
        Ok(Self {
            token,
            base_url: env::var("TELEGRAM_BASE_URL")
                .or_else(|_| env::var("FLYFLOR_TELEGRAM_BASE_URL"))
                .unwrap_or_else(|_| TELEGRAM_API_BASE_URL.to_string())
                .trim()
                .trim_end_matches('/')
                .to_string(),
            allowed_users: env_set("TELEGRAM_ALLOWED_USERS"),
            allow_all_users: env_bool("TELEGRAM_ALLOW_ALL_USERS", true),
            offset: Mutex::new(env_i64("TELEGRAM_UPDATE_OFFSET", 0)),
            dedup: Mutex::new(TtlDedup::new(
                MESSAGE_DEDUP_TTL_SECONDS,
                env_usize("TELEGRAM_DEDUP_MAX", 2_000),
            )),
        })
    }

    fn get_updates(&self) -> ChannelResult<Value> {
        let offset = self.offset.lock().map(|offset| *offset).unwrap_or_default();
        telegram_post(
            &self.base_url,
            &self.token,
            "getUpdates",
            json!({
                "offset": offset,
                "timeout": 30,
                "allowed_updates": ["message"]
            }),
            GET_UPDATES_TIMEOUT_MS,
        )
    }

    fn normalize_update(&self, update: &Value) -> Option<NormalizedInboundMessage> {
        let update_id = update.get("update_id").and_then(Value::as_i64)?;
        let message = update.get("message")?;
        let message_id = message.get("message_id").and_then(Value::as_i64)?;
        let from = message.get("from")?;
        let user_id = from.get("id").and_then(Value::as_i64)?.to_string();
        if !self.is_allowed(&user_id) {
            return None;
        }
        let chat = message.get("chat")?;
        let chat_id = chat.get("id").and_then(Value::as_i64)?.to_string();
        let chat_type = match chat.get("type").and_then(Value::as_str) {
            Some("group" | "supergroup" | "channel") => ChatType::Group,
            _ => ChatType::Direct,
        };
        let text = message
            .get("text")
            .or_else(|| message.get("caption"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(ToString::to_string)
            .or_else(|| media_notice(message))?;
        let source_message_id = format!("{chat_id}:{message_id}");
        if self.is_duplicate(&source_message_id) {
            return None;
        }
        if let Ok(mut offset) = self.offset.lock() {
            *offset = update_id + 1;
        }

        let display_name = display_name(from);
        let route = MessageRoute {
            platform: "telegram".to_string(),
            chat_id: chat_id.clone(),
            chat_type,
            user_id: user_id.clone(),
            display_name: display_name.clone(),
            thread_id: chat_id.clone(),
        };
        let metadata = json!({
            "channel": {
                "platform": "telegram",
                "adapter": "telegram-bot",
                "chatId": chat_id,
                "chatType": route.chat_type.as_gateway_str(),
                "userId": user_id,
                "sourceMessageId": source_message_id,
                "updateId": update_id,
                "username": from.get("username").and_then(Value::as_str),
                "mediaUnavailable": text.contains("media unavailable")
            }
        });
        Some(NormalizedInboundMessage {
            id: format!("telegram-{update_id}-{message_id}"),
            text,
            route,
            context: None,
            metadata,
        })
    }

    fn is_allowed(&self, user_id: &str) -> bool {
        self.allow_all_users || self.allowed_users.contains(user_id)
    }

    fn is_duplicate(&self, key: &str) -> bool {
        self.dedup
            .lock()
            .map(|mut dedup| dedup.is_duplicate(key))
            .unwrap_or(false)
    }

    fn send_text(&self, route: &MessageRoute, text: &str) -> ChannelResult<String> {
        let response = telegram_post(
            &self.base_url,
            &self.token,
            "sendMessage",
            json!({
                "chat_id": route.chat_id,
                "text": text
            }),
            API_TIMEOUT_MS,
        )?;
        classify_telegram_response(&response)?;
        response
            .get("result")
            .and_then(|result| result.get("message_id"))
            .and_then(Value::as_i64)
            .map(|id| id.to_string())
            .ok_or_else(|| ChannelError::retryable("Telegram sendMessage omitted message_id"))
    }
}

impl PlatformAdapter for TelegramBotAdapter {
    fn name(&self) -> &'static str {
        "telegram"
    }

    fn capabilities(&self) -> ChannelCapabilityReport {
        ChannelCapabilityReport {
            send: ChannelCapabilityState::Available,
            typing: ChannelCapabilityState::Available,
            edit: ChannelCapabilityState::Available,
            draft: ChannelCapabilityState::Unavailable,
            card: ChannelCapabilityState::Unavailable,
            media: ChannelCapabilityState::Unavailable,
        }
    }

    fn poll_updates(&self) -> ChannelResult<Vec<NormalizedInboundMessage>> {
        let response = self.get_updates()?;
        classify_telegram_response(&response)?;
        Ok(response
            .get("result")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|update| self.normalize_update(update))
            .collect())
    }

    fn send_typing(&self, route: &MessageRoute) -> ChannelResult<()> {
        let response = telegram_post(
            &self.base_url,
            &self.token,
            "sendChatAction",
            json!({
                "chat_id": route.chat_id,
                "action": "typing"
            }),
            API_TIMEOUT_MS,
        )?;
        classify_telegram_response(&response)
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal(
                "telegram sendMessage text must not be empty",
            ));
        }
        if outbound_mentions_media(&message.text, message.metadata.as_ref()) {
            return self.send_media_unavailable("outbound");
        }

        let mut last_message_id = None;
        for chunk in split_text_chunks(&message.text, MAX_MESSAGE_LENGTH) {
            last_message_id = Some(self.send_text(&message.route, &chunk)?);
        }
        Ok(PlatformSendOutcome {
            message_id: last_message_id,
        })
    }

    fn stream_update(&self, update: OutboundStreamUpdate) -> ChannelResult<PlatformSendOutcome> {
        if update.mode != StreamDeliveryMode::Edit {
            return Err(ChannelError::unavailable(format!(
                "{:?} streaming update is unavailable for telegram",
                update.mode
            )));
        }
        let message_id = update
            .message_id
            .parse::<i64>()
            .map_err(|_| ChannelError::retryable("telegram edit requires numeric message_id"))?;
        let response = telegram_post(
            &self.base_url,
            &self.token,
            "editMessageText",
            json!({
                "chat_id": update.route.chat_id,
                "message_id": message_id,
                "text": update.text
            }),
            API_TIMEOUT_MS,
        )?;
        classify_telegram_response(&response)?;
        Ok(PlatformSendOutcome {
            message_id: Some(message_id.to_string()),
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "telegram Bot API {media_kind} media delivery is explicitly unavailable in flyflor-cli"
        )))
    }
}

fn telegram_post(
    base_url: &str,
    token: &str,
    method: &str,
    payload: Value,
    timeout_ms: u64,
) -> ChannelResult<Value> {
    let url = format!("{}/bot{token}/{method}", base_url.trim_end_matches('/'));
    let body =
        serde_json::to_string(&payload).map_err(|error| ChannelError::fatal(error.to_string()))?;
    let output = Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            &seconds_arg(timeout_ms),
            "-X",
            "POST",
            &url,
            "-H",
            "Content-Type: application/json",
            "--data",
            &body,
        ])
        .output()
        .map_err(|error| ChannelError::unavailable(format!("curl is required: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ChannelError::retryable(format!(
            "Telegram curl failed with status {}: {}",
            output.status,
            stderr.trim()
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim()).map_err(|error| {
        ChannelError::retryable(format!(
            "Telegram returned invalid JSON: {error}; body={}",
            stdout.trim()
        ))
    })
}

fn classify_telegram_response(value: &Value) -> ChannelResult<()> {
    if value.get("ok").and_then(Value::as_bool) == Some(true) {
        return Ok(());
    }
    let code = value
        .get("error_code")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let description = value
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("unknown error");
    match code {
        401 | 403 => Err(ChannelError::session_expired(format!(
            "Telegram authorization failed: error_code={code} description={description}"
        ))),
        429 => Err(ChannelError::rate_limited(format!(
            "Telegram rate limited: error_code={code} description={description}"
        ))),
        400 => Err(ChannelError::fatal(format!(
            "Telegram bad request: error_code={code} description={description}"
        ))),
        _ => Err(ChannelError::retryable(format!(
            "Telegram error: error_code={code} description={description}"
        ))),
    }
}

fn display_name(from: &Value) -> String {
    let first = from.get("first_name").and_then(Value::as_str).unwrap_or("");
    let last = from.get("last_name").and_then(Value::as_str).unwrap_or("");
    let name = format!("{first} {last}").trim().to_string();
    if !name.is_empty() {
        return name;
    }
    from.get("username")
        .and_then(Value::as_str)
        .unwrap_or("telegram-user")
        .to_string()
}

fn media_notice(message: &Value) -> Option<String> {
    for (key, label) in [
        ("photo", "photo"),
        ("document", "document"),
        ("audio", "audio"),
        ("voice", "voice"),
        ("video", "video"),
    ] {
        if message.get(key).is_some() {
            return Some(format!(
                "[telegram {label} media unavailable: flyflor-cli does not download Bot API media yet]"
            ));
        }
    }
    None
}

fn outbound_mentions_media(text: &str, metadata: Option<&Value>) -> bool {
    if text.contains("MEDIA:") {
        return true;
    }
    metadata
        .and_then(|value| value.get("media"))
        .is_some_and(|media| !media.is_null())
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
    match env::var(name)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => default,
    }
}

fn env_i64(name: &str, default: i64) -> i64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(default)
}

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
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

struct TtlDedup {
    ttl: Duration,
    max_size: usize,
    seen: HashMap<String, Instant>,
}

impl TtlDedup {
    fn new(ttl_seconds: u64, max_size: usize) -> Self {
        Self {
            ttl: Duration::from_secs(ttl_seconds),
            max_size,
            seen: HashMap::new(),
        }
    }

    fn is_duplicate(&mut self, key: &str) -> bool {
        let now = Instant::now();
        self.seen.retain(|_, at| now.duration_since(*at) < self.ttl);
        if self.seen.contains_key(key) {
            return true;
        }
        self.seen.insert(key.to_string(), now);
        if self.seen.len() > self.max_size {
            let mut entries = self
                .seen
                .iter()
                .map(|(key, at)| (key.clone(), *at))
                .collect::<Vec<_>>();
            entries.sort_by_key(|(_, at)| *at);
            let remove_count = entries.len().saturating_sub(self.max_size);
            for (key, _) in entries.into_iter().take(remove_count) {
                self.seen.remove(&key);
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_auth_rate_limit_and_bad_request() {
        assert_eq!(
            classify_telegram_response(&json!({
                "ok": false,
                "error_code": 401,
                "description": "Unauthorized"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::SessionExpired
        );
        assert_eq!(
            classify_telegram_response(&json!({
                "ok": false,
                "error_code": 429,
                "description": "Too Many Requests"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::RateLimited
        );
        assert_eq!(
            classify_telegram_response(&json!({
                "ok": false,
                "error_code": 400,
                "description": "Bad Request"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::Fatal
        );
    }

    #[test]
    fn normalizes_text_media_and_deduplicates_updates() {
        let adapter = test_adapter();
        let normalized = adapter
            .normalize_update(&json!({
                "update_id": 42,
                "message": {
                    "message_id": 7,
                    "text": "hi",
                    "from": {
                        "id": 1001,
                        "first_name": "Ada",
                        "last_name": "Lovelace",
                        "username": "ada"
                    },
                    "chat": {
                        "id": -2002,
                        "type": "supergroup"
                    }
                }
            }))
            .unwrap();

        assert_eq!(normalized.id, "telegram-42-7");
        assert_eq!(normalized.text, "hi");
        assert_eq!(normalized.route.platform, "telegram");
        assert_eq!(normalized.route.chat_id, "-2002");
        assert_eq!(normalized.route.chat_type, ChatType::Group);
        assert_eq!(normalized.route.user_id, "1001");
        assert_eq!(normalized.route.display_name, "Ada Lovelace");
        assert_eq!(
            normalized
                .metadata
                .get("channel")
                .and_then(|channel| channel.get("sourceMessageId"))
                .and_then(Value::as_str),
            Some("-2002:7")
        );
        assert!(
            adapter
                .normalize_update(&json!({
                    "update_id": 42,
                    "message": {
                        "message_id": 7,
                        "text": "hi",
                        "from": { "id": 1001 },
                        "chat": { "id": -2002, "type": "supergroup" }
                    }
                }))
                .is_none()
        );

        let media = adapter
            .normalize_update(&json!({
                "update_id": 43,
                "message": {
                    "message_id": 8,
                    "photo": [{ "file_id": "p1" }],
                    "from": { "id": 1001, "username": "ada" },
                    "chat": { "id": 1001, "type": "private" }
                }
            }))
            .unwrap();
        assert!(media.text.contains("media unavailable"));
        assert_eq!(media.route.chat_type, ChatType::Direct);
    }

    #[test]
    fn allowlist_blocks_unknown_users_when_all_is_disabled() {
        let mut adapter = test_adapter();
        adapter.allow_all_users = false;
        adapter.allowed_users = HashSet::from(["1001".to_string()]);

        assert!(
            adapter
                .normalize_update(&json!({
                    "update_id": 1,
                    "message": {
                        "message_id": 1,
                        "text": "allowed",
                        "from": { "id": 1001 },
                        "chat": { "id": 1001, "type": "private" }
                    }
                }))
                .is_some()
        );
        assert!(
            adapter
                .normalize_update(&json!({
                    "update_id": 2,
                    "message": {
                        "message_id": 2,
                        "text": "blocked",
                        "from": { "id": 9999 },
                        "chat": { "id": 9999, "type": "private" }
                    }
                }))
                .is_none()
        );
    }

    #[test]
    fn split_text_preserves_unicode_chunks() {
        assert_eq!(split_text_chunks("你好世界", 2), vec!["你好", "世界"]);
    }

    fn test_adapter() -> TelegramBotAdapter {
        TelegramBotAdapter {
            token: "token".to_string(),
            base_url: TELEGRAM_API_BASE_URL.to_string(),
            allowed_users: HashSet::new(),
            allow_all_users: true,
            offset: Mutex::new(0),
            dedup: Mutex::new(TtlDedup::new(300, 100)),
        }
    }
}
