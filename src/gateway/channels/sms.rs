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

const DEFAULT_TWILIO_API_BASE: &str = "https://api.twilio.com";
const SMS_MAX_MESSAGE_LENGTH: usize = 1_500;
const TWILIO_TIMEOUT_MS: u64 = 15_000;

pub struct SmsAdapter {
    account_sid: String,
    auth_token: String,
    from_number: String,
    api_base: String,
    home_number: Option<String>,
    allowed_users: HashSet<String>,
}

impl SmsAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let account_sid = env::var("TWILIO_ACCOUNT_SID")
            .or_else(|_| env::var("FLYFLOR_TWILIO_ACCOUNT_SID"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if account_sid.is_empty() {
            return Err(ChannelError::missing_config(
                "TWILIO_ACCOUNT_SID is required for the sms channel",
            ));
        }
        let auth_token = env::var("TWILIO_AUTH_TOKEN")
            .or_else(|_| env::var("FLYFLOR_TWILIO_AUTH_TOKEN"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if auth_token.is_empty() {
            return Err(ChannelError::missing_config(
                "TWILIO_AUTH_TOKEN is required for the sms channel",
            ));
        }
        let from_number = env::var("TWILIO_FROM_NUMBER")
            .or_else(|_| env::var("FLYFLOR_TWILIO_FROM_NUMBER"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if from_number.is_empty() {
            return Err(ChannelError::missing_config(
                "TWILIO_FROM_NUMBER is required for the sms channel",
            ));
        }
        Ok(Self {
            account_sid,
            auth_token,
            from_number,
            api_base: env::var("TWILIO_API_BASE")
                .or_else(|_| env::var("FLYFLOR_TWILIO_API_BASE"))
                .unwrap_or_else(|_| DEFAULT_TWILIO_API_BASE.to_string())
                .trim()
                .trim_end_matches('/')
                .to_string(),
            home_number: env::var("SMS_HOME_NUMBER")
                .or_else(|_| env::var("FLYFLOR_SMS_HOME_NUMBER"))
                .ok()
                .map(|number| normalize_phone(&number))
                .filter(|number| !number.is_empty()),
            allowed_users: env_set("SMS_ALLOWED_USERS")
                .into_iter()
                .map(|number| normalize_phone(&number))
                .filter(|number| !number.is_empty())
                .collect(),
        })
    }

    fn messages_url(&self) -> String {
        format!(
            "{}/2010-04-01/Accounts/{}/Messages.json",
            self.api_base,
            url_encode_path(&self.account_sid)
        )
    }

    fn normalize_webhook(&self, value: &Value) -> Option<NormalizedInboundMessage> {
        let from = value_string_any(value, &["From", "from", "sender"])?;
        let from = normalize_phone(&from);
        if from.is_empty() {
            return None;
        }
        if !self.allowed_users.is_empty() && !self.allowed_users.contains(&from) {
            return None;
        }
        let text = value_string_any(value, &["Body", "body", "message", "text"])?
            .trim()
            .to_string();
        if text.is_empty() {
            return None;
        }
        let to = value_string_any(value, &["To", "to"])
            .map(|value| normalize_phone(&value))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| self.from_number.clone());
        let message_sid = value_string_any(value, &["MessageSid", "SmsSid", "id"])
            .unwrap_or_else(|| format!("sms-{}", now_millis()));
        let route = MessageRoute {
            platform: "sms".to_string(),
            chat_id: from.clone(),
            chat_type: ChatType::Direct,
            user_id: from.clone(),
            display_name: from.clone(),
            thread_id: from.clone(),
        };
        let metadata = json!({
            "channel": {
                "platform": "sms",
                "adapter": "twilio-rest",
                "chatId": from,
                "chatType": route.chat_type.as_gateway_str(),
                "userId": route.user_id,
                "sourceMessageId": message_sid,
                "to": to
            }
        });
        Some(NormalizedInboundMessage {
            id: format!("sms-{message_sid}"),
            text,
            route,
            context: None,
            metadata,
        })
    }
}

impl PlatformAdapter for SmsAdapter {
    fn name(&self) -> &'static str {
        "sms"
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
        let raw = env::var("SMS_INBOUND_WEBHOOK")
            .or_else(|_| env::var("FLYFLOR_SMS_INBOUND_WEBHOOK"))
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let Some(raw) = raw else {
            return Ok(Vec::new());
        };
        let value = parse_inbound_payload(&raw)?;
        Ok(self
            .normalize_webhook(&value)
            .into_iter()
            .collect::<Vec<_>>())
    }

    fn send_typing(&self, _route: &MessageRoute) -> ChannelResult<()> {
        Err(ChannelError::unavailable(
            "sms typing indicator is unavailable",
        ))
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal("sms message text must not be empty"));
        }
        if outbound_mentions_media(&message.text, message.metadata.as_ref()) {
            return self.send_media_unavailable("outbound");
        }
        let to = if message.route.chat_id.trim().is_empty() {
            self.home_number.clone().ok_or_else(|| {
                ChannelError::fatal("SMS_HOME_NUMBER is required when route chat_id is empty")
            })?
        } else {
            normalize_phone(&message.route.chat_id)
        };
        if to.is_empty() {
            return Err(ChannelError::fatal("sms destination must not be empty"));
        }
        let mut last_id = None;
        for chunk in split_text_chunks(&message.text, SMS_MAX_MESSAGE_LENGTH) {
            let response = twilio_post_message(
                &self.messages_url(),
                &self.account_sid,
                &self.auth_token,
                &self.from_number,
                &to,
                &chunk,
            )?;
            classify_twilio_response(&response)?;
            last_id = value_string_any(&response, &["sid", "messageSid", "id"])
                .or_else(|| Some(format!("sms-{}", now_millis())));
        }
        Ok(PlatformSendOutcome {
            message_id: last_id,
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "sms {media_kind} media delivery is explicitly unavailable in flyflor-cli"
        )))
    }
}

fn parse_inbound_payload(raw: &str) -> ChannelResult<Value> {
    if let Ok(value) = serde_json::from_str::<Value>(raw) {
        return Ok(value);
    }
    let mut object = serde_json::Map::new();
    for pair in raw.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        object.insert(percent_decode(key), Value::String(percent_decode(value)));
    }
    if object.is_empty() {
        return Err(ChannelError::fatal("sms inbound payload was empty"));
    }
    Ok(Value::Object(object))
}

fn twilio_post_message(
    url: &str,
    account_sid: &str,
    auth_token: &str,
    from: &str,
    to: &str,
    body: &str,
) -> ChannelResult<Value> {
    let output = Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            &seconds_arg(TWILIO_TIMEOUT_MS),
            "-X",
            "POST",
            url,
            "-u",
            &format!("{account_sid}:{auth_token}"),
            "--data-urlencode",
            &format!("From={from}"),
            "--data-urlencode",
            &format!("To={to}"),
            "--data-urlencode",
            &format!("Body={body}"),
        ])
        .output()
        .map_err(|error| ChannelError::unavailable(format!("curl is required: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr.trim();
        if message.contains("401") || message.contains("403") {
            return Err(ChannelError::session_expired(format!(
                "twilio authorization failed: {message}"
            )));
        }
        if message.contains("429") {
            return Err(ChannelError::rate_limited(format!(
                "twilio rate limited: {message}"
            )));
        }
        return Err(ChannelError::retryable(format!(
            "twilio curl failed with status {}: {}",
            output.status, message
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim()).map_err(|error| {
        ChannelError::retryable(format!(
            "twilio returned invalid JSON: {error}; body={}",
            stdout.trim()
        ))
    })
}

fn classify_twilio_response(value: &Value) -> ChannelResult<()> {
    let status = value
        .get("status_code")
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
        .unwrap_or("unknown twilio error");
    match status {
        401 | 403 => Err(ChannelError::session_expired(format!(
            "Twilio authorization failed: status={status} message={message}"
        ))),
        429 => Err(ChannelError::rate_limited(format!(
            "Twilio rate limited: status={status} message={message}"
        ))),
        400 | 404 => Err(ChannelError::fatal(format!(
            "Twilio bad request: status={status} message={message}"
        ))),
        _ => Err(ChannelError::retryable(format!(
            "Twilio error: status={status} message={message}"
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

fn normalize_phone(value: &str) -> String {
    value.trim().replace([' ', '-', '(', ')'], "")
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                output.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = &value[index + 1..index + 3];
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    output.push(byte);
                    index += 3;
                } else {
                    output.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&output).to_string()
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
    fn parses_json_and_form_inbound_payloads() {
        assert_eq!(
            value_string_any(
                &parse_inbound_payload(r#"{"From":"+15550001","Body":"hello"}"#).unwrap(),
                &["From"]
            ),
            Some("+15550001".to_string())
        );
        let form = parse_inbound_payload("From=%2B15550001&Body=hello+sms").unwrap();
        assert_eq!(
            value_string_any(&form, &["From"]),
            Some("+15550001".to_string())
        );
        assert_eq!(
            value_string_any(&form, &["Body"]),
            Some("hello sms".to_string())
        );
    }

    #[test]
    fn normalizes_twilio_webhook_payload() {
        let adapter = test_adapter();
        let message = adapter
            .normalize_webhook(&json!({
                "MessageSid": "SM1",
                "From": "+1 (555) 000-1",
                "To": "+15550002",
                "Body": "hello sms"
            }))
            .unwrap();

        assert_eq!(message.id, "sms-SM1");
        assert_eq!(message.text, "hello sms");
        assert_eq!(message.route.platform, "sms");
        assert_eq!(message.route.chat_id, "+15550001");
        assert_eq!(message.route.chat_type, ChatType::Direct);
        assert_eq!(
            message.metadata["channel"]["sourceMessageId"],
            Value::String("SM1".to_string())
        );
    }

    #[test]
    fn allowlist_blocks_unknown_phone() {
        let mut adapter = test_adapter();
        adapter.allowed_users = HashSet::from(["+15550002".to_string()]);

        assert!(
            adapter
                .normalize_webhook(&json!({
                    "MessageSid": "SM1",
                    "From": "+15550001",
                    "Body": "blocked"
                }))
                .is_none()
        );
    }

    #[test]
    fn classifies_twilio_error_codes() {
        assert_eq!(
            classify_twilio_response(&json!({
                "status_code": 401,
                "message": "unauthorized"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::SessionExpired
        );
        assert_eq!(
            classify_twilio_response(&json!({
                "status_code": 429,
                "message": "slow down"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::RateLimited
        );
        assert_eq!(
            classify_twilio_response(&json!({
                "status_code": 400,
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
        assert_eq!(url_encode_path("AC/test"), "AC%2Ftest");
    }

    fn test_adapter() -> SmsAdapter {
        SmsAdapter {
            account_sid: "AC123".to_string(),
            auth_token: "token".to_string(),
            from_number: "+15550000".to_string(),
            api_base: DEFAULT_TWILIO_API_BASE.to_string(),
            home_number: None,
            allowed_users: HashSet::new(),
        }
    }
}
