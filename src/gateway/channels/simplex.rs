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

const SIMPLEX_TIMEOUT_MS: u64 = 15_000;
const SIMPLEX_TEXT_LIMIT: usize = 16_000;
const CORR_PREFIX: &str = "flyflor-simplex-";

pub struct SimplexAdapter {
    ws_url: String,
    allowed_users: HashSet<String>,
    allow_all_users: bool,
    home_channel: Option<String>,
    seen_messages: Mutex<HashSet<String>>,
}

impl SimplexAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let ws_url = env_first(&["SIMPLEX_WS_URL", "FLYFLOR_SIMPLEX_WS_URL"]);
        if ws_url.is_empty() {
            return Err(ChannelError::missing_config(
                "SIMPLEX_WS_URL is required for the simplex channel",
            ));
        }
        Ok(Self {
            ws_url,
            allowed_users: env_set_any(&["SIMPLEX_ALLOWED_USERS", "SIMPLEX_ALLOW_FROM"]),
            allow_all_users: env_bool_any(&["SIMPLEX_ALLOW_ALL_USERS"]),
            home_channel: env_optional(&["SIMPLEX_HOME_CHANNEL"]),
            seen_messages: Mutex::new(HashSet::new()),
        })
    }

    fn normalize_event(&self, value: &Value) -> Option<NormalizedInboundMessage> {
        if own_corr_id(value) {
            return None;
        }
        let response_type = value_string_any(value, &["type"])
            .or_else(|| {
                value
                    .get("resp")
                    .and_then(|resp| value_string_any(resp, &["type"]))
            })
            .unwrap_or_else(|| "newChatItem".to_string());
        if response_type == "newChatItems" {
            return value
                .get("chatItems")
                .and_then(Value::as_array)
                .and_then(|items| items.iter().find_map(|item| self.normalize_chat_item(item)));
        }
        if response_type != "newChatItem" && response_type != "message" {
            return None;
        }
        self.normalize_chat_item(value)
    }

    fn normalize_chat_item(&self, wrapper: &Value) -> Option<NormalizedInboundMessage> {
        let chat_info = wrapper
            .get("chatInfo")
            .or_else(|| wrapper.get("chat"))
            .or_else(|| wrapper.get("conversation"))
            .unwrap_or(wrapper);
        let chat_item = wrapper
            .get("chatItem")
            .or_else(|| wrapper.get("item"))
            .or_else(|| wrapper.get("message"))
            .unwrap_or(wrapper);
        if sent_by_self(chat_item) {
            return None;
        }
        let content = chat_item
            .get("content")
            .and_then(|content| content.get("msgContent"))
            .or_else(|| chat_item.get("msgContent"))
            .or_else(|| chat_item.get("content"))
            .unwrap_or(chat_item);
        let text = value_string_any(content, &["text", "body", "message", "content"])
            .or_else(|| value_string_any(chat_item, &["text", "body", "message"]))
            .map(|text| text.trim().to_string())?;
        if text.is_empty() {
            return None;
        }
        let chat_type_raw = value_string_any(chat_info, &["type", "chatType"])
            .unwrap_or_else(|| "direct".to_string())
            .to_ascii_lowercase();
        let group_info = chat_info
            .get("groupInfo")
            .or_else(|| chat_info.get("group"))
            .unwrap_or(chat_info);
        let contact_info = chat_info
            .get("contact")
            .or_else(|| chat_info.get("contactInfo"))
            .unwrap_or(chat_info);
        let is_group = matches!(chat_type_raw.as_str(), "group" | "groupinfo" | "group_chat")
            || group_info.get("groupId").is_some();
        let (chat_id, chat_name) = if is_group {
            let group_id = value_string_any(group_info, &["groupId", "id", "chatId"])?;
            let name = value_string_any(
                group_info,
                &["displayName", "localDisplayName", "name", "fullName"],
            )
            .or_else(|| {
                group_info
                    .get("groupProfile")
                    .and_then(|profile| value_string_any(profile, &["displayName", "name"]))
            })
            .unwrap_or_else(|| group_id.clone());
            (format!("group:{group_id}"), name)
        } else {
            let contact_id = value_string_any(contact_info, &["contactId", "id", "chatId"])?;
            let name = value_string_any(
                contact_info,
                &["displayName", "localDisplayName", "name", "fullName"],
            )
            .unwrap_or_else(|| contact_id.clone());
            (contact_id, name)
        };
        let member = chat_item
            .get("chatItemMember")
            .or_else(|| chat_item.get("member"))
            .unwrap_or(chat_item);
        let user_id = if is_group {
            value_string_any(member, &["memberId", "id", "contactId"])
                .unwrap_or_else(|| chat_id.clone())
        } else {
            chat_id.clone()
        };
        if !self.allow_all_users
            && !self.allowed_users.is_empty()
            && !self.allowed_users.contains(&user_id)
            && !self.allowed_users.contains(&chat_id)
        {
            return None;
        }
        let display_name = value_string_any(
            member,
            &["displayName", "localDisplayName", "name", "fullName"],
        )
        .unwrap_or_else(|| chat_name.clone());
        let source_message_id = value_string_any(
            chat_item,
            &["chatItemId", "itemId", "msgId", "messageId", "id"],
        )
        .or_else(|| value_string_any(wrapper, &["chatItemId", "itemId", "messageId", "id"]))
        .unwrap_or_else(|| format!("simplex-{}", now_millis()));
        if self.mark_seen(&source_message_id) {
            return None;
        }
        let route = MessageRoute {
            platform: "simplex".to_string(),
            chat_id: chat_id.clone(),
            chat_type: if is_group {
                ChatType::Group
            } else {
                ChatType::Direct
            },
            user_id: user_id.clone(),
            display_name,
            thread_id: chat_id.clone(),
        };
        let metadata = json!({
            "channel": {
                "platform": "simplex",
                "adapter": "simplex-chat-websocket",
                "chatId": route.chat_id,
                "chatType": route.chat_type.as_gateway_str(),
                "homeChannel": self.home_channel,
                "sourceMessageId": source_message_id,
                "userId": user_id
            }
        });
        Some(NormalizedInboundMessage {
            id: format!("simplex-{source_message_id}"),
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

impl PlatformAdapter for SimplexAdapter {
    fn name(&self) -> &'static str {
        "simplex"
    }

    fn capabilities(&self) -> ChannelCapabilityReport {
        ChannelCapabilityReport {
            send: ChannelCapabilityState::Available,
            typing: ChannelCapabilityState::Unavailable,
            edit: ChannelCapabilityState::Unavailable,
            draft: ChannelCapabilityState::Unavailable,
            card: ChannelCapabilityState::Unavailable,
            media: ChannelCapabilityState::Degraded,
        }
    }

    fn poll_updates(&self) -> ChannelResult<Vec<NormalizedInboundMessage>> {
        let raw = env_first(&["SIMPLEX_INBOUND_EVENT", "SIMPLEX_INBOUND_MESSAGE"]);
        if raw.is_empty() {
            return Ok(Vec::new());
        }
        let value = serde_json::from_str::<Value>(&raw).map_err(|error| {
            ChannelError::fatal(format!("simplex inbound JSON is invalid: {error}"))
        })?;
        Ok(self.normalize_event(&value).into_iter().collect())
    }

    fn send_typing(&self, _route: &MessageRoute) -> ChannelResult<()> {
        Err(ChannelError::unavailable(
            "simplex typing is unavailable in the simplex-chat daemon websocket API",
        ))
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal(
                "simplex message text must not be empty",
            ));
        }
        if outbound_mentions_media(&message.text, message.metadata.as_ref()) {
            return self.send_media_unavailable("outbound");
        }
        let mut last_id = None;
        for chunk in split_text_chunks(&message.text, SIMPLEX_TEXT_LIMIT) {
            let corr_id = format!("{CORR_PREFIX}{}", now_millis());
            let command = simplex_command(&message.route.chat_id, &chunk);
            let response = send_simplex_ws(
                &self.ws_url,
                json!({
                    "corrId": corr_id,
                    "cmd": command
                }),
            )?;
            classify_simplex_response(&response)?;
            last_id = value_string_any(&response, &["id", "messageId", "chatItemId", "corrId"])
                .or_else(|| Some(format!("simplex-{}", now_millis())));
        }
        Ok(PlatformSendOutcome {
            message_id: last_id,
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "simplex {media_kind} media delivery is explicitly unavailable in flyflor-cli"
        )))
    }
}

fn simplex_command(chat_id: &str, text: &str) -> String {
    if let Some(group_id) = chat_id.strip_prefix("group:") {
        format!("#[{group_id}] {text}")
    } else {
        format!("@[{chat_id}] {text}")
    }
}

fn send_simplex_ws(url: &str, payload: Value) -> ChannelResult<Value> {
    let (mut socket, _) = connect(url).map_err(|error| {
        ChannelError::retryable(format!("simplex websocket connect failed: {error}"))
    })?;
    configure_socket_timeout(&mut socket);
    socket
        .send(Message::text(payload.to_string()))
        .map_err(|error| {
            ChannelError::retryable(format!("simplex websocket send failed: {error}"))
        })?;
    match socket.read() {
        Ok(Message::Text(text)) => parse_response_json(text.as_ref()),
        Ok(Message::Binary(bytes)) => parse_response_json(&String::from_utf8_lossy(&bytes)),
        Ok(_) => Ok(json!({ "ok": true })),
        Err(error) => Err(ChannelError::retryable(format!(
            "simplex websocket response failed: {error}"
        ))),
    }
}

fn configure_socket_timeout(socket: &mut tungstenite::WebSocket<MaybeTlsStream<TcpStream>>) {
    if let MaybeTlsStream::Plain(stream) = socket.get_mut() {
        let _ = stream.set_read_timeout(Some(Duration::from_millis(SIMPLEX_TIMEOUT_MS)));
        let _ = stream.set_write_timeout(Some(Duration::from_millis(SIMPLEX_TIMEOUT_MS)));
    }
}

fn parse_response_json(text: &str) -> ChannelResult<Value> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(json!({ "ok": true }));
    }
    serde_json::from_str::<Value>(trimmed).map_err(|error| {
        ChannelError::retryable(format!("simplex websocket returned invalid JSON: {error}"))
    })
}

fn classify_simplex_response(value: &Value) -> ChannelResult<()> {
    if value
        .get("ok")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| value.get("error").is_none())
    {
        return Ok(());
    }
    let message = value
        .get("error")
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("unknown simplex error");
    Err(ChannelError::retryable(format!("SimpleX error: {message}")))
}

fn sent_by_self(chat_item: &Value) -> bool {
    let status_type = chat_item
        .get("meta")
        .and_then(|meta| meta.get("itemStatus"))
        .and_then(|status| value_string_any(status, &["type"]))
        .or_else(|| value_string_any(chat_item, &["direction", "itemStatus"]))
        .unwrap_or_default();
    matches!(
        status_type.as_str(),
        "sndSent" | "sndSentDirect" | "sndSentViaProxy" | "sndNew" | "sent" | "outgoing"
    )
}

fn own_corr_id(value: &Value) -> bool {
    value_string_any(value, &["corrId", "correlationId"])
        .is_some_and(|id| id.starts_with(CORR_PREFIX) || id.starts_with("hermes-"))
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

fn env_bool_any(names: &[&str]) -> bool {
    names.iter().any(|name| {
        env::var(name).is_ok_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "y" | "on"
            )
        })
    })
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
    fn normalizes_direct_new_chat_item() {
        let adapter = test_adapter();
        let message = adapter
            .normalize_event(&json!({
                "type": "newChatItem",
                "chatInfo": {
                    "type": "direct",
                    "contact": {
                        "contactId": "contact-42",
                        "displayName": "Ada"
                    }
                },
                "chatItem": {
                    "chatItemId": "item-1",
                    "content": {
                        "msgContent": {
                            "text": "hello simplex"
                        }
                    },
                    "meta": {
                        "itemStatus": { "type": "rcvRead" }
                    }
                }
            }))
            .unwrap();

        assert_eq!(message.id, "simplex-item-1");
        assert_eq!(message.text, "hello simplex");
        assert_eq!(message.route.platform, "simplex");
        assert_eq!(message.route.chat_id, "contact-42");
        assert_eq!(message.route.chat_type, ChatType::Direct);
        assert_eq!(message.route.user_id, "contact-42");
        assert_eq!(message.metadata["channel"]["sourceMessageId"], "item-1");
    }

    #[test]
    fn normalizes_group_member_message() {
        let adapter = test_adapter();
        let message = adapter
            .normalize_event(&json!({
                "type": "newChatItem",
                "chatInfo": {
                    "type": "group",
                    "groupInfo": {
                        "groupId": "grp-99",
                        "displayName": "Ops"
                    }
                },
                "chatItem": {
                    "id": "item-2",
                    "chatItemMember": {
                        "memberId": "member-1",
                        "displayName": "Grace"
                    },
                    "content": {
                        "msgContent": {
                            "text": "hello group"
                        }
                    }
                }
            }))
            .unwrap();

        assert_eq!(message.route.chat_id, "group:grp-99");
        assert_eq!(message.route.chat_type, ChatType::Group);
        assert_eq!(message.route.user_id, "member-1");
        assert_eq!(message.route.display_name, "Grace");
    }

    #[test]
    fn filters_own_echo_and_seen_messages() {
        let adapter = test_adapter();
        assert!(
            adapter
                .normalize_event(&json!({
                    "corrId": "flyflor-simplex-1",
                    "type": "newChatItem"
                }))
                .is_none()
        );
        let event = json!({
            "chatInfo": { "contact": { "contactId": "contact-42" } },
            "chatItem": { "id": "dup", "content": { "msgContent": { "text": "hi" } } }
        });
        assert!(adapter.normalize_event(&event).is_some());
        assert!(adapter.normalize_event(&event).is_none());
    }

    #[test]
    fn allowlist_blocks_unknown_sender() {
        let mut adapter = test_adapter();
        adapter.allowed_users = HashSet::from(["allowed".to_string()]);
        assert!(
            adapter
                .normalize_event(&json!({
                    "chatInfo": { "contact": { "contactId": "blocked" } },
                    "chatItem": { "id": "item-1", "content": { "msgContent": { "text": "blocked" } } }
                }))
                .is_none()
        );
    }

    #[test]
    fn builds_simplex_commands_and_chunks_unicode() {
        assert_eq!(
            simplex_command("contact-42", "Hello"),
            "@[contact-42] Hello"
        );
        assert_eq!(simplex_command("group:grp-99", "Hello"), "#[grp-99] Hello");
        assert_eq!(split_text_chunks("你好世界", 2), vec!["你好", "世界"]);
    }

    #[test]
    fn classifies_response_errors() {
        assert!(classify_simplex_response(&json!({ "ok": true })).is_ok());
        assert_eq!(
            classify_simplex_response(&json!({ "ok": false, "error": "daemon busy" }))
                .unwrap_err()
                .kind,
            ChannelErrorKind::Retryable
        );
    }

    fn test_adapter() -> SimplexAdapter {
        SimplexAdapter {
            ws_url: "ws://127.0.0.1:5225".to_string(),
            allowed_users: HashSet::new(),
            allow_all_users: false,
            home_channel: None,
            seen_messages: Mutex::new(HashSet::new()),
        }
    }
}
