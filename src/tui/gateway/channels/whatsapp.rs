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

const DEFAULT_WHATSAPP_GRAPH_BASE: &str = "https://graph.facebook.com";
const WHATSAPP_API_VERSION: &str = "v20.0";
const WHATSAPP_MAX_MESSAGE_LENGTH: usize = 3_900;
const WHATSAPP_TIMEOUT_MS: u64 = 15_000;

pub struct WhatsAppAdapter {
    graph_base: String,
    api_version: String,
    access_token: String,
    phone_number_id: String,
    business_account_id: Option<String>,
    allowed_users: HashSet<String>,
}

impl WhatsAppAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let access_token = env::var("WHATSAPP_ACCESS_TOKEN")
            .or_else(|_| env::var("WHATSAPP_TOKEN"))
            .or_else(|_| env::var("FLYFLOR_WHATSAPP_TOKEN"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if access_token.is_empty() {
            return Err(ChannelError::missing_config(
                "WHATSAPP_ACCESS_TOKEN is required for the whatsapp channel",
            ));
        }
        let phone_number_id = env::var("WHATSAPP_PHONE_NUMBER_ID")
            .or_else(|_| env::var("FLYFLOR_WHATSAPP_PHONE_NUMBER_ID"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if phone_number_id.is_empty() {
            return Err(ChannelError::missing_config(
                "WHATSAPP_PHONE_NUMBER_ID is required for the whatsapp channel",
            ));
        }
        Ok(Self {
            graph_base: env::var("WHATSAPP_GRAPH_BASE")
                .or_else(|_| env::var("FLYFLOR_WHATSAPP_GRAPH_BASE"))
                .unwrap_or_else(|_| DEFAULT_WHATSAPP_GRAPH_BASE.to_string())
                .trim()
                .trim_end_matches('/')
                .to_string(),
            api_version: env::var("WHATSAPP_API_VERSION")
                .or_else(|_| env::var("FLYFLOR_WHATSAPP_API_VERSION"))
                .unwrap_or_else(|_| WHATSAPP_API_VERSION.to_string())
                .trim()
                .trim_matches('/')
                .to_string(),
            access_token,
            phone_number_id,
            business_account_id: env::var("WHATSAPP_BUSINESS_ACCOUNT_ID")
                .or_else(|_| env::var("FLYFLOR_WHATSAPP_BUSINESS_ACCOUNT_ID"))
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            allowed_users: env_set("WHATSAPP_ALLOWED_USERS")
                .into_iter()
                .map(|phone| normalize_phone(&phone))
                .filter(|phone| !phone.is_empty())
                .collect(),
        })
    }

    fn messages_url(&self) -> String {
        format!(
            "{}/{}/{}/messages",
            self.graph_base,
            url_encode_path(&self.api_version),
            url_encode_path(&self.phone_number_id)
        )
    }

    fn normalize_webhook(&self, value: &Value) -> Vec<NormalizedInboundMessage> {
        let mut messages = Vec::new();
        collect_whatsapp_messages(value, &mut messages);
        messages
            .into_iter()
            .filter_map(|message| self.normalize_message(&message))
            .collect()
    }

    fn normalize_message(&self, value: &Value) -> Option<NormalizedInboundMessage> {
        let message_type = value_string(value, "type").unwrap_or_else(|| "text".to_string());
        if message_type != "text" {
            return None;
        }
        let from = normalize_phone(&value_string(value, "from")?);
        if from.is_empty() {
            return None;
        }
        if !self.allowed_users.is_empty() && !self.allowed_users.contains(&from) {
            return None;
        }
        let text = value
            .get("text")
            .and_then(|text| value_string(text, "body"))
            .or_else(|| value_string(value, "body"))?
            .trim()
            .to_string();
        if text.is_empty() {
            return None;
        }
        let source_id =
            value_string(value, "id").unwrap_or_else(|| format!("wamid.{}", now_millis()));
        let display_name = value_string(value, "profile_name")
            .or_else(|| value_string(value, "name"))
            .unwrap_or_else(|| from.clone());
        let route = MessageRoute {
            platform: "whatsapp".to_string(),
            chat_id: from.clone(),
            chat_type: ChatType::Direct,
            user_id: from.clone(),
            display_name,
            thread_id: from.clone(),
        };
        let metadata = json!({
            "channel": {
                "platform": "whatsapp",
                "adapter": "whatsapp-cloud-api",
                "chatId": from,
                "chatType": route.chat_type.as_gateway_str(),
                "userId": route.user_id,
                "sourceMessageId": source_id,
                "phoneNumberId": self.phone_number_id,
                "businessAccountId": self.business_account_id
            }
        });
        Some(NormalizedInboundMessage {
            id: format!("whatsapp-{source_id}"),
            text,
            route,
            context: None,
            metadata,
        })
    }
}

impl PlatformAdapter for WhatsAppAdapter {
    fn name(&self) -> &'static str {
        "whatsapp"
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
        let raw = env::var("WHATSAPP_INBOUND_WEBHOOK")
            .or_else(|_| env::var("FLYFLOR_WHATSAPP_INBOUND_WEBHOOK"))
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let Some(raw) = raw else {
            return Ok(Vec::new());
        };
        let value = serde_json::from_str::<Value>(&raw).map_err(|error| {
            ChannelError::fatal(format!("whatsapp inbound webhook JSON is invalid: {error}"))
        })?;
        Ok(self.normalize_webhook(&value))
    }

    fn send_typing(&self, _route: &MessageRoute) -> ChannelResult<()> {
        Err(ChannelError::unavailable(
            "whatsapp typing indicator is unavailable in the Cloud API adapter",
        ))
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal(
                "whatsapp message text must not be empty",
            ));
        }
        if outbound_mentions_media(&message.text, message.metadata.as_ref()) {
            return self.send_media_unavailable("outbound");
        }
        let to = normalize_phone(&message.route.chat_id);
        if to.is_empty() {
            return Err(ChannelError::fatal(
                "whatsapp destination must not be empty",
            ));
        }
        let mut last_id = None;
        for chunk in split_text_chunks(&message.text, WHATSAPP_MAX_MESSAGE_LENGTH) {
            let response = whatsapp_post(
                &self.messages_url(),
                &self.access_token,
                json!({
                    "messaging_product": "whatsapp",
                    "recipient_type": "individual",
                    "to": to,
                    "type": "text",
                    "text": {
                        "preview_url": false,
                        "body": chunk
                    }
                }),
            )?;
            classify_whatsapp_response(&response)?;
            last_id = response
                .get("messages")
                .and_then(Value::as_array)
                .and_then(|messages| messages.first())
                .and_then(|message| value_string(message, "id"))
                .or_else(|| Some(format!("whatsapp-{}", now_millis())));
        }
        Ok(PlatformSendOutcome {
            message_id: last_id,
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "whatsapp {media_kind} media delivery is explicitly unavailable in flyflor-cli"
        )))
    }
}

fn whatsapp_post(url: &str, token: &str, payload: Value) -> ChannelResult<Value> {
    let body =
        serde_json::to_string(&payload).map_err(|error| ChannelError::fatal(error.to_string()))?;
    let output = Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            &seconds_arg(WHATSAPP_TIMEOUT_MS),
            "-X",
            "POST",
            url,
            "-H",
            &format!("Authorization: Bearer {token}"),
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
                "whatsapp authorization failed: {message}"
            )));
        }
        if message.contains("429") {
            return Err(ChannelError::rate_limited(format!(
                "whatsapp rate limited: {message}"
            )));
        }
        return Err(ChannelError::retryable(format!(
            "whatsapp curl failed with status {}: {}",
            output.status, message
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim()).map_err(|error| {
        ChannelError::retryable(format!(
            "whatsapp returned invalid JSON: {error}; body={}",
            stdout.trim()
        ))
    })
}

fn classify_whatsapp_response(value: &Value) -> ChannelResult<()> {
    let error = value.get("error");
    let status = error
        .and_then(|error| {
            error
                .get("code")
                .or_else(|| error.get("status"))
                .or_else(|| error.get("status_code"))
        })
        .or_else(|| value.get("status_code"))
        .and_then(Value::as_i64);
    if error.is_none() && status.is_none_or(|status| status < 400) {
        return Ok(());
    }
    let status = status.unwrap_or_default();
    let message = error
        .and_then(|error| error.get("message"))
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("unknown whatsapp error");
    match status {
        0 | 400 | 404 => Err(ChannelError::fatal(format!(
            "WhatsApp bad request: status={status} message={message}"
        ))),
        401 | 403 | 190 => Err(ChannelError::session_expired(format!(
            "WhatsApp authorization failed: status={status} message={message}"
        ))),
        429 | 4 | 17 | 32 => Err(ChannelError::rate_limited(format!(
            "WhatsApp rate limited: status={status} message={message}"
        ))),
        _ => Err(ChannelError::retryable(format!(
            "WhatsApp error: status={status} message={message}"
        ))),
    }
}

fn collect_whatsapp_messages(value: &Value, output: &mut Vec<Value>) {
    if let Some(messages) = value.get("messages").and_then(Value::as_array) {
        for message in messages {
            output.push(message.clone());
        }
    }
    if let Some(entries) = value.get("entry").and_then(Value::as_array) {
        for entry in entries {
            if let Some(changes) = entry.get("changes").and_then(Value::as_array) {
                for change in changes {
                    let Some(change_value) = change.get("value") else {
                        continue;
                    };
                    let contacts = contact_names(change_value);
                    if let Some(messages) = change_value.get("messages").and_then(Value::as_array) {
                        for message in messages {
                            let mut message = message.clone();
                            if let Some(from) = value_string(&message, "from")
                                && let Some(name) = contacts.get(&normalize_phone(&from))
                                && let Some(object) = message.as_object_mut()
                            {
                                object.insert("profile_name".to_string(), json!(name));
                            }
                            output.push(message);
                        }
                    }
                }
            }
        }
    }
}

fn contact_names(value: &Value) -> std::collections::HashMap<String, String> {
    let mut names = std::collections::HashMap::new();
    if let Some(contacts) = value.get("contacts").and_then(Value::as_array) {
        for contact in contacts {
            let Some(wa_id) = value_string(contact, "wa_id") else {
                continue;
            };
            let name = contact
                .get("profile")
                .and_then(|profile| value_string(profile, "name"))
                .unwrap_or_else(|| wa_id.clone());
            names.insert(normalize_phone(&wa_id), name);
        }
    }
    names
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

fn normalize_phone(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('+')
        .replace([' ', '-', '(', ')'], "")
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
    fn normalizes_cloud_api_webhook_payload() {
        let adapter = test_adapter();
        let messages = adapter.normalize_webhook(&json!({
            "entry": [{
                "changes": [{
                    "value": {
                        "contacts": [{
                            "profile": { "name": "WhatsApp User" },
                            "wa_id": "15550001"
                        }],
                        "messages": [{
                            "from": "15550001",
                            "id": "wamid.1",
                            "text": { "body": "hello whatsapp" },
                            "type": "text"
                        }]
                    }
                }]
            }]
        }));

        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert_eq!(message.id, "whatsapp-wamid.1");
        assert_eq!(message.text, "hello whatsapp");
        assert_eq!(message.route.platform, "whatsapp");
        assert_eq!(message.route.chat_id, "15550001");
        assert_eq!(message.route.chat_type, ChatType::Direct);
        assert_eq!(message.route.display_name, "WhatsApp User");
        assert_eq!(message.metadata["channel"]["sourceMessageId"], "wamid.1");
    }

    #[test]
    fn filters_unknown_users_and_non_text_messages() {
        let mut adapter = test_adapter();
        adapter.allowed_users = HashSet::from(["15550002".to_string()]);

        assert!(
            adapter
                .normalize_message(&json!({
                    "from": "15550001",
                    "id": "wamid.1",
                    "text": { "body": "blocked" },
                    "type": "text"
                }))
                .is_none()
        );
        assert!(
            adapter
                .normalize_message(&json!({
                    "from": "15550002",
                    "id": "wamid.2",
                    "image": { "id": "media" },
                    "type": "image"
                }))
                .is_none()
        );
    }

    #[test]
    fn classifies_whatsapp_error_codes() {
        assert_eq!(
            classify_whatsapp_response(&json!({
                "error": {
                    "code": 190,
                    "message": "expired"
                }
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::SessionExpired
        );
        assert_eq!(
            classify_whatsapp_response(&json!({
                "error": {
                    "code": 429,
                    "message": "slow down"
                }
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::RateLimited
        );
        assert_eq!(
            classify_whatsapp_response(&json!({
                "error": {
                    "code": 400,
                    "message": "bad"
                }
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::Fatal
        );
    }

    #[test]
    fn split_text_path_encoding_and_phone_normalization_preserve_unicode() {
        assert_eq!(split_text_chunks("你好世界", 2), vec!["你好", "世界"]);
        assert_eq!(url_encode_path("phone/id"), "phone%2Fid");
        assert_eq!(normalize_phone("+1 (555) 000-1"), "15550001");
    }

    fn test_adapter() -> WhatsAppAdapter {
        WhatsAppAdapter {
            graph_base: DEFAULT_WHATSAPP_GRAPH_BASE.to_string(),
            api_version: WHATSAPP_API_VERSION.to_string(),
            access_token: "token".to_string(),
            phone_number_id: "phone-id".to_string(),
            business_account_id: Some("business-id".to_string()),
            allowed_users: HashSet::new(),
        }
    }
}
