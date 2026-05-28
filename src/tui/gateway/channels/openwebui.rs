use std::{
    collections::{HashMap, HashSet},
    env,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::Command,
    sync::{
        Mutex,
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};

use super::platform::{
    ChannelCapabilityReport, ChannelCapabilityState, ChannelError, ChannelErrorKind, ChannelResult,
    ChatType, MessageRoute, NormalizedInboundMessage, OutboundMessage, PlatformAdapter,
    PlatformSendOutcome,
};

const DEFAULT_OPEN_WEBUI_BIND: &str = "127.0.0.1:8792";
const MAX_HTTP_BYTES: usize = 1024 * 1024;
const OPEN_WEBUI_TIMEOUT_MS: u64 = 15_000;

pub struct OpenWebuiAdapter {
    secret: String,
    callback_url: Option<String>,
    allowed_users: HashSet<String>,
    inbound_rx: Mutex<Receiver<NormalizedInboundMessage>>,
}

impl OpenWebuiAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let secret = env::var("OPEN_WEBUI_SECRET")
            .or_else(|_| env::var("FLYFLOR_OPEN_WEBUI_SECRET"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if secret.is_empty() {
            return Err(ChannelError::missing_config(
                "OPEN_WEBUI_SECRET is required for the open-webui channel",
            ));
        }
        let bind = env::var("OPEN_WEBUI_BIND")
            .or_else(|_| env::var("FLYFLOR_OPEN_WEBUI_BIND"))
            .unwrap_or_else(|_| DEFAULT_OPEN_WEBUI_BIND.to_string());
        let callback_url = env::var("OPEN_WEBUI_PUBLIC_URL")
            .or_else(|_| env::var("FLYFLOR_OPEN_WEBUI_PUBLIC_URL"))
            .ok()
            .map(|url| url.trim().to_string())
            .filter(|url| !url.is_empty());
        let allowed_users = env_set("OPEN_WEBUI_ALLOWED_USERS");
        let (tx, rx) = mpsc::channel();
        spawn_openwebui_listener(bind, secret.clone(), allowed_users.clone(), tx)?;
        Ok(Self {
            secret,
            callback_url,
            allowed_users,
            inbound_rx: Mutex::new(rx),
        })
    }

    fn normalize_payload(
        &self,
        payload: &Value,
        source: Option<&str>,
    ) -> ChannelResult<NormalizedInboundMessage> {
        let user_id = value_string(payload, "userId")
            .or_else(|| value_string(payload, "user_id"))
            .or_else(|| {
                payload
                    .get("user")
                    .and_then(|user| value_string(user, "id"))
            })
            .or_else(|| value_at_string(payload, &["message", "user_id"]))
            .unwrap_or_else(|| source.unwrap_or("open-webui-user").to_string());
        if !self.allowed_users.is_empty() && !self.allowed_users.contains(&user_id) {
            return Err(ChannelError::unavailable(format!(
                "open-webui user {user_id} is not allowed"
            )));
        }
        let text = value_string(payload, "text")
            .or_else(|| value_string(payload, "message"))
            .or_else(|| value_string(payload, "content"))
            .or_else(|| value_at_string(payload, &["message", "content"]))
            .or_else(|| value_at_string(payload, &["chat", "message", "content"]))
            .ok_or_else(|| ChannelError::fatal("open-webui payload requires text or content"))?
            .trim()
            .to_string();
        if text.is_empty() {
            return Err(ChannelError::fatal("open-webui text must not be empty"));
        }
        let chat_id = value_string(payload, "chatId")
            .or_else(|| value_string(payload, "chat_id"))
            .or_else(|| value_string(payload, "conversationId"))
            .or_else(|| value_string(payload, "conversation_id"))
            .or_else(|| value_at_string(payload, &["chat", "id"]))
            .unwrap_or_else(|| "open-webui".to_string());
        let id = value_string(payload, "id")
            .or_else(|| value_string(payload, "messageId"))
            .or_else(|| value_string(payload, "message_id"))
            .or_else(|| value_at_string(payload, &["message", "id"]))
            .unwrap_or_else(|| format!("open-webui-{}", now_millis()));
        let display_name = value_string(payload, "displayName")
            .or_else(|| value_string(payload, "display_name"))
            .or_else(|| {
                payload
                    .get("user")
                    .and_then(|user| value_string(user, "name"))
            })
            .unwrap_or_else(|| user_id.clone());
        let route = MessageRoute {
            platform: "open-webui".to_string(),
            chat_id: chat_id.clone(),
            chat_type: ChatType::Direct,
            user_id: user_id.clone(),
            display_name,
            thread_id: value_string(payload, "threadId")
                .or_else(|| value_string(payload, "thread_id"))
                .unwrap_or_else(|| chat_id.clone()),
        };
        let metadata = json!({
            "channel": {
                "platform": "open-webui",
                "adapter": "open-webui-webhook",
                "chatId": chat_id,
                "chatType": route.chat_type.as_gateway_str(),
                "userId": user_id,
                "source": source,
                "sourceMessageId": id
            },
            "openWebui": payload.get("metadata").cloned().unwrap_or_else(|| json!({}))
        });
        Ok(NormalizedInboundMessage {
            id: format!("open-webui-{id}"),
            text,
            route,
            context: payload.get("context").cloned(),
            metadata,
        })
    }
}

impl PlatformAdapter for OpenWebuiAdapter {
    fn name(&self) -> &'static str {
        "open-webui"
    }

    fn capabilities(&self) -> ChannelCapabilityReport {
        ChannelCapabilityReport {
            send: if self.callback_url.is_some() {
                ChannelCapabilityState::Available
            } else {
                ChannelCapabilityState::Degraded
            },
            typing: ChannelCapabilityState::Unavailable,
            edit: ChannelCapabilityState::Unavailable,
            draft: ChannelCapabilityState::Unavailable,
            card: ChannelCapabilityState::Unavailable,
            media: ChannelCapabilityState::Unavailable,
        }
    }

    fn poll_updates(&self) -> ChannelResult<Vec<NormalizedInboundMessage>> {
        let Ok(rx) = self.inbound_rx.lock() else {
            return Err(ChannelError::fatal(
                "open-webui inbound queue lock poisoned",
            ));
        };
        let mut messages = Vec::new();
        while let Ok(message) = rx.try_recv() {
            messages.push(message);
        }
        Ok(messages)
    }

    fn send_typing(&self, _route: &MessageRoute) -> ChannelResult<()> {
        Err(ChannelError::unavailable(
            "open-webui typing indicator is unavailable",
        ))
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        let Some(callback_url) = &self.callback_url else {
            return Err(ChannelError::unavailable(
                "OPEN_WEBUI_PUBLIC_URL is required for outbound Open WebUI replies",
            ));
        };
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal(
                "open-webui reply text must not be empty",
            ));
        }
        if outbound_mentions_media(&message.text, message.metadata.as_ref()) {
            return self.send_media_unavailable("outbound");
        }
        let response = openwebui_post(
            callback_url,
            &self.secret,
            json!({
                "text": message.text,
                "route": {
                    "platform": message.route.platform,
                    "chatId": message.route.chat_id,
                    "chatType": message.route.chat_type.as_gateway_str(),
                    "userId": message.route.user_id,
                    "displayName": message.route.display_name,
                    "threadId": message.route.thread_id
                },
                "replyToMessageId": message.reply_to_message_id,
                "metadata": message.metadata
            }),
        )?;
        Ok(PlatformSendOutcome {
            message_id: response
                .as_ref()
                .and_then(|value| {
                    value_string(value, "messageId").or_else(|| value_string(value, "id"))
                })
                .or_else(|| Some(format!("open-webui-{}", now_millis()))),
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "open-webui {media_kind} media delivery is explicitly unavailable in flyflor-cli"
        )))
    }
}

fn spawn_openwebui_listener(
    bind: String,
    secret: String,
    allowed_users: HashSet<String>,
    tx: mpsc::Sender<NormalizedInboundMessage>,
) -> ChannelResult<()> {
    let listener = TcpListener::bind(&bind).map_err(|error| {
        ChannelError::unavailable(format!("open-webui bind {bind} failed: {error}"))
    })?;
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                continue;
            };
            let secret = secret.clone();
            let allowed_users = allowed_users.clone();
            let tx = tx.clone();
            thread::spawn(move || handle_openwebui_stream(&mut stream, &secret, allowed_users, tx));
        }
    });
    Ok(())
}

fn handle_openwebui_stream(
    stream: &mut TcpStream,
    secret: &str,
    allowed_users: HashSet<String>,
    tx: mpsc::Sender<NormalizedInboundMessage>,
) {
    let response = match read_http_request(stream).and_then(|request| {
        let header_secret = request
            .headers
            .get("x-flyflor-webhook-secret")
            .or_else(|| request.headers.get("x-open-webui-secret"))
            .cloned();
        let bearer_secret = request
            .headers
            .get("authorization")
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(str::to_string);
        if header_secret.as_deref() != Some(secret) && bearer_secret.as_deref() != Some(secret) {
            return Err(ChannelError::session_expired(
                "open-webui request secret did not match",
            ));
        }
        let payload = serde_json::from_slice::<Value>(&request.body).map_err(|error| {
            ChannelError::fatal(format!("open-webui JSON parse failed: {error}"))
        })?;
        let source = request
            .headers
            .get("x-flyflor-webhook-source")
            .or_else(|| request.headers.get("x-open-webui-source"))
            .map(String::as_str);
        let adapter = OpenWebuiAdapter {
            secret: secret.to_string(),
            callback_url: None,
            allowed_users,
            inbound_rx: Mutex::new(mpsc::channel().1),
        };
        let message = adapter.normalize_payload(&payload, source)?;
        tx.send(message)
            .map_err(|error| ChannelError::retryable(error.to_string()))?;
        Ok(())
    }) {
        Ok(()) => http_response(202, "accepted"),
        Err(error) if error.kind == ChannelErrorKind::SessionExpired => {
            http_response(401, "unauthorized")
        }
        Err(error) if error.kind == ChannelErrorKind::Unavailable => {
            http_response(403, &error.message)
        }
        Err(error) => http_response(400, &error.message),
    };
    let _ = stream.write_all(response.as_bytes());
}

struct HttpRequest {
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn read_http_request(stream: &mut TcpStream) -> ChannelResult<HttpRequest> {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(1_000)));
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let mut header_end = None;
    loop {
        let count = stream
            .read(&mut chunk)
            .map_err(|error| ChannelError::retryable(error.to_string()))?;
        if count == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..count]);
        if buffer.len() > MAX_HTTP_BYTES {
            return Err(ChannelError::fatal(
                "open-webui request exceeded size limit",
            ));
        }
        if header_end.is_none() {
            header_end = find_header_end(&buffer);
        }
        if let Some(end) = header_end {
            let headers = parse_headers(&buffer[..end])?;
            let content_length = headers
                .get("content-length")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or_default();
            if buffer.len() >= end + 4 + content_length {
                return Ok(HttpRequest {
                    headers,
                    body: buffer[end + 4..end + 4 + content_length].to_vec(),
                });
            }
        }
    }
    Err(ChannelError::retryable("open-webui request ended early"))
}

fn parse_headers(raw: &[u8]) -> ChannelResult<HashMap<String, String>> {
    let text = std::str::from_utf8(raw).map_err(|error| {
        ChannelError::fatal(format!("open-webui headers were not UTF-8: {error}"))
    })?;
    let mut lines = text.lines();
    let Some(request_line) = lines.next() else {
        return Err(ChannelError::fatal("open-webui request line missing"));
    };
    if !request_line.starts_with("POST ") {
        return Err(ChannelError::fatal("open-webui only accepts POST"));
    }
    Ok(lines
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.trim().to_ascii_lowercase(), value.trim().to_string()))
        .collect())
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn http_response(status: u16, body: &str) -> String {
    let reason = match status {
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        _ => "Error",
    };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn openwebui_post(url: &str, secret: &str, payload: Value) -> ChannelResult<Option<Value>> {
    let body =
        serde_json::to_string(&payload).map_err(|error| ChannelError::fatal(error.to_string()))?;
    let output = Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            &seconds_arg(OPEN_WEBUI_TIMEOUT_MS),
            "-X",
            "POST",
            url,
            "-H",
            "Content-Type: application/json",
            "-H",
            &format!("X-Open-WebUI-Secret: {secret}"),
            "--data",
            &body,
        ])
        .output()
        .map_err(|error| ChannelError::unavailable(format!("curl is required: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ChannelError::retryable(format!(
            "open-webui callback failed with status {}: {}",
            output.status,
            stderr.trim()
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let text = stdout.trim();
    if text.is_empty() {
        return Ok(None);
    }
    serde_json::from_str::<Value>(text)
        .map(Some)
        .map_err(|error| {
            ChannelError::retryable(format!(
                "open-webui callback returned invalid JSON: {error}"
            ))
        })
}

fn value_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn value_at_string(value: &Value, path: &[&str]) -> Option<String> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    cursor
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
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
    fn normalizes_openwebui_payload_with_context_and_metadata() {
        let adapter = test_adapter(None, HashSet::new());
        let message = adapter
            .normalize_payload(
                &json!({
                    "id": "m-1",
                    "content": "hello",
                    "chat_id": "chat-1",
                    "user": { "id": "user-1", "name": "User One" },
                    "context": { "contextForkId": "fork-1" },
                    "metadata": { "confirmAnswer": { "choiceId": "continue-tools" } }
                }),
                Some("open-webui"),
            )
            .unwrap();

        assert_eq!(message.id, "open-webui-m-1");
        assert_eq!(message.text, "hello");
        assert_eq!(message.route.platform, "open-webui");
        assert_eq!(message.route.chat_id, "chat-1");
        assert_eq!(message.route.user_id, "user-1");
        assert_eq!(
            message
                .context
                .and_then(|context| value_string(&context, "contextForkId")),
            Some("fork-1".to_string())
        );
        assert_eq!(
            message
                .metadata
                .get("openWebui")
                .and_then(|metadata| metadata.get("confirmAnswer"))
                .and_then(|answer| answer.get("choiceId"))
                .and_then(Value::as_str),
            Some("continue-tools")
        );
    }

    #[test]
    fn normalizes_nested_message_payload() {
        let adapter = test_adapter(None, HashSet::new());
        let message = adapter
            .normalize_payload(
                &json!({
                    "chat": {
                        "id": "chat-2",
                        "message": { "content": "nested hello" }
                    },
                    "message": {
                        "id": "msg-2",
                        "user_id": "user-2"
                    }
                }),
                None,
            )
            .unwrap();

        assert_eq!(message.id, "open-webui-msg-2");
        assert_eq!(message.text, "nested hello");
        assert_eq!(message.route.chat_id, "chat-2");
        assert_eq!(message.route.user_id, "user-2");
    }

    #[test]
    fn allowlist_blocks_unknown_openwebui_user() {
        let adapter = test_adapter(None, HashSet::from(["allowed".to_string()]));
        let error = adapter
            .normalize_payload(&json!({ "text": "hello", "user_id": "blocked" }), None)
            .unwrap_err();

        assert_eq!(error.kind, ChannelErrorKind::Unavailable);
    }

    #[test]
    fn capabilities_are_degraded_without_callback_url() {
        let adapter = test_adapter(None, HashSet::new());

        assert_eq!(
            adapter.capabilities().send,
            ChannelCapabilityState::Degraded
        );
        assert_eq!(
            adapter
                .send_message(OutboundMessage {
                    route: test_route(),
                    text: "hello".to_string(),
                    reply_to_message_id: None,
                    metadata: None,
                })
                .unwrap_err()
                .kind,
            ChannelErrorKind::Unavailable
        );
    }

    #[test]
    fn parses_http_request_and_secret_header() {
        let raw = b"POST / HTTP/1.1\r\nHost: local\r\nX-Open-WebUI-Secret: s\r\nContent-Length: 16\r\n\r\n{\"text\":\"hello\"}";
        let end = find_header_end(raw).unwrap();
        let headers = parse_headers(&raw[..end]).unwrap();

        assert_eq!(
            headers.get("x-open-webui-secret").map(String::as_str),
            Some("s")
        );
    }

    fn test_adapter(
        callback_url: Option<String>,
        allowed_users: HashSet<String>,
    ) -> OpenWebuiAdapter {
        OpenWebuiAdapter {
            secret: "secret".to_string(),
            callback_url,
            allowed_users,
            inbound_rx: Mutex::new(mpsc::channel().1),
        }
    }

    fn test_route() -> MessageRoute {
        MessageRoute {
            platform: "open-webui".to_string(),
            chat_id: "chat-1".to_string(),
            chat_type: ChatType::Direct,
            user_id: "user-1".to_string(),
            display_name: "User One".to_string(),
            thread_id: "chat-1".to_string(),
        }
    }
}
