use std::{
    collections::HashSet,
    env,
    process::Command,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};

use super::platform::{
    ChannelCapabilityReport, ChannelCapabilityState, ChannelError, ChannelErrorKind, ChannelResult,
    ChatType, MessageRoute, NormalizedInboundMessage, OutboundMessage, PlatformAdapter,
    PlatformSendOutcome,
};

const MATRIX_TIMEOUT_MS: u64 = 20_000;
const MATRIX_MAX_MESSAGE_LENGTH: usize = 4_000;

pub struct MatrixAdapter {
    homeserver: String,
    access_token: String,
    user_id: String,
    allowed_users: HashSet<String>,
    since: Mutex<Option<String>>,
}

impl MatrixAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let homeserver = env::var("MATRIX_HOMESERVER")
            .or_else(|_| env::var("FLYFLOR_MATRIX_HOMESERVER"))
            .unwrap_or_default()
            .trim()
            .trim_end_matches('/')
            .to_string();
        if homeserver.is_empty() {
            return Err(ChannelError::missing_config(
                "MATRIX_HOMESERVER is required for the matrix channel",
            ));
        }
        let access_token = env::var("MATRIX_ACCESS_TOKEN")
            .or_else(|_| env::var("FLYFLOR_MATRIX_ACCESS_TOKEN"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if access_token.is_empty() {
            return Err(ChannelError::missing_config(
                "MATRIX_ACCESS_TOKEN is required for the matrix channel",
            ));
        }
        let user_id = env::var("MATRIX_USER_ID")
            .or_else(|_| env::var("FLYFLOR_MATRIX_USER_ID"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if user_id.is_empty() {
            return Err(ChannelError::missing_config(
                "MATRIX_USER_ID is required for the matrix channel",
            ));
        }
        Ok(Self {
            homeserver,
            access_token,
            user_id,
            allowed_users: env_set("MATRIX_ALLOWED_USERS"),
            since: Mutex::new(
                env::var("MATRIX_SYNC_SINCE")
                    .ok()
                    .filter(|value| !value.is_empty()),
            ),
        })
    }

    fn sync_url(&self) -> String {
        let since = self
            .since
            .lock()
            .ok()
            .and_then(|since| since.clone())
            .map(|since| format!("&since={}", url_encode_query(&since)))
            .unwrap_or_default();
        format!(
            "{}/_matrix/client/v3/sync?timeout=0{since}",
            self.homeserver
        )
    }

    fn send_url(&self, room_id: &str, chunk_index: usize) -> String {
        let txn_id = format!("{}-{chunk_index}", now_millis());
        format!(
            "{}/_matrix/client/v3/rooms/{}/send/m.room.message/{}",
            self.homeserver,
            url_encode_path(room_id),
            txn_id
        )
    }

    fn typing_url(&self, room_id: &str) -> String {
        format!(
            "{}/_matrix/client/v3/rooms/{}/typing/{}",
            self.homeserver,
            url_encode_path(room_id),
            url_encode_path(&self.user_id)
        )
    }

    fn normalize_event(&self, room_id: &str, event: &Value) -> Option<NormalizedInboundMessage> {
        if event.get("type").and_then(Value::as_str) != Some("m.room.message") {
            return None;
        }
        let sender = event.get("sender").and_then(Value::as_str)?;
        if sender == self.user_id {
            return None;
        }
        if !self.allowed_users.is_empty() && !self.allowed_users.contains(sender) {
            return None;
        }
        let content = event.get("content")?;
        let text = content
            .get("body")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())?
            .to_string();
        let event_id = event
            .get("event_id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("matrix-{}", now_millis()));
        let route = MessageRoute {
            platform: "matrix".to_string(),
            chat_id: room_id.to_string(),
            chat_type: ChatType::Group,
            user_id: sender.to_string(),
            display_name: sender.to_string(),
            thread_id: room_id.to_string(),
        };
        let metadata = json!({
            "channel": {
                "platform": "matrix",
                "adapter": "matrix-client-server-http",
                "roomId": room_id,
                "chatId": room_id,
                "chatType": route.chat_type.as_gateway_str(),
                "userId": sender,
                "sourceMessageId": event_id,
                "msgtype": content.get("msgtype").and_then(Value::as_str),
                "eventTs": event.get("origin_server_ts").and_then(Value::as_i64)
            }
        });
        Some(NormalizedInboundMessage {
            id: format!("matrix-{room_id}-{event_id}"),
            text,
            route,
            context: None,
            metadata,
        })
    }
}

impl PlatformAdapter for MatrixAdapter {
    fn name(&self) -> &'static str {
        "matrix"
    }

    fn capabilities(&self) -> ChannelCapabilityReport {
        ChannelCapabilityReport {
            send: ChannelCapabilityState::Available,
            typing: ChannelCapabilityState::Available,
            edit: ChannelCapabilityState::Unavailable,
            draft: ChannelCapabilityState::Unavailable,
            card: ChannelCapabilityState::Unavailable,
            media: ChannelCapabilityState::Unavailable,
        }
    }

    fn poll_updates(&self) -> ChannelResult<Vec<NormalizedInboundMessage>> {
        let response = matrix_get(&self.sync_url(), &self.access_token)?;
        classify_matrix_response(&response)?;
        if let Some(next_batch) = response.get("next_batch").and_then(Value::as_str)
            && let Ok(mut since) = self.since.lock()
        {
            *since = Some(next_batch.to_string());
        }
        Ok(parse_matrix_messages(&response, |room_id, event| {
            self.normalize_event(room_id, event)
        }))
    }

    fn send_typing(&self, route: &MessageRoute) -> ChannelResult<()> {
        let response = matrix_put(
            &self.typing_url(&route.chat_id),
            &self.access_token,
            json!({
                "typing": true,
                "timeout": 10_000
            }),
        )?;
        classify_matrix_response(&response)
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal("matrix message text must not be empty"));
        }
        if outbound_mentions_media(&message.text, message.metadata.as_ref()) {
            return self.send_media_unavailable("outbound");
        }
        let mut last_id = None;
        for (index, chunk) in split_text_chunks(&message.text, MATRIX_MAX_MESSAGE_LENGTH)
            .into_iter()
            .enumerate()
        {
            let response = matrix_put(
                &self.send_url(&message.route.chat_id, index),
                &self.access_token,
                json!({
                    "msgtype": "m.text",
                    "body": chunk
                }),
            )?;
            classify_matrix_response(&response)?;
            last_id = response
                .get("event_id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| Some(format!("matrix-{}", now_millis())));
        }
        Ok(PlatformSendOutcome {
            message_id: last_id,
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "matrix {media_kind} media delivery is explicitly unavailable in flyflor-cli"
        )))
    }
}

fn matrix_get(url: &str, token: &str) -> ChannelResult<Value> {
    run_matrix_curl(vec![
        "-sS".to_string(),
        "--max-time".to_string(),
        seconds_arg(MATRIX_TIMEOUT_MS),
        "-H".to_string(),
        format!("Authorization: Bearer {token}"),
        url.to_string(),
    ])
}

fn matrix_put(url: &str, token: &str, payload: Value) -> ChannelResult<Value> {
    let body =
        serde_json::to_string(&payload).map_err(|error| ChannelError::fatal(error.to_string()))?;
    run_matrix_curl(vec![
        "-sS".to_string(),
        "--max-time".to_string(),
        seconds_arg(MATRIX_TIMEOUT_MS),
        "-X".to_string(),
        "PUT".to_string(),
        url.to_string(),
        "-H".to_string(),
        format!("Authorization: Bearer {token}"),
        "-H".to_string(),
        "Content-Type: application/json".to_string(),
        "--data".to_string(),
        body,
    ])
}

fn run_matrix_curl(args: Vec<String>) -> ChannelResult<Value> {
    let output = Command::new("curl")
        .args(args)
        .output()
        .map_err(|error| ChannelError::unavailable(format!("curl is required: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr.trim();
        if message.contains("401") || message.contains("403") {
            return Err(ChannelError::session_expired(format!(
                "matrix authorization failed: {message}"
            )));
        }
        if message.contains("429") {
            return Err(ChannelError::rate_limited(format!(
                "matrix rate limited: {message}"
            )));
        }
        return Err(ChannelError::retryable(format!(
            "matrix curl failed with status {}: {}",
            output.status, message
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim()).map_err(|error| {
        ChannelError::retryable(format!(
            "matrix returned invalid JSON: {error}; body={}",
            stdout.trim()
        ))
    })
}

fn classify_matrix_response(value: &Value) -> ChannelResult<()> {
    let Some(error_code) = value.get("errcode").and_then(Value::as_str) else {
        return Ok(());
    };
    let message = value
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("unknown matrix error");
    match error_code {
        "M_FORBIDDEN" | "M_UNKNOWN_TOKEN" | "M_MISSING_TOKEN" => Err(
            ChannelError::session_expired(format!("{error_code}: {message}")),
        ),
        "M_LIMIT_EXCEEDED" => Err(ChannelError::rate_limited(format!(
            "{error_code}: {message}"
        ))),
        "M_BAD_JSON" | "M_NOT_JSON" | "M_INVALID_PARAM" => {
            Err(ChannelError::fatal(format!("{error_code}: {message}")))
        }
        _ => Err(ChannelError::retryable(format!("{error_code}: {message}"))),
    }
}

fn parse_matrix_messages<F>(response: &Value, mut normalize: F) -> Vec<NormalizedInboundMessage>
where
    F: FnMut(&str, &Value) -> Option<NormalizedInboundMessage>,
{
    let mut messages = Vec::new();
    let Some(rooms) = response
        .get("rooms")
        .and_then(|rooms| rooms.get("join"))
        .and_then(Value::as_object)
    else {
        return messages;
    };
    for (room_id, room) in rooms {
        let Some(events) = room
            .get("timeline")
            .and_then(|timeline| timeline.get("events"))
            .and_then(Value::as_array)
        else {
            continue;
        };
        for event in events {
            if let Some(message) = normalize(room_id, event) {
                messages.push(message);
            }
        }
    }
    messages
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
    url_encode(value)
}

fn url_encode_query(value: &str) -> String {
    url_encode(value)
}

fn url_encode(value: &str) -> String {
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
    fn normalizes_sync_events_and_filters_self_and_non_message() {
        let adapter = test_adapter();
        let messages = parse_matrix_messages(
            &json!({
                "next_batch": "s2",
                "rooms": {
                    "join": {
                        "!room:example.org": {
                            "timeline": {
                                "events": [
                                    {
                                        "type": "m.room.message",
                                        "event_id": "$event1",
                                        "sender": "@user:example.org",
                                        "origin_server_ts": 1770000000000_i64,
                                        "content": {
                                            "msgtype": "m.text",
                                            "body": "hello"
                                        }
                                    },
                                    {
                                        "type": "m.room.message",
                                        "event_id": "$event2",
                                        "sender": "@bot:example.org",
                                        "content": {
                                            "msgtype": "m.text",
                                            "body": "self"
                                        }
                                    },
                                    {
                                        "type": "m.room.member",
                                        "sender": "@user:example.org",
                                        "content": {}
                                    }
                                ]
                            }
                        }
                    }
                }
            }),
            |room_id, event| adapter.normalize_event(room_id, event),
        );

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id, "matrix-!room:example.org-$event1");
        assert_eq!(messages[0].text, "hello");
        assert_eq!(messages[0].route.platform, "matrix");
        assert_eq!(messages[0].route.chat_id, "!room:example.org");
        assert_eq!(messages[0].route.user_id, "@user:example.org");
        assert_eq!(
            messages[0]
                .metadata
                .get("channel")
                .and_then(|channel| channel.get("sourceMessageId"))
                .and_then(Value::as_str),
            Some("$event1")
        );
    }

    #[test]
    fn allowlist_blocks_unknown_sender() {
        let mut adapter = test_adapter();
        adapter.allowed_users = HashSet::from(["@allowed:example.org".to_string()]);

        assert!(
            adapter
                .normalize_event(
                    "!room:example.org",
                    &json!({
                        "type": "m.room.message",
                        "event_id": "$event1",
                        "sender": "@blocked:example.org",
                        "content": { "msgtype": "m.text", "body": "blocked" }
                    })
                )
                .is_none()
        );
    }

    #[test]
    fn classifies_matrix_error_codes() {
        assert_eq!(
            classify_matrix_response(&json!({
                "errcode": "M_UNKNOWN_TOKEN",
                "error": "token expired"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::SessionExpired
        );
        assert_eq!(
            classify_matrix_response(&json!({
                "errcode": "M_LIMIT_EXCEEDED",
                "error": "slow down"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::RateLimited
        );
        assert_eq!(
            classify_matrix_response(&json!({
                "errcode": "M_BAD_JSON",
                "error": "bad json"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::Fatal
        );
    }

    #[test]
    fn split_text_and_path_encoding_preserve_unicode() {
        assert_eq!(split_text_chunks("你好世界", 2), vec!["你好", "世界"]);
        assert_eq!(
            url_encode_path("!room:example.org"),
            "%21room%3Aexample.org"
        );
        assert_eq!(url_encode_query("s/1 + next"), "s%2F1%20%2B%20next");
    }

    fn test_adapter() -> MatrixAdapter {
        MatrixAdapter {
            homeserver: "http://127.0.0.1:8008".to_string(),
            access_token: "token".to_string(),
            user_id: "@bot:example.org".to_string(),
            allowed_users: HashSet::new(),
            since: Mutex::new(None),
        }
    }
}
