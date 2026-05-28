use std::{
    collections::HashSet,
    env,
    io::{Read, Write},
    net::TcpStream,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose};
use serde_json::{Value, json};

use super::platform::{
    ChannelCapabilityReport, ChannelCapabilityState, ChannelError, ChannelErrorKind, ChannelResult,
    ChatType, MessageRoute, NormalizedInboundMessage, OutboundMessage, PlatformAdapter,
    PlatformSendOutcome,
};

const DEFAULT_SMTP_PORT: u16 = 25;
const EMAIL_MAX_MESSAGE_LENGTH: usize = 8_000;
const SMTP_TIMEOUT_MS: u64 = 15_000;

pub struct EmailAdapter {
    address: String,
    password: String,
    smtp_host: String,
    smtp_port: u16,
    home_address: Option<String>,
    allowed_users: HashSet<String>,
}

impl EmailAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let address = env::var("EMAIL_ADDRESS")
            .or_else(|_| env::var("FLYFLOR_EMAIL_ADDRESS"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if address.is_empty() {
            return Err(ChannelError::missing_config(
                "EMAIL_ADDRESS is required for the email channel",
            ));
        }
        let password = env::var("EMAIL_PASSWORD")
            .or_else(|_| env::var("FLYFLOR_EMAIL_PASSWORD"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if password.is_empty() {
            return Err(ChannelError::missing_config(
                "EMAIL_PASSWORD is required for the email channel",
            ));
        }
        let smtp_host = env::var("EMAIL_SMTP_HOST")
            .or_else(|_| env::var("FLYFLOR_EMAIL_SMTP_HOST"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if smtp_host.is_empty() {
            return Err(ChannelError::missing_config(
                "EMAIL_SMTP_HOST is required for the email channel",
            ));
        }
        Ok(Self {
            address: normalize_email(&address),
            password,
            smtp_host,
            smtp_port: env_u16("EMAIL_SMTP_PORT", DEFAULT_SMTP_PORT),
            home_address: env::var("EMAIL_HOME_ADDRESS")
                .or_else(|_| env::var("FLYFLOR_EMAIL_HOME_ADDRESS"))
                .ok()
                .map(|value| normalize_email(&value))
                .filter(|value| !value.is_empty()),
            allowed_users: env_set("EMAIL_ALLOWED_USERS")
                .into_iter()
                .map(|value| normalize_email(&value))
                .filter(|value| !value.is_empty())
                .collect(),
        })
    }

    fn smtp_addr(&self) -> String {
        format!("{}:{}", self.smtp_host, self.smtp_port)
    }

    fn normalize_payload(&self, value: &Value) -> Option<NormalizedInboundMessage> {
        let from = value_string_any(value, &["from", "From", "sender", "replyTo", "reply_to"])
            .map(|value| normalize_email(&value))?;
        if from.is_empty() {
            return None;
        }
        if !self.allowed_users.is_empty() && !self.allowed_users.contains(&from) {
            return None;
        }
        if from == self.address {
            return None;
        }
        let text = value_string_any(value, &["text", "body", "plain", "message"])?
            .trim()
            .to_string();
        if text.is_empty() {
            return None;
        }
        let message_id = value_string_any(value, &["messageId", "message_id", "id"])
            .unwrap_or_else(|| format!("email-{}", now_millis()));
        let subject = value_string_any(value, &["subject", "Subject"]).unwrap_or_default();
        let thread_id = value_string_any(
            value,
            &["threadId", "thread_id", "inReplyTo", "in_reply_to"],
        )
        .unwrap_or_else(|| from.clone());
        let route = MessageRoute {
            platform: "email".to_string(),
            chat_id: from.clone(),
            chat_type: ChatType::Direct,
            user_id: from.clone(),
            display_name: value_string_any(value, &["displayName", "display_name", "name"])
                .unwrap_or_else(|| from.clone()),
            thread_id: thread_id.clone(),
        };
        let metadata = json!({
            "channel": {
                "platform": "email",
                "adapter": "smtp-env-payload",
                "chatId": from,
                "chatType": route.chat_type.as_gateway_str(),
                "userId": route.user_id,
                "sourceMessageId": message_id,
                "subject": subject,
                "threadId": thread_id,
                "to": value_string_any(value, &["to", "To"]).map(|value| normalize_email(&value))
            }
        });
        Some(NormalizedInboundMessage {
            id: format!("email-{message_id}"),
            text,
            route,
            context: value.get("context").cloned(),
            metadata,
        })
    }
}

impl PlatformAdapter for EmailAdapter {
    fn name(&self) -> &'static str {
        "email"
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
        let raw = env::var("EMAIL_INBOUND_MESSAGE")
            .or_else(|_| env::var("FLYFLOR_EMAIL_INBOUND_MESSAGE"))
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let Some(raw) = raw else {
            return Ok(Vec::new());
        };
        let value = serde_json::from_str::<Value>(&raw).map_err(|error| {
            ChannelError::fatal(format!("email inbound JSON parse failed: {error}"))
        })?;
        let messages = if let Some(items) = value.get("messages").and_then(Value::as_array) {
            items
                .iter()
                .filter_map(|item| self.normalize_payload(item))
                .collect()
        } else {
            self.normalize_payload(&value).into_iter().collect()
        };
        Ok(messages)
    }

    fn send_typing(&self, _route: &MessageRoute) -> ChannelResult<()> {
        Err(ChannelError::unavailable(
            "email typing indicator is unavailable",
        ))
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal("email message text must not be empty"));
        }
        if outbound_mentions_media(&message.text, message.metadata.as_ref()) {
            return self.send_media_unavailable("outbound");
        }
        let to = if message.route.chat_id.trim().is_empty() {
            self.home_address.clone().ok_or_else(|| {
                ChannelError::fatal("EMAIL_HOME_ADDRESS is required when route chat_id is empty")
            })?
        } else {
            normalize_email(&message.route.chat_id)
        };
        if to.is_empty() {
            return Err(ChannelError::fatal("email destination must not be empty"));
        }
        let subject = outbound_subject(message.metadata.as_ref());
        let mut last_id = None;
        for chunk in split_text_chunks(&message.text, EMAIL_MAX_MESSAGE_LENGTH) {
            let message_id = format!("flyflor-email-{}@local", now_millis());
            smtp_send_text(
                &self.smtp_addr(),
                &self.address,
                &self.password,
                &to,
                &subject,
                &chunk,
                &message_id,
                message.reply_to_message_id.as_deref(),
            )?;
            last_id = Some(message_id);
        }
        Ok(PlatformSendOutcome {
            message_id: last_id,
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "email {media_kind} media delivery is explicitly unavailable in flyflor-cli"
        )))
    }
}

fn smtp_send_text(
    addr: &str,
    from: &str,
    password: &str,
    to: &str,
    subject: &str,
    body: &str,
    message_id: &str,
    in_reply_to: Option<&str>,
) -> ChannelResult<()> {
    let mut stream = TcpStream::connect(addr).map_err(|error| {
        ChannelError::retryable(format!("email SMTP connect {addr} failed: {error}"))
    })?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(SMTP_TIMEOUT_MS)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(SMTP_TIMEOUT_MS)));
    expect_smtp(&mut stream, &[220])?;
    smtp_command(&mut stream, "EHLO flyflor-cli\r\n", &[250])?;
    smtp_command(&mut stream, "AUTH LOGIN\r\n", &[334])?;
    smtp_command(
        &mut stream,
        &format!("{}\r\n", general_purpose::STANDARD.encode(from)),
        &[334],
    )?;
    smtp_command(
        &mut stream,
        &format!("{}\r\n", general_purpose::STANDARD.encode(password)),
        &[235],
    )?;
    smtp_command(&mut stream, &format!("MAIL FROM:<{from}>\r\n"), &[250])?;
    smtp_command(&mut stream, &format!("RCPT TO:<{to}>\r\n"), &[250, 251])?;
    smtp_command(&mut stream, "DATA\r\n", &[354])?;
    let data = email_data(from, to, subject, body, message_id, in_reply_to);
    stream.write_all(data.as_bytes()).map_err(|error| {
        ChannelError::retryable(format!("email SMTP DATA write failed: {error}"))
    })?;
    expect_smtp(&mut stream, &[250])?;
    let _ = smtp_command(&mut stream, "QUIT\r\n", &[221]);
    Ok(())
}

fn smtp_command(stream: &mut TcpStream, command: &str, expected: &[u16]) -> ChannelResult<String> {
    stream
        .write_all(command.as_bytes())
        .map_err(|error| ChannelError::retryable(format!("email SMTP write failed: {error}")))?;
    expect_smtp(stream, expected)
}

fn expect_smtp(stream: &mut TcpStream, expected: &[u16]) -> ChannelResult<String> {
    let response = read_smtp_response(stream)?;
    let code = response
        .get(0..3)
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or_default();
    if expected.contains(&code) {
        return Ok(response);
    }
    match code {
        401 | 530 | 535 => Err(ChannelError::session_expired(format!(
            "email SMTP authorization failed: {}",
            response.trim()
        ))),
        421 | 450 | 451 | 452 => Err(ChannelError::rate_limited(format!(
            "email SMTP temporarily unavailable: {}",
            response.trim()
        ))),
        500..=599 => Err(ChannelError::fatal(format!(
            "email SMTP rejected command: {}",
            response.trim()
        ))),
        _ => Err(ChannelError::retryable(format!(
            "email SMTP unexpected response: {}",
            response.trim()
        ))),
    }
}

fn read_smtp_response(stream: &mut TcpStream) -> ChannelResult<String> {
    let mut response = String::new();
    let mut line = Vec::new();
    let mut code = None;
    loop {
        line.clear();
        loop {
            let mut byte = [0u8; 1];
            let count = stream.read(&mut byte).map_err(|error| {
                ChannelError::retryable(format!("email SMTP read failed: {error}"))
            })?;
            if count == 0 {
                return Err(ChannelError::retryable(
                    "email SMTP connection closed while reading response",
                ));
            }
            line.push(byte[0]);
            if byte[0] == b'\n' {
                break;
            }
        }
        let text = String::from_utf8_lossy(&line);
        response.push_str(&text);
        if line.len() >= 4 {
            let line_code = std::str::from_utf8(&line[..3])
                .ok()
                .and_then(|value| value.parse::<u16>().ok());
            if code.is_none() {
                code = line_code;
            }
            if line_code == code && line[3] == b' ' {
                return Ok(response);
            }
            if line_code == code && line[3] != b'-' {
                return Ok(response);
            }
        } else {
            return Ok(response);
        }
    }
}

fn email_data(
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
    message_id: &str,
    in_reply_to: Option<&str>,
) -> String {
    let mut headers = vec![
        format!("From: {from}"),
        format!("To: {to}"),
        format!("Subject: {}", sanitize_header(subject)),
        format!("Message-ID: <{message_id}>"),
        "MIME-Version: 1.0".to_string(),
        "Content-Type: text/plain; charset=UTF-8".to_string(),
        "Content-Transfer-Encoding: 8bit".to_string(),
    ];
    if let Some(in_reply_to) = in_reply_to.filter(|value| !value.trim().is_empty()) {
        headers.push(format!("In-Reply-To: <{}>", sanitize_header(in_reply_to)));
        headers.push(format!("References: <{}>", sanitize_header(in_reply_to)));
    }
    format!(
        "{}\r\n\r\n{}\r\n.\r\n",
        headers.join("\r\n"),
        dot_stuff(body)
    )
}

fn outbound_subject(metadata: Option<&Value>) -> String {
    metadata
        .and_then(|metadata| metadata.get("channel"))
        .and_then(|channel| channel.get("subject"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|subject| {
            if subject.to_ascii_lowercase().starts_with("re:") {
                subject.to_string()
            } else {
                format!("Re: {subject}")
            }
        })
        .unwrap_or_else(|| "Re: Flyflor".to_string())
}

fn dot_stuff(body: &str) -> String {
    body.replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(|line| {
            if line.starts_with('.') {
                format!(".{line}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\r\n")
}

fn sanitize_header(value: &str) -> String {
    value.replace(['\r', '\n'], " ").trim().to_string()
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

fn env_u16(name: &str, default: u16) -> u16 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(default)
}

fn normalize_email(value: &str) -> String {
    value.trim().to_ascii_lowercase()
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
    fn normalizes_inbound_email_payload() {
        let adapter = test_adapter();
        let message = adapter
            .normalize_payload(&json!({
                "id": "email-1",
                "from": "User@Example.COM",
                "to": "bot@example.com",
                "subject": "Hello",
                "text": "hello email",
                "context": { "contextForkId": "fork-email" }
            }))
            .unwrap();

        assert_eq!(message.id, "email-email-1");
        assert_eq!(message.text, "hello email");
        assert_eq!(message.route.platform, "email");
        assert_eq!(message.route.chat_id, "user@example.com");
        assert_eq!(message.route.chat_type, ChatType::Direct);
        assert_eq!(message.metadata["channel"]["subject"], "Hello");
        assert_eq!(
            message
                .context
                .and_then(|context| value_string_any(&context, &["contextForkId"])),
            Some("fork-email".to_string())
        );
    }

    #[test]
    fn allowlist_and_self_loop_filter_email() {
        let mut adapter = test_adapter();
        adapter.allowed_users = HashSet::from(["allowed@example.com".to_string()]);

        assert!(
            adapter
                .normalize_payload(&json!({
                    "id": "email-1",
                    "from": "blocked@example.com",
                    "text": "blocked"
                }))
                .is_none()
        );
        assert!(
            test_adapter()
                .normalize_payload(&json!({
                    "id": "email-2",
                    "from": "bot@example.com",
                    "text": "self"
                }))
                .is_none()
        );
    }

    #[test]
    fn formats_smtp_data_with_reply_headers_and_dot_stuffing() {
        let data = email_data(
            "bot@example.com",
            "user@example.com",
            "Re: Hello",
            ".first\nsecond",
            "msg-1@local",
            Some("source-1"),
        );

        assert!(data.contains("From: bot@example.com\r\n"));
        assert!(data.contains("To: user@example.com\r\n"));
        assert!(data.contains("Subject: Re: Hello\r\n"));
        assert!(data.contains("In-Reply-To: <source-1>\r\n"));
        assert!(data.contains("\r\n..first\r\nsecond\r\n.\r\n"));
    }

    #[test]
    fn subject_and_chunks_preserve_unicode() {
        assert_eq!(split_text_chunks("你好世界", 2), vec!["你好", "世界"]);
        assert_eq!(
            outbound_subject(Some(&json!({ "channel": { "subject": "Hello" } }))),
            "Re: Hello"
        );
        assert_eq!(
            outbound_subject(Some(&json!({ "channel": { "subject": "Re: Hello" } }))),
            "Re: Hello"
        );
    }

    fn test_adapter() -> EmailAdapter {
        EmailAdapter {
            address: "bot@example.com".to_string(),
            password: "password".to_string(),
            smtp_host: "127.0.0.1".to_string(),
            smtp_port: DEFAULT_SMTP_PORT,
            home_address: None,
            allowed_users: HashSet::new(),
        }
    }
}
