use std::{
    collections::HashSet,
    env,
    net::TcpStream,
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};
use tungstenite::{Message, connect, stream::MaybeTlsStream};

use super::platform::{
    ChannelCapabilityReport, ChannelCapabilityState, ChannelError, ChannelErrorKind, ChannelResult,
    ChatType, MessageRoute, NormalizedInboundMessage, OutboundMessage, PlatformAdapter,
    PlatformSendOutcome,
};

const DEFAULT_WECOM_WEBSOCKET_URL: &str = "wss://openws.work.weixin.qq.com";
const WECOM_TEXT_LIMIT: usize = 4_000;
const WECOM_TIMEOUT_MS: u64 = 15_000;

pub struct WeComAdapter {
    bot_id: String,
    secret: String,
    websocket_url: String,
    allowed_users: HashSet<String>,
    allowed_groups: HashSet<String>,
    seen_messages: Mutex<HashSet<String>>,
}

impl WeComAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let bot_id = env_first(&["WECOM_BOT_ID", "FLYFLOR_WECOM_BOT_ID"]);
        if bot_id.is_empty() {
            return Err(ChannelError::missing_config(
                "WECOM_BOT_ID is required for the wecom channel",
            ));
        }
        let secret = env_first(&["WECOM_SECRET", "FLYFLOR_WECOM_SECRET"]);
        if secret.is_empty() {
            return Err(ChannelError::missing_config(
                "WECOM_SECRET is required for the wecom channel",
            ));
        }
        Ok(Self {
            bot_id,
            secret,
            websocket_url: env_first(&["WECOM_WEBSOCKET_URL", "FLYFLOR_WECOM_WEBSOCKET_URL"])
                .if_empty(DEFAULT_WECOM_WEBSOCKET_URL),
            allowed_users: env_set_any(&["WECOM_ALLOWED_USERS", "WECOM_ALLOW_FROM"]),
            allowed_groups: env_set_any(&["WECOM_ALLOWED_GROUPS", "WECOM_GROUP_ALLOW_FROM"]),
            seen_messages: Mutex::new(HashSet::new()),
        })
    }

    fn normalize_event(&self, value: &Value) -> Option<NormalizedInboundMessage> {
        let body = value
            .get("body")
            .or_else(|| value.get("payload"))
            .or_else(|| value.get("event"))
            .unwrap_or(value);
        let command = value_string_any(value, &["cmd", "command"])
            .unwrap_or_else(|| "aibot_msg_callback".to_string());
        if command != "aibot_msg_callback" && command != "aibot_callback" {
            return None;
        }
        let sender = body
            .get("from")
            .filter(|value| value.is_object())
            .unwrap_or(body);
        let user_id =
            value_string_any(sender, &["userid", "userId", "user_id"]).unwrap_or_default();
        if !self.allowed_users.is_empty() && !self.allowed_users.contains(&user_id) {
            return None;
        }
        let chat_id = value_string_any(body, &["chatid", "chatId", "chat_id"])
            .or_else(|| value_string_any(body, &["touser", "toUser"]))
            .unwrap_or_else(|| user_id.clone());
        if chat_id.is_empty() {
            return None;
        }
        let is_group = value_string_any(body, &["chattype", "chatType", "chat_type"])
            .map(|value| matches!(value.to_lowercase().as_str(), "group" | "group_chat"))
            .unwrap_or(false);
        if is_group && !self.allowed_groups.is_empty() && !self.allowed_groups.contains(&chat_id) {
            return None;
        }
        let mut text = extract_text(body)?.trim().to_string();
        if is_group {
            text = strip_leading_mention(&text);
        }
        if text.is_empty() {
            return None;
        }
        let message_id = value_string_any(body, &["msgid", "msgId", "messageId", "id"])
            .unwrap_or_else(|| format!("wecom-{}", now_millis()));
        if self.mark_seen(&message_id) {
            return None;
        }
        let reply_req_id = value
            .get("headers")
            .and_then(|headers| value_string_any(headers, &["req_id", "reqId"]))
            .unwrap_or_default();
        let route = MessageRoute {
            platform: "wecom".to_string(),
            chat_id: chat_id.clone(),
            chat_type: if is_group {
                ChatType::Group
            } else {
                ChatType::Direct
            },
            user_id: if user_id.is_empty() {
                chat_id.clone()
            } else {
                user_id.clone()
            },
            display_name: value_string_any(sender, &["name", "displayName"])
                .unwrap_or_else(|| user_id.if_empty(&chat_id)),
            thread_id: chat_id.clone(),
        };
        let metadata = json!({
            "channel": {
                "platform": "wecom",
                "adapter": "wecom-ai-bot-websocket",
                "botId": self.bot_id,
                "chatId": route.chat_id,
                "chatType": route.chat_type.as_gateway_str(),
                "msgType": value_string_any(body, &["msgtype", "msgType"]).unwrap_or_else(|| "text".to_string()),
                "replyReqId": reply_req_id,
                "sourceMessageId": message_id,
                "userId": route.user_id
            }
        });
        Some(NormalizedInboundMessage {
            id: format!("wecom-{message_id}"),
            text,
            route,
            context: None,
            metadata,
        })
    }

    fn mark_seen(&self, message_id: &str) -> bool {
        let Ok(mut seen) = self.seen_messages.lock() else {
            return false;
        };
        if seen.contains(message_id) {
            true
        } else {
            seen.insert(message_id.to_string());
            false
        }
    }
}

impl PlatformAdapter for WeComAdapter {
    fn name(&self) -> &'static str {
        "wecom"
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
        let raw = env_first(&["WECOM_INBOUND_EVENT", "WECOM_INBOUND_MESSAGE"]);
        if raw.is_empty() {
            return Ok(Vec::new());
        }
        let value = serde_json::from_str::<Value>(&raw).map_err(|error| {
            ChannelError::fatal(format!("wecom inbound JSON is invalid: {error}"))
        })?;
        Ok(self.normalize_event(&value).into_iter().collect())
    }

    fn send_typing(&self, _route: &MessageRoute) -> ChannelResult<()> {
        Err(ChannelError::unavailable(
            "wecom typing is unavailable in the current AI Bot text adapter",
        ))
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal("wecom message text must not be empty"));
        }
        if outbound_mentions_media(&message.text, message.metadata.as_ref()) {
            return self.send_media_unavailable("outbound");
        }
        let reply_req_id = message
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("channel"))
            .and_then(|channel| value_string_any(channel, &["replyReqId", "req_id"]));
        let mut last_id = None;
        for chunk in split_text_chunks(&message.text, WECOM_TEXT_LIMIT) {
            let req_id = format!("flyflor-wecom-{}", now_millis());
            let payload = if let Some(reply_req_id) =
                reply_req_id.clone().filter(|value| !value.is_empty())
            {
                json!({
                    "cmd": "aibot_respond_msg",
                    "headers": { "req_id": req_id, "reply_req_id": reply_req_id },
                    "body": {
                        "msgtype": "markdown",
                        "markdown": { "content": chunk }
                    }
                })
            } else {
                json!({
                    "cmd": "aibot_send_msg",
                    "headers": { "req_id": req_id },
                    "body": {
                        "bot_id": self.bot_id,
                        "chatid": message.route.chat_id,
                        "msgtype": "markdown",
                        "markdown": { "content": chunk }
                    }
                })
            };
            let response = send_wecom_ws(&self.websocket_url, payload)?;
            classify_wecom_response(&response)?;
            last_id = value_string_any(&response, &["msgid", "messageId", "id"]).or_else(|| {
                response
                    .get("headers")
                    .and_then(|headers| value_string_any(headers, &["req_id", "reqId"]))
            });
        }
        Ok(PlatformSendOutcome {
            message_id: last_id,
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "wecom {media_kind} media delivery is explicitly unavailable in flyflor-cli"
        )))
    }
}

fn send_wecom_ws(url: &str, payload: Value) -> ChannelResult<Value> {
    let (mut socket, _) = connect(url).map_err(|error| {
        ChannelError::retryable(format!("wecom websocket connect failed: {error}"))
    })?;
    configure_socket_timeout(&mut socket);
    socket
        .send(Message::text(payload.to_string()))
        .map_err(|error| {
            ChannelError::retryable(format!("wecom websocket send failed: {error}"))
        })?;
    match socket.read() {
        Ok(Message::Text(text)) => parse_response_json(text.as_ref()),
        Ok(Message::Binary(bytes)) => parse_response_json(&String::from_utf8_lossy(&bytes)),
        Ok(_) => Ok(json!({ "errcode": 0 })),
        Err(error) => Err(ChannelError::retryable(format!(
            "wecom websocket response failed: {error}"
        ))),
    }
}

fn configure_socket_timeout(socket: &mut tungstenite::WebSocket<MaybeTlsStream<TcpStream>>) {
    if let MaybeTlsStream::Plain(stream) = socket.get_mut() {
        let _ = stream.set_read_timeout(Some(Duration::from_millis(WECOM_TIMEOUT_MS)));
        let _ = stream.set_write_timeout(Some(Duration::from_millis(WECOM_TIMEOUT_MS)));
    }
}

fn parse_response_json(text: &str) -> ChannelResult<Value> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(json!({ "errcode": 0 }));
    }
    serde_json::from_str::<Value>(trimmed).map_err(|error| {
        ChannelError::retryable(format!("wecom websocket returned invalid JSON: {error}"))
    })
}

fn classify_wecom_response(value: &Value) -> ChannelResult<()> {
    let code = value
        .get("errcode")
        .or_else(|| value.get("errCode"))
        .and_then(|value| value.as_i64().or_else(|| value.as_str()?.parse().ok()))
        .unwrap_or(0);
    if code == 0 {
        return Ok(());
    }
    let message = value
        .get("errmsg")
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("unknown wecom error");
    match code {
        40001 | 40013 | 40014 | 42001 => Err(ChannelError::session_expired(format!(
            "WeCom authorization failed: code={code} message={message}"
        ))),
        45009 | 60020 | 600039 => Err(ChannelError::rate_limited(format!(
            "WeCom rate limited or temporarily unsupported: code={code} message={message}"
        ))),
        40003 | 40004 | 40058 => Err(ChannelError::fatal(format!(
            "WeCom bad request: code={code} message={message}"
        ))),
        _ => Err(ChannelError::retryable(format!(
            "WeCom error: code={code} message={message}"
        ))),
    }
}

fn extract_text(body: &Value) -> Option<String> {
    if let Some(text) = body
        .get("text")
        .and_then(|value| value.get("content"))
        .and_then(Value::as_str)
    {
        return Some(text.to_string());
    }
    if let Some(content) = value_string_any(body, &["content", "text"]) {
        return Some(content);
    }
    if let Some(items) = body
        .get("mixed")
        .and_then(|mixed| mixed.get("msg_item"))
        .and_then(Value::as_array)
    {
        let parts = items
            .iter()
            .filter(|item| {
                value_string_any(item, &["msgtype", "msgType"])
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("text"))
            })
            .filter_map(|item| item.get("text")?.get("content")?.as_str())
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        if !parts.is_empty() {
            return Some(parts.join("\n"));
        }
    }
    body.get("voice")
        .and_then(|voice| voice.get("content"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn strip_leading_mention(text: &str) -> String {
    let trimmed = text.trim();
    if !trimmed.starts_with('@') {
        return trimmed.to_string();
    }
    trimmed
        .split_once(char::is_whitespace)
        .map(|(_, rest)| rest.trim().to_string())
        .unwrap_or_default()
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

fn env_first(names: &[&str]) -> String {
    names
        .iter()
        .find_map(|name| env::var(name).ok())
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_default()
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
    fn normalizes_wecom_ai_bot_callback() {
        let adapter = test_adapter();
        let message = adapter
            .normalize_event(&json!({
                "cmd": "aibot_msg_callback",
                "headers": { "req_id": "req-1" },
                "body": {
                    "chatid": "group-1",
                    "chattype": "group",
                    "from": { "userid": "user-1" },
                    "msgid": "msg-1",
                    "msgtype": "text",
                    "text": { "content": "@Flyflor hello wecom" }
                }
            }))
            .unwrap();

        assert_eq!(message.id, "wecom-msg-1");
        assert_eq!(message.text, "hello wecom");
        assert_eq!(message.route.platform, "wecom");
        assert_eq!(message.route.chat_id, "group-1");
        assert_eq!(message.route.chat_type, ChatType::Group);
        assert_eq!(message.metadata["channel"]["replyReqId"], "req-1");
        assert_eq!(message.metadata["channel"]["sourceMessageId"], "msg-1");
    }

    #[test]
    fn allowlists_block_unknown_sender_and_group() {
        let mut adapter = test_adapter();
        adapter.allowed_users = HashSet::from(["allowed-user".to_string()]);
        adapter.allowed_groups = HashSet::from(["allowed-group".to_string()]);

        assert!(
            adapter
                .normalize_event(&json!({
                    "cmd": "aibot_msg_callback",
                    "body": {
                        "chatid": "allowed-group",
                        "chattype": "group",
                        "from": { "userid": "blocked-user" },
                        "msgid": "msg-1",
                        "text": { "content": "blocked" }
                    }
                }))
                .is_none()
        );
        assert!(
            adapter
                .normalize_event(&json!({
                    "cmd": "aibot_msg_callback",
                    "body": {
                        "chatid": "blocked-group",
                        "chattype": "group",
                        "from": { "userid": "allowed-user" },
                        "msgid": "msg-2",
                        "text": { "content": "blocked" }
                    }
                }))
                .is_none()
        );
    }

    #[test]
    fn classifies_wecom_error_codes() {
        assert_eq!(
            classify_wecom_response(&json!({ "errcode": 40013, "errmsg": "bad secret" }))
                .unwrap_err()
                .kind,
            ChannelErrorKind::SessionExpired
        );
        assert_eq!(
            classify_wecom_response(&json!({ "errcode": 600039, "errmsg": "unsupported" }))
                .unwrap_err()
                .kind,
            ChannelErrorKind::RateLimited
        );
        assert_eq!(
            classify_wecom_response(&json!({ "errcode": 40003, "errmsg": "bad user" }))
                .unwrap_err()
                .kind,
            ChannelErrorKind::Fatal
        );
    }

    #[test]
    fn mixed_and_unicode_text_helpers_work() {
        assert_eq!(
            extract_text(&json!({
                "msgtype": "mixed",
                "mixed": {
                    "msg_item": [
                        { "msgtype": "text", "text": { "content": "part1" } },
                        { "msgtype": "image", "image": { "url": "https://example.test/x.png" } },
                        { "msgtype": "text", "text": { "content": "part2" } }
                    ]
                }
            })),
            Some("part1\npart2".to_string())
        );
        assert_eq!(split_text_chunks("你好世界", 2), vec!["你好", "世界"]);
    }

    fn test_adapter() -> WeComAdapter {
        WeComAdapter {
            bot_id: "bot-1".to_string(),
            secret: "secret".to_string(),
            websocket_url: DEFAULT_WECOM_WEBSOCKET_URL.to_string(),
            allowed_users: HashSet::new(),
            allowed_groups: HashSet::new(),
            seen_messages: Mutex::new(HashSet::new()),
        }
    }
}
