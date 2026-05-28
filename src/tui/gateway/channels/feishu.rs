use std::{
    collections::HashSet,
    env,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};

use super::platform::{
    ChannelCapabilityReport, ChannelCapabilityState, ChannelError, ChannelErrorKind, ChannelResult,
    ChatType, MessageRoute, NormalizedInboundMessage, OutboundMessage, OutboundStreamUpdate,
    PlatformAdapter, PlatformSendOutcome,
};

const DEFAULT_FEISHU_API_BASE: &str = "https://open.feishu.cn";
const FEISHU_MAX_TEXT_LENGTH: usize = 1_400;
const FEISHU_TIMEOUT_MS: u64 = 15_000;

pub struct FeishuAdapter {
    api_base: String,
    app_id: String,
    app_secret: String,
    tenant_access_token: Option<String>,
    verification_token: Option<String>,
    home_chat_id: Option<String>,
    allowed_users: HashSet<String>,
}

impl FeishuAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let app_id = env::var("FEISHU_APP_ID")
            .or_else(|_| env::var("LARK_APP_ID"))
            .or_else(|_| env::var("FLYFLOR_FEISHU_APP_ID"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if app_id.is_empty() {
            return Err(ChannelError::missing_config(
                "FEISHU_APP_ID is required for the feishu channel",
            ));
        }
        let app_secret = env::var("FEISHU_APP_SECRET")
            .or_else(|_| env::var("LARK_APP_SECRET"))
            .or_else(|_| env::var("FLYFLOR_FEISHU_APP_SECRET"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if app_secret.is_empty() {
            return Err(ChannelError::missing_config(
                "FEISHU_APP_SECRET is required for the feishu channel",
            ));
        }
        Ok(Self {
            api_base: env::var("FEISHU_API_BASE")
                .or_else(|_| env::var("LARK_API_BASE"))
                .or_else(|_| env::var("FLYFLOR_FEISHU_API_BASE"))
                .unwrap_or_else(|_| DEFAULT_FEISHU_API_BASE.to_string())
                .trim()
                .trim_end_matches('/')
                .to_string(),
            app_id,
            app_secret,
            tenant_access_token: env::var("FEISHU_TENANT_ACCESS_TOKEN")
                .or_else(|_| env::var("LARK_TENANT_ACCESS_TOKEN"))
                .or_else(|_| env::var("FLYFLOR_FEISHU_TENANT_ACCESS_TOKEN"))
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            verification_token: env::var("FEISHU_VERIFICATION_TOKEN")
                .or_else(|_| env::var("LARK_VERIFICATION_TOKEN"))
                .or_else(|_| env::var("FLYFLOR_FEISHU_VERIFICATION_TOKEN"))
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            home_chat_id: env::var("FEISHU_HOME_CHAT_ID")
                .or_else(|_| env::var("LARK_HOME_CHAT_ID"))
                .or_else(|_| env::var("FLYFLOR_FEISHU_HOME_CHAT_ID"))
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            allowed_users: env_set("FEISHU_ALLOWED_USERS"),
        })
    }

    fn tenant_token_url(&self) -> String {
        format!(
            "{}/open-apis/auth/v3/tenant_access_token/internal",
            self.api_base
        )
    }

    fn reply_url(&self, message_id: &str) -> String {
        format!(
            "{}/open-apis/im/v1/messages/{}/reply",
            self.api_base,
            url_encode_path(message_id)
        )
    }

    fn send_url(&self) -> String {
        format!(
            "{}/open-apis/im/v1/messages?receive_id_type=chat_id",
            self.api_base
        )
    }

    fn update_url(&self, message_id: &str) -> String {
        format!(
            "{}/open-apis/im/v1/messages/{}",
            self.api_base,
            url_encode_path(message_id)
        )
    }

    fn access_token(&self) -> ChannelResult<String> {
        if let Some(token) = self.tenant_access_token.clone() {
            return Ok(token);
        }
        let response = feishu_post(
            &self.tenant_token_url(),
            None,
            json!({
                "app_id": self.app_id,
                "app_secret": self.app_secret
            }),
        )?;
        classify_feishu_response(&response)?;
        response
            .get("tenant_access_token")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .ok_or_else(|| ChannelError::retryable("feishu tenant token missing from response"))
    }

    fn normalize_webhook(&self, value: &Value) -> Vec<NormalizedInboundMessage> {
        if let Some(challenge) = value.get("challenge").and_then(Value::as_str) {
            return vec![NormalizedInboundMessage {
                id: format!("feishu-challenge-{}", now_millis()),
                text: challenge.to_string(),
                route: MessageRoute {
                    platform: "feishu".to_string(),
                    chat_id: "challenge".to_string(),
                    chat_type: ChatType::Direct,
                    user_id: "feishu-challenge".to_string(),
                    display_name: "Feishu Challenge".to_string(),
                    thread_id: "challenge".to_string(),
                },
                context: None,
                metadata: json!({
                    "channel": {
                        "platform": "feishu",
                        "adapter": "feishu-open-platform",
                        "challenge": true
                    }
                }),
            }];
        }
        if let Some(expected) = self.verification_token.as_deref()
            && value
                .get("token")
                .and_then(Value::as_str)
                .is_some_and(|token| token != expected)
        {
            return Vec::new();
        }
        let event = value.get("event").unwrap_or(value);
        self.normalize_event(event).into_iter().collect()
    }

    fn normalize_event(&self, event: &Value) -> Option<NormalizedInboundMessage> {
        let message = event.get("message").unwrap_or(event);
        let message_type = value_string(message, "message_type")
            .or_else(|| value_string(message, "msg_type"))
            .unwrap_or_else(|| "text".to_string());
        if message_type != "text" {
            return None;
        }
        let text = parse_text_content(message.get("content")?)?;
        if text.trim().is_empty() {
            return None;
        }
        let sender = event.get("sender").unwrap_or(&Value::Null);
        let sender_id = sender.get("sender_id").unwrap_or(sender);
        let user_id = value_string(sender_id, "open_id")
            .or_else(|| value_string(sender_id, "user_id"))
            .or_else(|| value_string(event, "open_id"))
            .unwrap_or_else(|| "feishu-user".to_string());
        if !self.allowed_users.is_empty() && !self.allowed_users.contains(&user_id) {
            return None;
        }
        let chat_id = value_string(message, "chat_id")
            .or_else(|| value_string(event, "chat_id"))
            .unwrap_or_else(|| user_id.clone());
        let chat_type = match value_string(message, "chat_type").as_deref() {
            Some("group") | Some("p2p") if chat_id != user_id => ChatType::Group,
            _ if chat_id != user_id => ChatType::Group,
            _ => ChatType::Direct,
        };
        let message_id = value_string(message, "message_id")
            .unwrap_or_else(|| format!("feishu-{}", now_millis()));
        let route = MessageRoute {
            platform: "feishu".to_string(),
            chat_id: chat_id.clone(),
            chat_type,
            user_id: user_id.clone(),
            display_name: user_id.clone(),
            thread_id: chat_id.clone(),
        };
        let metadata = json!({
            "channel": {
                "platform": "feishu",
                "adapter": "feishu-open-platform",
                "chatId": chat_id,
                "chatType": route.chat_type.as_gateway_str(),
                "userId": user_id,
                "sourceMessageId": message_id,
                "messageId": message_id
            }
        });
        Some(NormalizedInboundMessage {
            id: format!("feishu-{message_id}"),
            text,
            route,
            context: None,
            metadata,
        })
    }
}

impl PlatformAdapter for FeishuAdapter {
    fn name(&self) -> &'static str {
        "feishu"
    }

    fn capabilities(&self) -> ChannelCapabilityReport {
        ChannelCapabilityReport {
            send: ChannelCapabilityState::Available,
            typing: ChannelCapabilityState::Unavailable,
            edit: ChannelCapabilityState::Unavailable,
            draft: ChannelCapabilityState::Unavailable,
            card: ChannelCapabilityState::Available,
            media: ChannelCapabilityState::Unavailable,
        }
    }

    fn poll_updates(&self) -> ChannelResult<Vec<NormalizedInboundMessage>> {
        let raw = env::var("FEISHU_INBOUND_WEBHOOK")
            .or_else(|_| env::var("LARK_INBOUND_WEBHOOK"))
            .or_else(|_| env::var("FLYFLOR_FEISHU_INBOUND_WEBHOOK"))
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let Some(raw) = raw else {
            return Ok(Vec::new());
        };
        let value = serde_json::from_str::<Value>(&raw).map_err(|error| {
            ChannelError::fatal(format!("feishu inbound webhook JSON is invalid: {error}"))
        })?;
        Ok(self.normalize_webhook(&value))
    }

    fn send_typing(&self, _route: &MessageRoute) -> ChannelResult<()> {
        Err(ChannelError::unavailable(
            "feishu typing indicator is unavailable in the Open Platform adapter",
        ))
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal("feishu message text must not be empty"));
        }
        if outbound_mentions_media(&message.text, message.metadata.as_ref()) {
            return self.send_media_unavailable("outbound");
        }
        let token = self.access_token()?;
        let chunks = split_text_chunks(&message.text, FEISHU_MAX_TEXT_LENGTH);
        let mut last_id = None;
        for chunk in chunks {
            let response = if let Some(reply_to) =
                source_message_id(&message).or_else(|| message.reply_to_message_id.clone())
            {
                feishu_post(
                    &self.reply_url(reply_to.strip_prefix("feishu-").unwrap_or(&reply_to)),
                    Some(&token),
                    json!({
                        "msg_type": "text",
                        "content": json_string(json!({ "text": chunk }))
                    }),
                )?
            } else {
                let receive_id = if message.route.chat_id.trim().is_empty() {
                    self.home_chat_id.clone().ok_or_else(|| {
                        ChannelError::fatal(
                            "FEISHU_HOME_CHAT_ID is required when route chat_id is empty",
                        )
                    })?
                } else {
                    message.route.chat_id.clone()
                };
                feishu_post(
                    &self.send_url(),
                    Some(&token),
                    json!({
                        "receive_id": receive_id,
                        "msg_type": "text",
                        "content": json_string(json!({ "text": chunk }))
                    }),
                )?
            };
            classify_feishu_response(&response)?;
            last_id = response
                .get("data")
                .and_then(|data| value_string(data, "message_id"))
                .or_else(|| value_string(&response, "message_id"))
                .or_else(|| Some(format!("feishu-{}", now_millis())));
        }
        Ok(PlatformSendOutcome {
            message_id: last_id,
        })
    }

    fn stream_update(&self, update: OutboundStreamUpdate) -> ChannelResult<PlatformSendOutcome> {
        if update.text.trim().is_empty() {
            return Err(ChannelError::fatal("feishu card text must not be empty"));
        }
        let token = self.access_token()?;
        let message_id = update
            .metadata
            .as_ref()
            .and_then(source_message_id_from_metadata)
            .unwrap_or_else(|| update.message_id.clone());
        let response = feishu_patch(
            &self.update_url(message_id.strip_prefix("feishu-").unwrap_or(&message_id)),
            &token,
            json!({
                "msg_type": "interactive",
                "content": json_string(card_content(&update.text, update.final_update))
            }),
        )?;
        classify_feishu_response(&response)?;
        Ok(PlatformSendOutcome {
            message_id: response
                .get("data")
                .and_then(|data| value_string(data, "message_id"))
                .or_else(|| value_string(&response, "message_id"))
                .or_else(|| Some(message_id)),
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "feishu {media_kind} media delivery is explicitly unavailable in flyflor-cli"
        )))
    }
}

fn feishu_post(url: &str, token: Option<&str>, payload: Value) -> ChannelResult<Value> {
    run_feishu_curl("POST", url, token, payload)
}

fn feishu_patch(url: &str, token: &str, payload: Value) -> ChannelResult<Value> {
    run_feishu_curl("PATCH", url, Some(token), payload)
}

fn run_feishu_curl(
    method: &str,
    url: &str,
    token: Option<&str>,
    payload: Value,
) -> ChannelResult<Value> {
    let body =
        serde_json::to_string(&payload).map_err(|error| ChannelError::fatal(error.to_string()))?;
    let mut args = vec![
        "-sS".to_string(),
        "--max-time".to_string(),
        seconds_arg(FEISHU_TIMEOUT_MS),
        "-X".to_string(),
        method.to_string(),
        url.to_string(),
        "-H".to_string(),
        "Content-Type: application/json".to_string(),
    ];
    if let Some(token) = token {
        args.push("-H".to_string());
        args.push(format!("Authorization: Bearer {token}"));
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
                "feishu authorization failed: {message}"
            )));
        }
        if message.contains("429") {
            return Err(ChannelError::rate_limited(format!(
                "feishu rate limited: {message}"
            )));
        }
        return Err(ChannelError::retryable(format!(
            "feishu curl failed with status {}: {}",
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
            "feishu returned invalid JSON: {error}; body={text}"
        ))
    })
}

fn classify_feishu_response(value: &Value) -> ChannelResult<()> {
    let code = value
        .get("code")
        .or_else(|| value.get("StatusCode"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if code == 0 {
        return Ok(());
    }
    let message = value
        .get("msg")
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("unknown feishu error");
    match code {
        99991663 | 99991668 | 99991672 => Err(ChannelError::session_expired(format!(
            "Feishu authorization failed: code={code} message={message}"
        ))),
        99991400 | 99991401 | 99991402 => Err(ChannelError::rate_limited(format!(
            "Feishu rate limited: code={code} message={message}"
        ))),
        400 | 230001 | 230002 => Err(ChannelError::fatal(format!(
            "Feishu bad request: code={code} message={message}"
        ))),
        _ => Err(ChannelError::retryable(format!(
            "Feishu error: code={code} message={message}"
        ))),
    }
}

fn parse_text_content(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        if let Ok(parsed) = serde_json::from_str::<Value>(text) {
            return value_string(&parsed, "text").or_else(|| Some(text.to_string()));
        }
        return Some(text.to_string());
    }
    value_string(value, "text")
}

fn card_content(text: &str, final_update: bool) -> Value {
    json!({
        "config": { "wide_screen_mode": true },
        "elements": [{
            "tag": "div",
            "text": {
                "tag": "lark_md",
                "content": text
            }
        }],
        "header": {
            "template": if final_update { "green" } else { "blue" },
            "title": {
                "tag": "plain_text",
                "content": if final_update { "Flyflor" } else { "Flyflor working" }
            }
        }
    })
}

fn json_string(value: Value) -> String {
    serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string())
}

fn source_message_id(message: &OutboundMessage) -> Option<String> {
    message
        .metadata
        .as_ref()
        .and_then(source_message_id_from_metadata)
}

fn source_message_id_from_metadata(metadata: &Value) -> Option<String> {
    metadata
        .get("channel")
        .and_then(|channel| {
            value_string(channel, "messageId").or_else(|| value_string(channel, "sourceMessageId"))
        })
        .filter(|value| !value.is_empty())
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
    fn normalizes_feishu_message_event() {
        let adapter = test_adapter();
        let messages = adapter.normalize_webhook(&json!({
            "token": "verify-token",
            "event": {
                "sender": { "sender_id": { "open_id": "ou_1" } },
                "message": {
                    "chat_id": "oc_1",
                    "chat_type": "group",
                    "content": "{\"text\":\"hello feishu\"}",
                    "message_id": "om_1",
                    "message_type": "text"
                }
            }
        }));

        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert_eq!(message.id, "feishu-om_1");
        assert_eq!(message.text, "hello feishu");
        assert_eq!(message.route.platform, "feishu");
        assert_eq!(message.route.chat_id, "oc_1");
        assert_eq!(message.route.chat_type, ChatType::Group);
        assert_eq!(message.metadata["channel"]["sourceMessageId"], "om_1");
    }

    #[test]
    fn filters_token_allowlist_and_non_text_messages() {
        let mut adapter = test_adapter();
        adapter.allowed_users = HashSet::from(["ou_allowed".to_string()]);

        assert!(
            adapter
                .normalize_webhook(&json!({ "token": "wrong" }))
                .is_empty()
        );
        assert!(
            adapter
                .normalize_event(&json!({
                    "sender": { "sender_id": { "open_id": "ou_blocked" } },
                    "message": {
                        "chat_id": "oc_1",
                        "content": "{\"text\":\"blocked\"}",
                        "message_id": "om_1",
                        "message_type": "text"
                    }
                }))
                .is_none()
        );
        assert!(
            adapter
                .normalize_event(&json!({
                    "sender": { "sender_id": { "open_id": "ou_allowed" } },
                    "message": {
                        "chat_id": "oc_1",
                        "content": "{}",
                        "message_id": "om_2",
                        "message_type": "image"
                    }
                }))
                .is_none()
        );
    }

    #[test]
    fn classifies_feishu_error_codes() {
        assert_eq!(
            classify_feishu_response(&json!({
                "code": 99991663,
                "msg": "expired"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::SessionExpired
        );
        assert_eq!(
            classify_feishu_response(&json!({
                "code": 99991400,
                "msg": "slow down"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::RateLimited
        );
        assert_eq!(
            classify_feishu_response(&json!({
                "code": 230001,
                "msg": "bad"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::Fatal
        );
    }

    #[test]
    fn card_content_and_helpers_preserve_unicode() {
        assert_eq!(split_text_chunks("你好世界", 2), vec!["你好", "世界"]);
        assert_eq!(url_encode_path("om/1"), "om%2F1");
        let card = card_content("运行中", false);
        assert_eq!(card["elements"][0]["text"]["content"], "运行中");
    }

    fn test_adapter() -> FeishuAdapter {
        FeishuAdapter {
            api_base: DEFAULT_FEISHU_API_BASE.to_string(),
            app_id: "app-id".to_string(),
            app_secret: "secret".to_string(),
            tenant_access_token: Some("tenant-token".to_string()),
            verification_token: Some("verify-token".to_string()),
            home_chat_id: None,
            allowed_users: HashSet::new(),
        }
    }
}
