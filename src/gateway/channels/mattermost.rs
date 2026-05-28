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

const MATTERMOST_TIMEOUT_MS: u64 = 15_000;
const MATTERMOST_MAX_MESSAGE_LENGTH: usize = 3_900;

pub struct MattermostAdapter {
    base_url: String,
    token: String,
    channel_id: String,
    allowed_users: HashSet<String>,
    last_create_at: Mutex<i64>,
}

impl MattermostAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let base_url = env::var("MATTERMOST_URL")
            .or_else(|_| env::var("FLYFLOR_MATTERMOST_URL"))
            .unwrap_or_default()
            .trim()
            .trim_end_matches('/')
            .to_string();
        if base_url.is_empty() {
            return Err(ChannelError::missing_config(
                "MATTERMOST_URL is required for the mattermost channel",
            ));
        }
        let token = env::var("MATTERMOST_TOKEN")
            .or_else(|_| env::var("FLYFLOR_MATTERMOST_TOKEN"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if token.is_empty() {
            return Err(ChannelError::missing_config(
                "MATTERMOST_TOKEN is required for the mattermost channel",
            ));
        }
        let channel_id = env::var("MATTERMOST_CHANNEL")
            .or_else(|_| env::var("MATTERMOST_HOME_CHANNEL"))
            .or_else(|_| env::var("FLYFLOR_MATTERMOST_CHANNEL"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if channel_id.is_empty() {
            return Err(ChannelError::missing_config(
                "MATTERMOST_CHANNEL is required for the mattermost channel",
            ));
        }
        Ok(Self {
            base_url,
            token,
            channel_id,
            allowed_users: env_set("MATTERMOST_ALLOWED_USERS"),
            last_create_at: Mutex::new(env_i64("MATTERMOST_SINCE_CREATE_AT", 0)),
        })
    }

    fn posts_url(&self) -> String {
        format!(
            "{}/api/v4/channels/{}/posts?per_page=20",
            self.base_url,
            url_encode_path(&self.channel_id)
        )
    }

    fn create_post_url(&self) -> String {
        format!("{}/api/v4/posts", self.base_url)
    }

    fn normalize_post(&self, post: &Value) -> Option<NormalizedInboundMessage> {
        let id = value_string(post, "id")?;
        let user_id = value_string(post, "user_id")?;
        if !self.allowed_users.is_empty() && !self.allowed_users.contains(&user_id) {
            return None;
        }
        let text = value_string(post, "message")?.trim().to_string();
        if text.is_empty() {
            return None;
        }
        let channel_id =
            value_string(post, "channel_id").unwrap_or_else(|| self.channel_id.clone());
        let create_at = post
            .get("create_at")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        let route = MessageRoute {
            platform: "mattermost".to_string(),
            chat_id: channel_id.clone(),
            chat_type: ChatType::Group,
            user_id: user_id.clone(),
            display_name: user_id.clone(),
            thread_id: value_string(post, "root_id")
                .filter(|root_id| !root_id.is_empty())
                .unwrap_or_else(|| channel_id.clone()),
        };
        let metadata = json!({
            "channel": {
                "platform": "mattermost",
                "adapter": "mattermost-rest",
                "chatId": channel_id,
                "chatType": route.chat_type.as_gateway_str(),
                "userId": user_id,
                "sourceMessageId": id,
                "rootId": value_string(post, "root_id"),
                "createAt": create_at
            }
        });
        Some(NormalizedInboundMessage {
            id: format!("mattermost-{id}"),
            text,
            route,
            context: None,
            metadata,
        })
    }
}

impl PlatformAdapter for MattermostAdapter {
    fn name(&self) -> &'static str {
        "mattermost"
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
        let response = mattermost_get(&self.posts_url(), &self.token)?;
        classify_mattermost_response(&response)?;
        let mut posts = parse_posts(&response);
        posts.sort_by_key(|post| {
            post.get("create_at")
                .and_then(Value::as_i64)
                .unwrap_or_default()
        });
        let last_seen = self.last_create_at.lock().map(|value| *value).unwrap_or(0);
        let mut next_last_seen = last_seen;
        let messages = posts
            .into_iter()
            .filter(|post| {
                let create_at = post
                    .get("create_at")
                    .and_then(Value::as_i64)
                    .unwrap_or_default();
                if create_at > next_last_seen {
                    next_last_seen = create_at;
                }
                create_at > last_seen
            })
            .filter_map(|post| self.normalize_post(&post))
            .collect();
        if let Ok(mut last_create_at) = self.last_create_at.lock() {
            *last_create_at = next_last_seen;
        }
        Ok(messages)
    }

    fn send_typing(&self, _route: &MessageRoute) -> ChannelResult<()> {
        Err(ChannelError::unavailable(
            "mattermost typing indicator is unavailable in the REST adapter",
        ))
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal(
                "mattermost message text must not be empty",
            ));
        }
        if outbound_mentions_media(&message.text, message.metadata.as_ref()) {
            return self.send_media_unavailable("outbound");
        }
        let mut last_id = None;
        for chunk in split_text_chunks(&message.text, MATTERMOST_MAX_MESSAGE_LENGTH) {
            let response = mattermost_post(
                &self.create_post_url(),
                &self.token,
                json!({
                    "channel_id": message.route.chat_id,
                    "message": chunk,
                    "root_id": message.reply_to_message_id
                }),
            )?;
            classify_mattermost_response(&response)?;
            last_id = value_string(&response, "id")
                .or_else(|| Some(format!("mattermost-{}", now_millis())));
        }
        Ok(PlatformSendOutcome {
            message_id: last_id,
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "mattermost {media_kind} media delivery is explicitly unavailable in flyflor-cli"
        )))
    }
}

fn mattermost_get(url: &str, token: &str) -> ChannelResult<Value> {
    run_mattermost_curl(vec![
        "-sS".to_string(),
        "--max-time".to_string(),
        seconds_arg(MATTERMOST_TIMEOUT_MS),
        "-H".to_string(),
        format!("Authorization: Bearer {token}"),
        url.to_string(),
    ])
}

fn mattermost_post(url: &str, token: &str, payload: Value) -> ChannelResult<Value> {
    let body =
        serde_json::to_string(&payload).map_err(|error| ChannelError::fatal(error.to_string()))?;
    run_mattermost_curl(vec![
        "-sS".to_string(),
        "--max-time".to_string(),
        seconds_arg(MATTERMOST_TIMEOUT_MS),
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

fn run_mattermost_curl(args: Vec<String>) -> ChannelResult<Value> {
    let output = Command::new("curl")
        .args(args)
        .output()
        .map_err(|error| ChannelError::unavailable(format!("curl is required: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr.trim();
        if message.contains("401") || message.contains("403") {
            return Err(ChannelError::session_expired(format!(
                "mattermost authorization failed: {message}"
            )));
        }
        if message.contains("429") {
            return Err(ChannelError::rate_limited(format!(
                "mattermost rate limited: {message}"
            )));
        }
        return Err(ChannelError::retryable(format!(
            "mattermost curl failed with status {}: {}",
            output.status, message
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim()).map_err(|error| {
        ChannelError::retryable(format!(
            "mattermost returned invalid JSON: {error}; body={}",
            stdout.trim()
        ))
    })
}

fn classify_mattermost_response(value: &Value) -> ChannelResult<()> {
    let status = value
        .get("status_code")
        .or_else(|| value.get("statusCode"))
        .and_then(Value::as_i64);
    if status.is_none() || status.is_some_and(|status| status < 400) {
        return Ok(());
    }
    let status = status.unwrap_or_default();
    let message = value
        .get("message")
        .or_else(|| value.get("error"))
        .and_then(Value::as_str)
        .unwrap_or("unknown mattermost error");
    match status {
        401 | 403 => Err(ChannelError::session_expired(format!(
            "Mattermost authorization failed: status={status} message={message}"
        ))),
        429 => Err(ChannelError::rate_limited(format!(
            "Mattermost rate limited: status={status} message={message}"
        ))),
        400 | 404 => Err(ChannelError::fatal(format!(
            "Mattermost bad request: status={status} message={message}"
        ))),
        _ => Err(ChannelError::retryable(format!(
            "Mattermost error: status={status} message={message}"
        ))),
    }
}

fn parse_posts(response: &Value) -> Vec<Value> {
    response
        .get("order")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|id| {
            response
                .get("posts")
                .and_then(|posts| posts.get(id))
                .cloned()
        })
        .collect()
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

fn value_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
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

fn env_i64(name: &str, default: i64) -> i64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(default)
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
    fn parses_ordered_posts_and_filters_old_or_disallowed() {
        let adapter = test_adapter();
        let posts = parse_posts(&json!({
            "order": ["p2", "p1"],
            "posts": {
                "p1": {
                    "id": "p1",
                    "channel_id": "channel-1",
                    "user_id": "user-1",
                    "message": "old",
                    "create_at": 1
                },
                "p2": {
                    "id": "p2",
                    "channel_id": "channel-1",
                    "user_id": "user-1",
                    "message": "new",
                    "create_at": 2
                }
            }
        }));
        assert_eq!(posts.len(), 2);
        assert_eq!(adapter.normalize_post(&posts[0]).unwrap().text, "new");
        assert_eq!(
            adapter.normalize_post(&posts[0]).unwrap().route.platform,
            "mattermost"
        );
    }

    #[test]
    fn allowlist_blocks_unknown_user() {
        let mut adapter = test_adapter();
        adapter.allowed_users = HashSet::from(["allowed".to_string()]);
        assert!(
            adapter
                .normalize_post(&json!({
                    "id": "p1",
                    "channel_id": "channel-1",
                    "user_id": "blocked",
                    "message": "nope",
                    "create_at": 1
                }))
                .is_none()
        );
    }

    #[test]
    fn classifies_mattermost_error_codes() {
        assert_eq!(
            classify_mattermost_response(&json!({
                "status_code": 401,
                "message": "unauthorized"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::SessionExpired
        );
        assert_eq!(
            classify_mattermost_response(&json!({
                "status_code": 429,
                "message": "slow down"
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::RateLimited
        );
        assert_eq!(
            classify_mattermost_response(&json!({
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
        assert_eq!(url_encode_path("channel/id"), "channel%2Fid");
    }

    fn test_adapter() -> MattermostAdapter {
        MattermostAdapter {
            base_url: "http://127.0.0.1:8065".to_string(),
            token: "token".to_string(),
            channel_id: "channel-1".to_string(),
            allowed_users: HashSet::new(),
            last_create_at: Mutex::new(0),
        }
    }
}
