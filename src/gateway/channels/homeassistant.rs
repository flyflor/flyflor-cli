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
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};

use super::platform::{
    ChannelCapabilityReport, ChannelCapabilityState, ChannelError, ChannelErrorKind, ChannelResult,
    ChatType, MessageRoute, NormalizedInboundMessage, OutboundMessage, PlatformAdapter,
    PlatformSendOutcome,
};

const DEFAULT_HOME_ASSISTANT_BIND: &str = "127.0.0.1:8791";
const HOME_ASSISTANT_TIMEOUT_MS: u64 = 15_000;
const MAX_HTTP_BYTES: usize = 1024 * 1024;

pub struct HomeAssistantAdapter {
    base_url: String,
    token: String,
    webhook_secret: Option<String>,
    allowed_users: HashSet<String>,
    inbound_rx: Mutex<Receiver<NormalizedInboundMessage>>,
}

impl HomeAssistantAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let base_url = env::var("HOME_ASSISTANT_URL")
            .or_else(|_| env::var("HASS_URL"))
            .or_else(|_| env::var("FLYFLOR_HOME_ASSISTANT_URL"))
            .unwrap_or_default()
            .trim()
            .trim_end_matches('/')
            .to_string();
        if base_url.is_empty() {
            return Err(ChannelError::missing_config(
                "HOME_ASSISTANT_URL is required for the homeassistant channel",
            ));
        }
        let token = env::var("HOME_ASSISTANT_TOKEN")
            .or_else(|_| env::var("HASS_TOKEN"))
            .or_else(|_| env::var("FLYFLOR_HOME_ASSISTANT_TOKEN"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if token.is_empty() {
            return Err(ChannelError::missing_config(
                "HOME_ASSISTANT_TOKEN is required for the homeassistant channel",
            ));
        }
        let bind = env::var("HOME_ASSISTANT_WEBHOOK_BIND")
            .or_else(|_| env::var("FLYFLOR_HOME_ASSISTANT_WEBHOOK_BIND"))
            .unwrap_or_else(|_| DEFAULT_HOME_ASSISTANT_BIND.to_string());
        let webhook_secret = env::var("HOME_ASSISTANT_WEBHOOK_SECRET")
            .or_else(|_| env::var("FLYFLOR_HOME_ASSISTANT_WEBHOOK_SECRET"))
            .ok()
            .map(|secret| secret.trim().to_string())
            .filter(|secret| !secret.is_empty());
        let allowed_users = env_set("HOME_ASSISTANT_ALLOWED_USERS");
        let (tx, rx) = mpsc::channel();
        spawn_homeassistant_listener(bind, webhook_secret.clone(), allowed_users.clone(), tx)?;
        Ok(Self {
            base_url,
            token,
            webhook_secret,
            allowed_users,
            inbound_rx: Mutex::new(rx),
        })
    }

    fn conversation_url(&self) -> String {
        format!("{}/api/conversation/process", self.base_url)
    }

    fn normalize_payload(
        &self,
        payload: &Value,
        source: Option<&str>,
    ) -> ChannelResult<NormalizedInboundMessage> {
        let user_id = value_string(payload, "userId")
            .or_else(|| value_string(payload, "user_id"))
            .or_else(|| value_string(payload, "person_id"))
            .or_else(|| value_at_string(payload, &["event", "data", "user_id"]))
            .unwrap_or_else(|| source.unwrap_or("homeassistant-user").to_string());
        if !self.allowed_users.is_empty() && !self.allowed_users.contains(&user_id) {
            return Err(ChannelError::unavailable(format!(
                "homeassistant user {user_id} is not allowed"
            )));
        }
        let text = value_string(payload, "text")
            .or_else(|| value_string(payload, "message"))
            .or_else(|| value_string(payload, "query"))
            .or_else(|| value_string(payload, "sentence"))
            .or_else(|| value_at_string(payload, &["event", "data", "text"]))
            .or_else(|| value_at_string(payload, &["event", "data", "message"]))
            .ok_or_else(|| {
                ChannelError::fatal("homeassistant webhook payload requires text or message")
            })?
            .trim()
            .to_string();
        if text.is_empty() {
            return Err(ChannelError::fatal(
                "homeassistant webhook text must not be empty",
            ));
        }
        let conversation_id = value_string(payload, "conversationId")
            .or_else(|| value_string(payload, "conversation_id"))
            .or_else(|| value_string(payload, "chatId"))
            .or_else(|| value_at_string(payload, &["context", "id"]))
            .or_else(|| value_at_string(payload, &["event", "context", "id"]))
            .unwrap_or_else(|| "homeassistant".to_string());
        let id = value_string(payload, "id")
            .or_else(|| value_string(payload, "messageId"))
            .or_else(|| value_string(payload, "event_id"))
            .or_else(|| value_at_string(payload, &["event", "context", "id"]))
            .unwrap_or_else(|| format!("homeassistant-{}", now_millis()));
        let display_name = value_string(payload, "displayName")
            .or_else(|| value_string(payload, "display_name"))
            .or_else(|| value_string(payload, "name"))
            .unwrap_or_else(|| user_id.clone());
        let route = MessageRoute {
            platform: "homeassistant".to_string(),
            chat_id: conversation_id.clone(),
            chat_type: ChatType::Direct,
            user_id: user_id.clone(),
            display_name,
            thread_id: conversation_id.clone(),
        };
        let metadata = json!({
            "channel": {
                "platform": "homeassistant",
                "adapter": "homeassistant-rest-webhook",
                "chatId": conversation_id,
                "chatType": route.chat_type.as_gateway_str(),
                "userId": user_id,
                "source": source,
                "sourceMessageId": id
            },
            "homeassistant": payload.get("metadata").cloned().unwrap_or_else(|| json!({}))
        });
        Ok(NormalizedInboundMessage {
            id: format!("homeassistant-{id}"),
            text,
            route,
            context: payload.get("context").cloned(),
            metadata,
        })
    }
}

impl PlatformAdapter for HomeAssistantAdapter {
    fn name(&self) -> &'static str {
        "homeassistant"
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
        let Ok(rx) = self.inbound_rx.lock() else {
            return Err(ChannelError::fatal(
                "homeassistant inbound queue lock poisoned",
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
            "homeassistant typing indicator is unavailable",
        ))
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal(
                "homeassistant conversation text must not be empty",
            ));
        }
        if outbound_mentions_media(&message.text, message.metadata.as_ref()) {
            return self.send_media_unavailable("outbound");
        }
        let response = homeassistant_post(
            &self.conversation_url(),
            &self.token,
            json!({
                "text": message.text,
                "conversation_id": message.route.thread_id,
            }),
        )?;
        classify_homeassistant_response(&response)?;
        Ok(PlatformSendOutcome {
            message_id: value_string(&response, "conversation_id")
                .or_else(|| value_at_string(&response, &["response", "conversation_id"]))
                .or_else(|| Some(format!("homeassistant-{}", now_millis()))),
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "homeassistant {media_kind} media delivery is explicitly unavailable in flyflor-cli"
        )))
    }
}

fn spawn_homeassistant_listener(
    bind: String,
    webhook_secret: Option<String>,
    allowed_users: HashSet<String>,
    tx: mpsc::Sender<NormalizedInboundMessage>,
) -> ChannelResult<()> {
    let listener = TcpListener::bind(&bind).map_err(|error| {
        ChannelError::unavailable(format!("homeassistant bind {bind} failed: {error}"))
    })?;
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                continue;
            };
            let webhook_secret = webhook_secret.clone();
            let allowed_users = allowed_users.clone();
            let tx = tx.clone();
            thread::spawn(move || {
                handle_homeassistant_stream(&mut stream, webhook_secret, allowed_users, tx)
            });
        }
    });
    Ok(())
}

fn handle_homeassistant_stream(
    stream: &mut TcpStream,
    webhook_secret: Option<String>,
    allowed_users: HashSet<String>,
    tx: mpsc::Sender<NormalizedInboundMessage>,
) {
    let response = match read_http_request(stream).and_then(|request| {
        if let Some(secret) = webhook_secret.as_deref() {
            let header_secret = request
                .headers
                .get("x-flyflor-webhook-secret")
                .or_else(|| request.headers.get("x-homeassistant-secret"))
                .cloned();
            let bearer_secret = request
                .headers
                .get("authorization")
                .and_then(|value| value.strip_prefix("Bearer "))
                .map(str::to_string);
            if header_secret.as_deref() != Some(secret) && bearer_secret.as_deref() != Some(secret)
            {
                return Err(ChannelError::session_expired(
                    "homeassistant webhook secret did not match",
                ));
            }
        }
        let payload = serde_json::from_slice::<Value>(&request.body).map_err(|error| {
            ChannelError::fatal(format!("homeassistant JSON parse failed: {error}"))
        })?;
        let source = request
            .headers
            .get("x-flyflor-webhook-source")
            .or_else(|| request.headers.get("x-homeassistant-source"))
            .map(String::as_str);
        let adapter = HomeAssistantAdapter {
            base_url: String::new(),
            token: String::new(),
            webhook_secret,
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

fn homeassistant_post(url: &str, token: &str, payload: Value) -> ChannelResult<Value> {
    let body =
        serde_json::to_string(&payload).map_err(|error| ChannelError::fatal(error.to_string()))?;
    run_homeassistant_curl(vec![
        "-sS".to_string(),
        "--max-time".to_string(),
        seconds_arg(HOME_ASSISTANT_TIMEOUT_MS),
        "-X".to_string(),
        "POST".to_string(),
        url.to_string(),
        "-H".to_string(),
        format!("Authorization: Bearer {token}"),
        "-H".to_string(),
        "Content-Type: application/json".to_string(),
        "--data".to_string(),
        body,
    ])
}

fn run_homeassistant_curl(args: Vec<String>) -> ChannelResult<Value> {
    let output = Command::new("curl")
        .args(args)
        .output()
        .map_err(|error| ChannelError::unavailable(format!("curl is required: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr.trim();
        if message.contains("401") || message.contains("403") {
            return Err(ChannelError::session_expired(format!(
                "homeassistant authorization failed: {message}"
            )));
        }
        if message.contains("429") {
            return Err(ChannelError::rate_limited(format!(
                "homeassistant rate limited: {message}"
            )));
        }
        return Err(ChannelError::retryable(format!(
            "homeassistant curl failed with status {}: {}",
            output.status, message
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim()).map_err(|error| {
        ChannelError::retryable(format!(
            "homeassistant returned invalid JSON: {error}; body={}",
            stdout.trim()
        ))
    })
}

fn classify_homeassistant_response(value: &Value) -> ChannelResult<()> {
    let status = value
        .get("status_code")
        .or_else(|| value.get("statusCode"))
        .and_then(Value::as_i64);
    let code = value
        .get("code")
        .or_else(|| value.get("error"))
        .and_then(Value::as_str);
    if status.is_none() && code.is_none() {
        return Ok(());
    }
    if status.is_some_and(|status| status < 400) {
        return Ok(());
    }
    let status = status.unwrap_or_default();
    let message = value
        .get("message")
        .or_else(|| value.get("error_description"))
        .and_then(Value::as_str)
        .unwrap_or("unknown homeassistant error");
    match status {
        401 | 403 => Err(ChannelError::session_expired(format!(
            "Home Assistant authorization failed: status={status} message={message}"
        ))),
        429 => Err(ChannelError::rate_limited(format!(
            "Home Assistant rate limited: status={status} message={message}"
        ))),
        400 | 404 => Err(ChannelError::fatal(format!(
            "Home Assistant bad request: status={status} message={message}"
        ))),
        _ => Err(ChannelError::retryable(format!(
            "Home Assistant error: status={status} message={message}"
        ))),
    }
}

struct HttpRequest {
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn read_http_request(stream: &mut TcpStream) -> ChannelResult<HttpRequest> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let read = stream
            .read(&mut chunk)
            .map_err(|error| ChannelError::retryable(error.to_string()))?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > MAX_HTTP_BYTES {
            return Err(ChannelError::fatal("homeassistant HTTP request too large"));
        }
        if let Some(header_end) = find_header_end(&buffer) {
            let headers_text = String::from_utf8_lossy(&buffer[..header_end]);
            let headers = parse_headers(&headers_text);
            let content_length = headers
                .get("content-length")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            let body_start = header_end + 4;
            while buffer.len() < body_start + content_length {
                let read = stream
                    .read(&mut chunk)
                    .map_err(|error| ChannelError::retryable(error.to_string()))?;
                if read == 0 {
                    break;
                }
                buffer.extend_from_slice(&chunk[..read]);
                if buffer.len() > MAX_HTTP_BYTES {
                    return Err(ChannelError::fatal("homeassistant HTTP request too large"));
                }
            }
            return Ok(HttpRequest {
                headers,
                body: buffer[body_start..buffer.len().min(body_start + content_length)].to_vec(),
            });
        }
    }
    Err(ChannelError::fatal(
        "homeassistant HTTP request was incomplete",
    ))
}

fn parse_headers(text: &str) -> HashMap<String, String> {
    text.lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.trim().to_ascii_lowercase(), value.trim().to_string()))
        .collect()
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn http_response(status: u16, body: &str) -> String {
    let status_text = match status {
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        _ => "OK",
    };
    format!(
        "HTTP/1.1 {status} {status_text}\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn outbound_mentions_media(text: &str, metadata: Option<&Value>) -> bool {
    if text.contains("MEDIA:") {
        return true;
    }
    metadata
        .and_then(|value| value.get("media"))
        .is_some_and(|media| !media.is_null())
}

fn value_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn value_at_string(value: &Value, path: &[&str]) -> Option<String> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    cursor.as_str().map(str::to_string)
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
    fn normalizes_homeassistant_webhook_payload() {
        let adapter = test_adapter();
        let message = adapter
            .normalize_payload(
                &json!({
                    "id": "evt-1",
                    "text": "turn on the kitchen lights",
                    "conversation_id": "ha-convo-1",
                    "user_id": "person.alice",
                    "display_name": "Alice"
                }),
                Some("automation"),
            )
            .unwrap();

        assert_eq!(message.id, "homeassistant-evt-1");
        assert_eq!(message.text, "turn on the kitchen lights");
        assert_eq!(message.route.platform, "homeassistant");
        assert_eq!(message.route.chat_id, "ha-convo-1");
        assert_eq!(message.route.user_id, "person.alice");
        assert_eq!(
            message.metadata["channel"]["adapter"],
            "homeassistant-rest-webhook"
        );
    }

    #[test]
    fn normalizes_nested_event_data_payload() {
        let adapter = test_adapter();
        let message = adapter
            .normalize_payload(
                &json!({
                    "event": {
                        "context": { "id": "ctx-1" },
                        "data": {
                            "message": "nested message",
                            "user_id": "person.bob"
                        }
                    }
                }),
                None,
            )
            .unwrap();

        assert_eq!(message.text, "nested message");
        assert_eq!(message.route.thread_id, "ctx-1");
        assert_eq!(message.route.user_id, "person.bob");
    }

    #[test]
    fn allowlist_blocks_unknown_homeassistant_user() {
        let mut adapter = test_adapter();
        adapter.allowed_users = HashSet::from(["person.alice".to_string()]);

        assert_eq!(
            adapter
                .normalize_payload(
                    &json!({
                        "text": "blocked",
                        "user_id": "person.mallory"
                    }),
                    None,
                )
                .unwrap_err()
                .kind,
            ChannelErrorKind::Unavailable
        );
    }

    #[test]
    fn classifies_homeassistant_error_codes() {
        assert_eq!(
            classify_homeassistant_response(&json!({
                "status_code": 401,
                "message": "unauthorized"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::SessionExpired
        );
        assert_eq!(
            classify_homeassistant_response(&json!({
                "status_code": 429,
                "message": "slow down"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::RateLimited
        );
        assert_eq!(
            classify_homeassistant_response(&json!({
                "status_code": 400,
                "message": "bad"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::Fatal
        );
        assert!(classify_homeassistant_response(&json!({ "response": {} })).is_ok());
    }

    fn test_adapter() -> HomeAssistantAdapter {
        HomeAssistantAdapter {
            base_url: "http://127.0.0.1:8123".to_string(),
            token: "token".to_string(),
            webhook_secret: None,
            allowed_users: HashSet::new(),
            inbound_rx: Mutex::new(mpsc::channel().1),
        }
    }
}
