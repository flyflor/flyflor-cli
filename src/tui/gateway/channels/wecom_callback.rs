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

const DEFAULT_WECOM_API_BASE: &str = "https://qyapi.weixin.qq.com/cgi-bin";
const WECOM_TEXT_LIMIT: usize = 2_048;
const WECOM_TIMEOUT_MS: u64 = 15_000;

pub struct WeComCallbackAdapter {
    api_base: String,
    corp_id: String,
    corp_secret: String,
    agent_id: String,
    access_token: Option<String>,
    allowed_users: HashSet<String>,
}

impl WeComCallbackAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let corp_id = env_first(&["WECOM_CORP_ID", "FLYFLOR_WECOM_CORP_ID"]);
        if corp_id.is_empty() {
            return Err(ChannelError::missing_config(
                "WECOM_CORP_ID is required for the wecom-callback channel",
            ));
        }
        let corp_secret = env_first(&["WECOM_CORP_SECRET", "FLYFLOR_WECOM_CORP_SECRET"]);
        if corp_secret.is_empty() {
            return Err(ChannelError::missing_config(
                "WECOM_CORP_SECRET is required for the wecom-callback channel",
            ));
        }
        let agent_id = env_first(&["WECOM_AGENT_ID", "FLYFLOR_WECOM_AGENT_ID"]);
        if agent_id.is_empty() {
            return Err(ChannelError::missing_config(
                "WECOM_AGENT_ID is required for the wecom-callback channel",
            ));
        }
        Ok(Self {
            api_base: env_first(&["WECOM_API_BASE", "FLYFLOR_WECOM_API_BASE"])
                .if_empty(DEFAULT_WECOM_API_BASE),
            corp_id,
            corp_secret,
            agent_id,
            access_token: env_optional(&["WECOM_ACCESS_TOKEN", "FLYFLOR_WECOM_ACCESS_TOKEN"]),
            allowed_users: env_set_any(&["WECOM_CALLBACK_ALLOWED_USERS", "WECOM_ALLOWED_USERS"]),
        })
    }

    fn access_token(&self) -> ChannelResult<String> {
        if let Some(token) = self.access_token.clone() {
            return Ok(token);
        }
        let url = format!(
            "{}/gettoken?corpid={}&corpsecret={}",
            self.api_base,
            query_value(&self.corp_id),
            query_value(&self.corp_secret)
        );
        let response = wecom_get(&url)?;
        classify_wecom_response(&response)?;
        value_string_any(&response, &["access_token", "accessToken"])
            .ok_or_else(|| ChannelError::retryable("wecom access_token missing from response"))
    }

    fn normalize_event(&self, value: &Value) -> Option<NormalizedInboundMessage> {
        let event = value
            .get("event")
            .or_else(|| value.get("xml"))
            .or_else(|| value.get("payload"))
            .unwrap_or(value);
        let user_id = value_string_any(
            event,
            &["FromUserName", "fromUserName", "userId", "user_id"],
        )?;
        if !self.allowed_users.is_empty() && !self.allowed_users.contains(&user_id) {
            return None;
        }
        let text = value_string_any(event, &["Content", "content", "text"])?
            .trim()
            .to_string();
        if text.is_empty() {
            return None;
        }
        let message_id = value_string_any(event, &["MsgId", "msgId", "messageId", "id"])
            .unwrap_or_else(|| format!("wecom-callback-{}", now_millis()));
        let route = MessageRoute {
            platform: "wecom-callback".to_string(),
            chat_id: format!("{}:{user_id}", self.corp_id),
            chat_type: ChatType::Direct,
            user_id: user_id.clone(),
            display_name: value_string_any(event, &["Name", "name"])
                .unwrap_or_else(|| user_id.clone()),
            thread_id: format!("{}:{user_id}", self.corp_id),
        };
        let metadata = json!({
            "channel": {
                "platform": "wecom-callback",
                "adapter": "wecom-callback-corp-api",
                "chatId": route.chat_id,
                "chatType": route.chat_type.as_gateway_str(),
                "corpId": self.corp_id,
                "userId": user_id,
                "sourceMessageId": message_id
            }
        });
        Some(NormalizedInboundMessage {
            id: format!("wecom-callback-{message_id}"),
            text,
            route,
            context: None,
            metadata,
        })
    }
}

impl PlatformAdapter for WeComCallbackAdapter {
    fn name(&self) -> &'static str {
        "wecom-callback"
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
        let raw = env_first(&[
            "WECOM_CALLBACK_INBOUND_EVENT",
            "WECOM_CALLBACK_INBOUND_MESSAGE",
        ]);
        if raw.is_empty() {
            return Ok(Vec::new());
        }
        let value = serde_json::from_str::<Value>(&raw).map_err(|error| {
            ChannelError::fatal(format!("wecom-callback inbound JSON is invalid: {error}"))
        })?;
        Ok(self.normalize_event(&value).into_iter().collect())
    }

    fn send_typing(&self, _route: &MessageRoute) -> ChannelResult<()> {
        Err(ChannelError::unavailable(
            "wecom-callback typing is unavailable in the Corp API text adapter",
        ))
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal(
                "wecom-callback message text must not be empty",
            ));
        }
        if outbound_mentions_media(&message.text, message.metadata.as_ref()) {
            return self.send_media_unavailable("outbound");
        }
        let token = self.access_token()?;
        let user_id = message
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("channel"))
            .and_then(|channel| value_string_any(channel, &["userId", "touser"]))
            .or_else(|| {
                message
                    .route
                    .chat_id
                    .split_once(':')
                    .map(|(_, user)| user.to_string())
            })
            .unwrap_or_else(|| message.route.user_id.clone());
        let mut last_id = None;
        for chunk in split_text_chunks(&message.text, WECOM_TEXT_LIMIT) {
            let url = format!(
                "{}/message/send?access_token={}",
                self.api_base,
                query_value(&token)
            );
            let response = wecom_post(
                &url,
                json!({
                    "touser": user_id,
                    "msgtype": "text",
                    "agentid": self.agent_id.parse::<u64>().unwrap_or(0),
                    "text": { "content": chunk },
                    "safe": 0
                }),
            )?;
            classify_wecom_response(&response)?;
            last_id = value_string_any(&response, &["msgid", "messageId", "id"])
                .or_else(|| Some(format!("wecom-callback-{}", now_millis())));
        }
        Ok(PlatformSendOutcome {
            message_id: last_id,
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "wecom-callback {media_kind} media delivery is explicitly unavailable in flyflor-cli"
        )))
    }
}

fn wecom_get(url: &str) -> ChannelResult<Value> {
    curl_json(["-sS", "--max-time", &seconds_arg(WECOM_TIMEOUT_MS), url])
}

fn wecom_post(url: &str, payload: Value) -> ChannelResult<Value> {
    let body =
        serde_json::to_string(&payload).map_err(|error| ChannelError::fatal(error.to_string()))?;
    curl_json([
        "-sS",
        "--max-time",
        &seconds_arg(WECOM_TIMEOUT_MS),
        "-X",
        "POST",
        url,
        "-H",
        "Content-Type: application/json",
        "--data",
        &body,
    ])
}

fn curl_json<const N: usize>(args: [&str; N]) -> ChannelResult<Value> {
    let output = Command::new("curl")
        .args(args)
        .output()
        .map_err(|error| ChannelError::unavailable(format!("curl is required: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr.trim();
        if message.contains("429") {
            return Err(ChannelError::rate_limited(format!(
                "wecom-callback rate limited: {message}"
            )));
        }
        return Err(ChannelError::retryable(format!(
            "wecom-callback curl failed with status {}: {}",
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
            "wecom-callback returned invalid JSON: {error}; body={text}"
        ))
    })
}

fn classify_wecom_response(value: &Value) -> ChannelResult<()> {
    let code = value
        .get("errcode")
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
        40001 | 40014 | 42001 => Err(ChannelError::session_expired(format!(
            "WeCom authorization failed: code={code} message={message}"
        ))),
        45009 | 60020 => Err(ChannelError::rate_limited(format!(
            "WeCom rate limited: code={code} message={message}"
        ))),
        40003 | 40004 | 40058 => Err(ChannelError::fatal(format!(
            "WeCom bad request: code={code} message={message}"
        ))),
        _ => Err(ChannelError::retryable(format!(
            "WeCom error: code={code} message={message}"
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

fn query_value(value: &str) -> String {
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
    fn normalizes_wecom_callback_event() {
        let adapter = test_adapter();
        let message = adapter
            .normalize_event(&json!({
                "FromUserName": "zhangsan",
                "Content": "hello wecom",
                "MsgId": "msg-1"
            }))
            .unwrap();

        assert_eq!(message.id, "wecom-callback-msg-1");
        assert_eq!(message.text, "hello wecom");
        assert_eq!(message.route.platform, "wecom-callback");
        assert_eq!(message.route.chat_id, "corp-1:zhangsan");
        assert_eq!(message.route.chat_type, ChatType::Direct);
        assert_eq!(message.metadata["channel"]["corpId"], "corp-1");
        assert_eq!(message.metadata["channel"]["sourceMessageId"], "msg-1");
    }

    #[test]
    fn allowlist_blocks_unknown_sender() {
        let mut adapter = test_adapter();
        adapter.allowed_users = HashSet::from(["lisi".to_string()]);

        assert!(
            adapter
                .normalize_event(&json!({
                    "FromUserName": "zhangsan",
                    "Content": "blocked",
                    "MsgId": "msg-1"
                }))
                .is_none()
        );
    }

    #[test]
    fn classifies_wecom_error_codes() {
        assert_eq!(
            classify_wecom_response(&json!({
                "errcode": 42001,
                "errmsg": "expired"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::SessionExpired
        );
        assert_eq!(
            classify_wecom_response(&json!({
                "errcode": 45009,
                "errmsg": "busy"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::RateLimited
        );
        assert_eq!(
            classify_wecom_response(&json!({
                "errcode": 40003,
                "errmsg": "bad user"
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

    fn test_adapter() -> WeComCallbackAdapter {
        WeComCallbackAdapter {
            api_base: DEFAULT_WECOM_API_BASE.to_string(),
            corp_id: "corp-1".to_string(),
            corp_secret: "secret".to_string(),
            agent_id: "1000001".to_string(),
            access_token: Some("token".to_string()),
            allowed_users: HashSet::new(),
        }
    }
}
