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

const DEFAULT_GOOGLE_CHAT_API_BASE: &str = "https://chat.googleapis.com";
const GOOGLE_CHAT_TEXT_LIMIT: usize = 4_000;
const GOOGLE_CHAT_TIMEOUT_MS: u64 = 15_000;

pub struct GoogleChatAdapter {
    project_id: String,
    subscription_name: String,
    service_account_json: String,
    api_base: String,
    access_token: Option<String>,
    allowed_users: HashSet<String>,
}

impl GoogleChatAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let project_id = env_first(&["GOOGLE_CHAT_PROJECT_ID", "GOOGLE_CLOUD_PROJECT"]);
        if project_id.is_empty() {
            return Err(ChannelError::missing_config(
                "GOOGLE_CHAT_PROJECT_ID is required for the google-chat channel",
            ));
        }
        let subscription_name =
            env_first(&["GOOGLE_CHAT_SUBSCRIPTION_NAME", "GOOGLE_CHAT_SUBSCRIPTION"]);
        if subscription_name.is_empty() {
            return Err(ChannelError::missing_config(
                "GOOGLE_CHAT_SUBSCRIPTION_NAME is required for the google-chat channel",
            ));
        }
        let service_account_json = env_first(&[
            "GOOGLE_CHAT_SERVICE_ACCOUNT_JSON",
            "GOOGLE_APPLICATION_CREDENTIALS",
        ]);
        if service_account_json.is_empty() {
            return Err(ChannelError::missing_config(
                "GOOGLE_CHAT_SERVICE_ACCOUNT_JSON is required for the google-chat channel",
            ));
        }
        Ok(Self {
            project_id,
            subscription_name,
            service_account_json,
            api_base: env_first(&["GOOGLE_CHAT_API_BASE", "FLYFLOR_GOOGLE_CHAT_API_BASE"])
                .if_empty(DEFAULT_GOOGLE_CHAT_API_BASE),
            access_token: env_optional(&["GOOGLE_CHAT_ACCESS_TOKEN", "GOOGLE_CHAT_TOKEN"]),
            allowed_users: env_set_any(&["GOOGLE_CHAT_ALLOWED_USERS"]),
        })
    }

    fn normalize_event(&self, value: &Value) -> Option<NormalizedInboundMessage> {
        let envelope = value
            .get("chat")
            .and_then(|chat| chat.get("messagePayload"))
            .or_else(|| value.get("messagePayload"))
            .or_else(|| value.get("payload"))
            .unwrap_or(value);
        let message = envelope
            .get("message")
            .or_else(|| value.get("message"))
            .unwrap_or(envelope);
        let sender = message.get("sender").unwrap_or(message);
        if value_string_any(sender, &["type"])
            .is_some_and(|sender_type| sender_type.eq_ignore_ascii_case("BOT"))
        {
            return None;
        }
        let user_id = value_string_any(sender, &["name", "email", "displayName"])
            .unwrap_or_else(|| "google-chat-user".to_string());
        let sender_email = value_string_any(sender, &["email"]);
        if !self.allowed_users.is_empty()
            && !self.allowed_users.contains(&user_id)
            && !sender_email
                .as_ref()
                .is_some_and(|email| self.allowed_users.contains(email))
        {
            return None;
        }
        let text = value_string_any(message, &["argumentText", "text"])?
            .trim()
            .to_string();
        if text.is_empty() {
            return None;
        }
        let space = message
            .get("space")
            .or_else(|| envelope.get("space"))
            .unwrap_or(message);
        let space_name = value_string_any(space, &["name"])?;
        let space_type = value_string_any(space, &["spaceType"])
            .unwrap_or_else(|| "SPACE".to_string())
            .to_uppercase();
        let thread_name = message
            .get("thread")
            .and_then(|thread| value_string_any(thread, &["name"]))
            .unwrap_or_else(|| space_name.clone());
        let message_name = value_string_any(message, &["name", "messageId", "id"])
            .unwrap_or_else(|| format!("google-chat-{}", now_millis()));
        let route = MessageRoute {
            platform: "google-chat".to_string(),
            chat_id: thread_name.clone(),
            chat_type: if space_type == "DIRECT_MESSAGE" {
                ChatType::Direct
            } else {
                ChatType::Group
            },
            user_id: user_id.clone(),
            display_name: value_string_any(sender, &["displayName"])
                .or(sender_email.clone())
                .unwrap_or_else(|| user_id.clone()),
            thread_id: thread_name.clone(),
        };
        let metadata = json!({
            "channel": {
                "platform": "google-chat",
                "adapter": "google-chat-rest-pubsub",
                "projectId": self.project_id,
                "subscriptionName": self.subscription_name,
                "serviceAccountConfigured": !self.service_account_json.is_empty(),
                "chatId": route.chat_id,
                "chatType": route.chat_type.as_gateway_str(),
                "spaceName": space_name,
                "threadName": thread_name,
                "sourceMessageId": message_name,
                "userId": user_id
            }
        });
        Some(NormalizedInboundMessage {
            id: format!("google-chat-{}", stable_id(&message_name)),
            text,
            route,
            context: None,
            metadata,
        })
    }
}

impl PlatformAdapter for GoogleChatAdapter {
    fn name(&self) -> &'static str {
        "google-chat"
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
        let raw = env_first(&["GOOGLE_CHAT_INBOUND_EVENT", "GOOGLE_CHAT_INBOUND_MESSAGE"]);
        if raw.is_empty() {
            return Ok(Vec::new());
        }
        let value = serde_json::from_str::<Value>(&raw).map_err(|error| {
            ChannelError::fatal(format!("google-chat inbound JSON is invalid: {error}"))
        })?;
        Ok(self.normalize_event(&value).into_iter().collect())
    }

    fn send_typing(&self, _route: &MessageRoute) -> ChannelResult<()> {
        Err(ChannelError::unavailable(
            "google-chat typing is unavailable in the current REST adapter",
        ))
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal(
                "google-chat message text must not be empty",
            ));
        }
        if outbound_mentions_media(&message.text, message.metadata.as_ref()) {
            return self.send_media_unavailable("outbound");
        }
        let token = self.access_token.clone().ok_or_else(|| {
            ChannelError::missing_config(
                "GOOGLE_CHAT_ACCESS_TOKEN is required for flyflor-cli Google Chat REST send",
            )
        })?;
        let channel = message
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("channel"));
        let space_name = channel
            .and_then(|channel| value_string_any(channel, &["spaceName"]))
            .or_else(|| space_from_thread(&message.route.thread_id))
            .unwrap_or_else(|| message.route.chat_id.clone());
        let thread_name = channel
            .and_then(|channel| value_string_any(channel, &["threadName"]))
            .unwrap_or_else(|| message.route.thread_id.clone());
        let mut last_id = None;
        for chunk in split_text_chunks(&message.text, GOOGLE_CHAT_TEXT_LIMIT) {
            let url = format!("{}/v1/{}/messages", self.api_base, space_name);
            let response = google_chat_post(
                &url,
                &token,
                json!({
                    "text": chunk,
                    "thread": { "name": thread_name }
                }),
            )?;
            classify_google_chat_response(&response)?;
            last_id = value_string_any(&response, &["name", "messageId", "id"])
                .or_else(|| Some(format!("google-chat-{}", now_millis())));
        }
        Ok(PlatformSendOutcome {
            message_id: last_id,
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "google-chat {media_kind} media delivery is explicitly unavailable in flyflor-cli"
        )))
    }
}

fn google_chat_post(url: &str, token: &str, payload: Value) -> ChannelResult<Value> {
    let body =
        serde_json::to_string(&payload).map_err(|error| ChannelError::fatal(error.to_string()))?;
    let auth = format!("Authorization: Bearer {token}");
    curl_json([
        "-sS",
        "--max-time",
        &seconds_arg(GOOGLE_CHAT_TIMEOUT_MS),
        "-X",
        "POST",
        url,
        "-H",
        "Content-Type: application/json",
        "-H",
        &auth,
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
        return Err(ChannelError::retryable(format!(
            "google-chat curl failed with status {}: {}",
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
            "google-chat returned invalid JSON: {error}; body={text}"
        ))
    })
}

fn classify_google_chat_response(value: &Value) -> ChannelResult<()> {
    let Some(error) = value.get("error") else {
        return Ok(());
    };
    let code = error.get("code").and_then(Value::as_i64).unwrap_or(0);
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("unknown google-chat error");
    match code {
        401 | 403 => Err(ChannelError::session_expired(format!(
            "Google Chat authorization failed: code={code} message={message}"
        ))),
        429 => Err(ChannelError::rate_limited(format!(
            "Google Chat rate limited: code={code} message={message}"
        ))),
        400 | 404 => Err(ChannelError::fatal(format!(
            "Google Chat bad request: code={code} message={message}"
        ))),
        _ if code >= 500 => Err(ChannelError::retryable(format!(
            "Google Chat transient error: code={code} message={message}"
        ))),
        _ => Err(ChannelError::retryable(format!(
            "Google Chat error: code={code} message={message}"
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

fn space_from_thread(thread_name: &str) -> Option<String> {
    thread_name
        .split_once("/threads/")
        .map(|(space, _)| space.to_string())
        .filter(|space| !space.is_empty())
}

fn stable_id(value: &str) -> String {
    value.replace('/', "-").replace('.', "-")
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
    fn normalizes_google_chat_message_payload() {
        let adapter = test_adapter();
        let message = adapter
            .normalize_event(&json!({
                "chat": {
                    "messagePayload": {
                        "message": {
                            "name": "spaces/S/messages/M",
                            "sender": {
                                "name": "users/1",
                                "email": "user@example.com",
                                "displayName": "Google User",
                                "type": "HUMAN"
                            },
                            "text": "hello google",
                            "argumentText": "hello google",
                            "thread": { "name": "spaces/S/threads/T" },
                            "space": { "name": "spaces/S", "spaceType": "ROOM" }
                        }
                    }
                }
            }))
            .unwrap();

        assert_eq!(message.id, "google-chat-spaces-S-messages-M");
        assert_eq!(message.text, "hello google");
        assert_eq!(message.route.platform, "google-chat");
        assert_eq!(message.route.chat_id, "spaces/S/threads/T");
        assert_eq!(message.route.chat_type, ChatType::Group);
        assert_eq!(message.metadata["channel"]["spaceName"], "spaces/S");
        assert_eq!(
            message.metadata["channel"]["sourceMessageId"],
            "spaces/S/messages/M"
        );
    }

    #[test]
    fn filters_bots_and_unknown_allowed_users() {
        let mut adapter = test_adapter();
        adapter.allowed_users = HashSet::from(["allowed@example.com".to_string()]);

        assert!(
            adapter
                .normalize_event(&json!({
                    "message": {
                        "sender": { "email": "blocked@example.com", "type": "HUMAN" },
                        "text": "blocked",
                        "space": { "name": "spaces/S", "spaceType": "DIRECT_MESSAGE" }
                    }
                }))
                .is_none()
        );
        assert!(
            adapter
                .normalize_event(&json!({
                    "message": {
                        "sender": { "email": "allowed@example.com", "type": "BOT" },
                        "text": "blocked",
                        "space": { "name": "spaces/S", "spaceType": "DIRECT_MESSAGE" }
                    }
                }))
                .is_none()
        );
    }

    #[test]
    fn classifies_google_chat_error_codes() {
        assert_eq!(
            classify_google_chat_response(
                &json!({ "error": { "code": 403, "message": "denied" } })
            )
            .unwrap_err()
            .kind,
            ChannelErrorKind::SessionExpired
        );
        assert_eq!(
            classify_google_chat_response(&json!({ "error": { "code": 429, "message": "busy" } }))
                .unwrap_err()
                .kind,
            ChannelErrorKind::RateLimited
        );
        assert_eq!(
            classify_google_chat_response(&json!({ "error": { "code": 400, "message": "bad" } }))
                .unwrap_err()
                .kind,
            ChannelErrorKind::Fatal
        );
    }

    #[test]
    fn helper_chunks_and_space_derivation_preserve_unicode() {
        assert_eq!(split_text_chunks("你好世界", 2), vec!["你好", "世界"]);
        assert_eq!(
            space_from_thread("spaces/S/threads/T"),
            Some("spaces/S".to_string())
        );
    }

    fn test_adapter() -> GoogleChatAdapter {
        GoogleChatAdapter {
            project_id: "project-1".to_string(),
            subscription_name: "projects/project-1/subscriptions/sub-1".to_string(),
            service_account_json: "{}".to_string(),
            api_base: DEFAULT_GOOGLE_CHAT_API_BASE.to_string(),
            access_token: Some("token".to_string()),
            allowed_users: HashSet::new(),
        }
    }
}
