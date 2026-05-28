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

const DEFAULT_DINGTALK_API_BASE: &str = "https://api.dingtalk.com";
const DINGTALK_MAX_TEXT_LENGTH: usize = 1_800;
const DINGTALK_TIMEOUT_MS: u64 = 15_000;

pub struct DingTalkAdapter {
    api_base: String,
    client_id: String,
    client_secret: String,
    access_token: Option<String>,
    robot_code: Option<String>,
    home_conversation_id: Option<String>,
    allowed_users: HashSet<String>,
}

impl DingTalkAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let client_id = env::var("DINGTALK_CLIENT_ID")
            .or_else(|_| env::var("DINGTALK_APP_KEY"))
            .or_else(|_| env::var("FLYFLOR_DINGTALK_CLIENT_ID"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if client_id.is_empty() {
            return Err(ChannelError::missing_config(
                "DINGTALK_CLIENT_ID is required for the dingtalk channel",
            ));
        }
        let client_secret = env::var("DINGTALK_CLIENT_SECRET")
            .or_else(|_| env::var("DINGTALK_APP_SECRET"))
            .or_else(|_| env::var("FLYFLOR_DINGTALK_APP_SECRET"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if client_secret.is_empty() {
            return Err(ChannelError::missing_config(
                "DINGTALK_CLIENT_SECRET is required for the dingtalk channel",
            ));
        }
        Ok(Self {
            api_base: env::var("DINGTALK_API_BASE")
                .or_else(|_| env::var("FLYFLOR_DINGTALK_API_BASE"))
                .unwrap_or_else(|_| DEFAULT_DINGTALK_API_BASE.to_string())
                .trim()
                .trim_end_matches('/')
                .to_string(),
            client_id,
            client_secret,
            access_token: env::var("DINGTALK_ACCESS_TOKEN")
                .or_else(|_| env::var("FLYFLOR_DINGTALK_ACCESS_TOKEN"))
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            robot_code: env::var("DINGTALK_ROBOT_CODE")
                .or_else(|_| env::var("FLYFLOR_DINGTALK_ROBOT_CODE"))
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            home_conversation_id: env::var("DINGTALK_HOME_CONVERSATION_ID")
                .or_else(|_| env::var("DINGTALK_HOME_CHANNEL"))
                .or_else(|_| env::var("FLYFLOR_DINGTALK_HOME_CONVERSATION_ID"))
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            allowed_users: env_set("DINGTALK_ALLOWED_USERS"),
        })
    }

    fn access_token_url(&self) -> String {
        format!("{}/v1.0/oauth2/accessToken", self.api_base)
    }

    fn robot_group_send_url(&self) -> String {
        format!("{}/v1.0/robot/groupMessages/send", self.api_base)
    }

    fn access_token(&self) -> ChannelResult<String> {
        if let Some(token) = self.access_token.clone() {
            return Ok(token);
        }
        let response = dingtalk_post(
            &self.access_token_url(),
            None,
            json!({
                "appKey": self.client_id,
                "appSecret": self.client_secret
            }),
        )?;
        classify_dingtalk_response(&response)?;
        value_string_any(&response, &["accessToken", "access_token"])
            .ok_or_else(|| ChannelError::retryable("dingtalk access token missing from response"))
    }

    fn normalize_webhook(&self, value: &Value) -> Option<NormalizedInboundMessage> {
        let event = value.get("event").unwrap_or(value);
        let text = event
            .get("text")
            .and_then(|text| {
                value_string_any(text, &["content", "text"])
                    .or_else(|| text.as_str().map(ToString::to_string))
            })
            .or_else(|| value_string_any(event, &["content", "text", "msgContent"]))?
            .trim()
            .to_string();
        if text.is_empty() {
            return None;
        }
        let user_id = value_string_any(
            event,
            &[
                "senderStaffId",
                "sender_user_id",
                "senderUserId",
                "userId",
                "openId",
            ],
        )
        .unwrap_or_else(|| "dingtalk-user".to_string());
        if !self.allowed_users.is_empty() && !self.allowed_users.contains(&user_id) {
            return None;
        }
        let chat_id = value_string_any(
            event,
            &[
                "conversationId",
                "openConversationId",
                "conversation_id",
                "chatId",
            ],
        )
        .unwrap_or_else(|| user_id.clone());
        let message_id = value_string_any(event, &["msgId", "messageId", "message_id", "id"])
            .unwrap_or_else(|| format!("dingtalk-{}", now_millis()));
        let session_webhook = value_string_any(event, &["sessionWebhook", "session_webhook"]);
        let route = MessageRoute {
            platform: "dingtalk".to_string(),
            chat_id: chat_id.clone(),
            chat_type: if chat_id == user_id {
                ChatType::Direct
            } else {
                ChatType::Group
            },
            user_id: user_id.clone(),
            display_name: value_string_any(event, &["senderNick", "senderName", "name"])
                .unwrap_or_else(|| user_id.clone()),
            thread_id: chat_id.clone(),
        };
        let metadata = json!({
            "channel": {
                "platform": "dingtalk",
                "adapter": "dingtalk-openapi",
                "chatId": chat_id,
                "chatType": route.chat_type.as_gateway_str(),
                "userId": user_id,
                "sourceMessageId": message_id,
                "sessionWebhook": session_webhook
            }
        });
        Some(NormalizedInboundMessage {
            id: format!("dingtalk-{message_id}"),
            text,
            route,
            context: None,
            metadata,
        })
    }
}

impl PlatformAdapter for DingTalkAdapter {
    fn name(&self) -> &'static str {
        "dingtalk"
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
        let raw = env::var("DINGTALK_INBOUND_WEBHOOK")
            .or_else(|_| env::var("FLYFLOR_DINGTALK_INBOUND_WEBHOOK"))
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let Some(raw) = raw else {
            return Ok(Vec::new());
        };
        let value = serde_json::from_str::<Value>(&raw).map_err(|error| {
            ChannelError::fatal(format!("dingtalk inbound webhook JSON is invalid: {error}"))
        })?;
        Ok(self.normalize_webhook(&value).into_iter().collect())
    }

    fn send_typing(&self, _route: &MessageRoute) -> ChannelResult<()> {
        Err(ChannelError::unavailable(
            "dingtalk typing indicator is unavailable in the OpenAPI adapter",
        ))
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal(
                "dingtalk message text must not be empty",
            ));
        }
        if outbound_mentions_media(&message.text, message.metadata.as_ref()) {
            return self.send_media_unavailable("outbound");
        }
        let chunks = split_text_chunks(&message.text, DINGTALK_MAX_TEXT_LENGTH);
        let session_webhook = message
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("channel"))
            .and_then(|channel| value_string_any(channel, &["sessionWebhook", "session_webhook"]));
        let mut last_id = None;
        for chunk in chunks {
            let response = if let Some(webhook) = session_webhook.as_deref() {
                dingtalk_post(
                    webhook,
                    None,
                    json!({
                        "msgtype": "text",
                        "text": { "content": chunk }
                    }),
                )?
            } else {
                let access_token = self.access_token()?;
                let robot_code = self.robot_code.clone().ok_or_else(|| {
                    ChannelError::fatal(
                        "DINGTALK_ROBOT_CODE is required when sessionWebhook is unavailable",
                    )
                })?;
                let conversation_id = if message.route.chat_id.trim().is_empty() {
                    self.home_conversation_id.clone().ok_or_else(|| {
                        ChannelError::fatal(
                            "DINGTALK_HOME_CONVERSATION_ID is required when route chat_id is empty",
                        )
                    })?
                } else {
                    message.route.chat_id.clone()
                };
                dingtalk_post(
                    &self.robot_group_send_url(),
                    Some(&access_token),
                    json!({
                        "robotCode": robot_code,
                        "openConversationId": conversation_id,
                        "msgKey": "sampleText",
                        "msgParam": json_string(json!({ "content": chunk }))
                    }),
                )?
            };
            classify_dingtalk_response(&response)?;
            last_id = value_string_any(&response, &["processQueryKey", "messageId", "id"])
                .or_else(|| Some(format!("dingtalk-{}", now_millis())));
        }
        Ok(PlatformSendOutcome {
            message_id: last_id,
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "dingtalk {media_kind} media delivery is explicitly unavailable in flyflor-cli"
        )))
    }
}

fn dingtalk_post(url: &str, token: Option<&str>, payload: Value) -> ChannelResult<Value> {
    let body =
        serde_json::to_string(&payload).map_err(|error| ChannelError::fatal(error.to_string()))?;
    let mut args = vec![
        "-sS".to_string(),
        "--max-time".to_string(),
        seconds_arg(DINGTALK_TIMEOUT_MS),
        "-X".to_string(),
        "POST".to_string(),
        url.to_string(),
        "-H".to_string(),
        "Content-Type: application/json".to_string(),
    ];
    if let Some(token) = token {
        args.push("-H".to_string());
        args.push(format!("x-acs-dingtalk-access-token: {token}"));
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
                "dingtalk authorization failed: {message}"
            )));
        }
        if message.contains("429") {
            return Err(ChannelError::rate_limited(format!(
                "dingtalk rate limited: {message}"
            )));
        }
        return Err(ChannelError::retryable(format!(
            "dingtalk curl failed with status {}: {}",
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
            "dingtalk returned invalid JSON: {error}; body={text}"
        ))
    })
}

fn classify_dingtalk_response(value: &Value) -> ChannelResult<()> {
    let code = value
        .get("errcode")
        .or_else(|| value.get("code"))
        .or_else(|| value.get("statusCode"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if code == 0 {
        return Ok(());
    }
    let message = value
        .get("errmsg")
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("unknown dingtalk error");
    match code {
        40001 | 40014 | 41001 | 43002 => Err(ChannelError::session_expired(format!(
            "DingTalk authorization failed: code={code} message={message}"
        ))),
        90018 | 130101 => Err(ChannelError::rate_limited(format!(
            "DingTalk rate limited: code={code} message={message}"
        ))),
        400 | 40035 | 41005 => Err(ChannelError::fatal(format!(
            "DingTalk bad request: code={code} message={message}"
        ))),
        _ => Err(ChannelError::retryable(format!(
            "DingTalk error: code={code} message={message}"
        ))),
    }
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

fn json_string(value: Value) -> String {
    serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string())
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
    fn normalizes_dingtalk_webhook_payload() {
        let adapter = test_adapter();
        let message = adapter
            .normalize_webhook(&json!({
                "conversationId": "cid-1",
                "msgId": "msg-1",
                "senderNick": "Ding User",
                "senderStaffId": "staff-1",
                "sessionWebhook": "http://127.0.0.1/session",
                "text": { "content": "hello dingtalk" }
            }))
            .unwrap();

        assert_eq!(message.id, "dingtalk-msg-1");
        assert_eq!(message.text, "hello dingtalk");
        assert_eq!(message.route.platform, "dingtalk");
        assert_eq!(message.route.chat_id, "cid-1");
        assert_eq!(message.route.chat_type, ChatType::Group);
        assert_eq!(message.route.user_id, "staff-1");
        assert_eq!(
            message.metadata["channel"]["sessionWebhook"],
            "http://127.0.0.1/session"
        );
    }

    #[test]
    fn allowlist_blocks_unknown_sender() {
        let mut adapter = test_adapter();
        adapter.allowed_users = HashSet::from(["staff-2".to_string()]);

        assert!(
            adapter
                .normalize_webhook(&json!({
                    "senderStaffId": "staff-1",
                    "text": { "content": "blocked" }
                }))
                .is_none()
        );
    }

    #[test]
    fn classifies_dingtalk_error_codes() {
        assert_eq!(
            classify_dingtalk_response(&json!({
                "errcode": 40001,
                "errmsg": "expired"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::SessionExpired
        );
        assert_eq!(
            classify_dingtalk_response(&json!({
                "errcode": 90018,
                "errmsg": "slow down"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::RateLimited
        );
        assert_eq!(
            classify_dingtalk_response(&json!({
                "errcode": 40035,
                "errmsg": "bad"
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

    fn test_adapter() -> DingTalkAdapter {
        DingTalkAdapter {
            api_base: DEFAULT_DINGTALK_API_BASE.to_string(),
            client_id: "client-id".to_string(),
            client_secret: "secret".to_string(),
            access_token: Some("token".to_string()),
            robot_code: Some("robot-code".to_string()),
            home_conversation_id: None,
            allowed_users: HashSet::new(),
        }
    }
}
