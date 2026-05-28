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

const DEFAULT_GRAPH_API_BASE: &str = "https://graph.microsoft.com";
const TEAMS_TEXT_LIMIT: usize = 28_000;
const TEAMS_TIMEOUT_MS: u64 = 15_000;

pub struct TeamsAdapter {
    client_id: String,
    client_secret: String,
    tenant_id: String,
    incoming_webhook_url: Option<String>,
    graph_access_token: Option<String>,
    graph_api_base: String,
    allowed_users: HashSet<String>,
    allow_all_users: bool,
    seen_messages: Mutex<HashSet<String>>,
}

impl TeamsAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let client_id = env_first(&["TEAMS_CLIENT_ID", "FLYFLOR_TEAMS_CLIENT_ID"]);
        if client_id.is_empty() {
            return Err(ChannelError::missing_config(
                "TEAMS_CLIENT_ID is required for the teams channel",
            ));
        }
        let client_secret = env_first(&["TEAMS_CLIENT_SECRET", "FLYFLOR_TEAMS_CLIENT_SECRET"]);
        if client_secret.is_empty() {
            return Err(ChannelError::missing_config(
                "TEAMS_CLIENT_SECRET is required for the teams channel",
            ));
        }
        let tenant_id = env_first(&["TEAMS_TENANT_ID", "FLYFLOR_TEAMS_TENANT_ID"]);
        if tenant_id.is_empty() {
            return Err(ChannelError::missing_config(
                "TEAMS_TENANT_ID is required for the teams channel",
            ));
        }
        Ok(Self {
            client_id,
            client_secret,
            tenant_id,
            incoming_webhook_url: env_optional(&[
                "TEAMS_INCOMING_WEBHOOK_URL",
                "TEAMS_WEBHOOK_URL",
            ]),
            graph_access_token: env_optional(&["TEAMS_GRAPH_ACCESS_TOKEN", "TEAMS_ACCESS_TOKEN"]),
            graph_api_base: env_first(&["TEAMS_GRAPH_API_BASE"]).if_empty(DEFAULT_GRAPH_API_BASE),
            allowed_users: env_set_any(&["TEAMS_ALLOWED_USERS", "TEAMS_ALLOW_FROM"]),
            allow_all_users: env_bool_any(&["TEAMS_ALLOW_ALL_USERS"]),
            seen_messages: Mutex::new(HashSet::new()),
        })
    }

    fn normalize_activity(&self, value: &Value) -> Option<NormalizedInboundMessage> {
        let activity = value
            .get("activity")
            .or_else(|| value.get("payload"))
            .or_else(|| value.get("body"))
            .unwrap_or(value);
        if value_string_any(activity, &["type"])
            .unwrap_or_else(|| "message".to_string())
            .to_ascii_lowercase()
            != "message"
        {
            return None;
        }
        let sender = activity.get("from").unwrap_or(activity);
        let sender_id = value_string_any(sender, &["aadObjectId", "aad_object_id", "id"])?;
        let transport_user_id =
            value_string_any(sender, &["id"]).unwrap_or_else(|| sender_id.clone());
        if transport_user_id == self.client_id || sender_id == self.client_id {
            return None;
        }
        if !self.allow_all_users
            && !self.allowed_users.is_empty()
            && !self.allowed_users.contains(&sender_id)
            && !self.allowed_users.contains(&transport_user_id)
        {
            return None;
        }
        let text = strip_bot_mentions(&value_string_any(activity, &["text", "summary"])?);
        if text.is_empty() {
            return None;
        }
        let conversation = activity.get("conversation").unwrap_or(activity);
        let chat_id = value_string_any(conversation, &["id", "conversationId", "chatId"])?;
        let conversation_type = value_string_any(conversation, &["conversationType", "type"])
            .unwrap_or_else(|| "personal".to_string())
            .to_ascii_lowercase();
        let chat_type = if conversation_type == "personal" {
            ChatType::Direct
        } else {
            ChatType::Group
        };
        let source_message_id = value_string_any(activity, &["id", "activityId", "messageId"])
            .unwrap_or_else(|| format!("teams-{}", now_millis()));
        if self.mark_seen(&source_message_id) {
            return None;
        }
        let channel_data = activity.get("channelData").unwrap_or(activity);
        let tenant_id = channel_data
            .get("tenant")
            .and_then(|tenant| value_string_any(tenant, &["id", "tenantId"]))
            .or_else(|| value_string_any(conversation, &["tenantId"]))
            .unwrap_or_else(|| self.tenant_id.clone());
        let team_id = channel_data
            .get("team")
            .and_then(|team| value_string_any(team, &["id", "teamId"]));
        let channel_id = channel_data
            .get("channel")
            .and_then(|channel| value_string_any(channel, &["id", "channelId"]));
        let route = MessageRoute {
            platform: "teams".to_string(),
            chat_id: chat_id.clone(),
            chat_type,
            user_id: sender_id.clone(),
            display_name: value_string_any(sender, &["name", "displayName"])
                .unwrap_or_else(|| sender_id.clone()),
            thread_id: chat_id.clone(),
        };
        let metadata = json!({
            "channel": {
                "platform": "teams",
                "adapter": "teams-botframework-env",
                "chatId": route.chat_id,
                "chatType": route.chat_type.as_gateway_str(),
                "sourceMessageId": source_message_id,
                "tenantId": tenant_id,
                "teamId": team_id,
                "channelId": channel_id,
                "serviceUrl": value_string_any(activity, &["serviceUrl"]),
                "userId": sender_id,
                "clientId": self.client_id,
                "clientSecretConfigured": !self.client_secret.is_empty()
            }
        });
        Some(NormalizedInboundMessage {
            id: format!("teams-{source_message_id}"),
            text,
            route,
            context: None,
            metadata,
        })
    }

    fn mark_seen(&self, message_id: &str) -> bool {
        let Ok(mut seen) = self.seen_messages.lock() else {
            return false;
        };
        if seen.contains(message_id) {
            true
        } else {
            seen.insert(message_id.to_string());
            false
        }
    }
}

impl PlatformAdapter for TeamsAdapter {
    fn name(&self) -> &'static str {
        "teams"
    }

    fn capabilities(&self) -> ChannelCapabilityReport {
        ChannelCapabilityReport {
            send: if self.incoming_webhook_url.is_some() || self.graph_access_token.is_some() {
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
        let raw = env_first(&["TEAMS_INBOUND_EVENT", "TEAMS_INBOUND_ACTIVITY"]);
        if raw.is_empty() {
            return Ok(Vec::new());
        }
        let value = serde_json::from_str::<Value>(&raw).map_err(|error| {
            ChannelError::fatal(format!("teams inbound JSON is invalid: {error}"))
        })?;
        Ok(self.normalize_activity(&value).into_iter().collect())
    }

    fn send_typing(&self, _route: &MessageRoute) -> ChannelResult<()> {
        Err(ChannelError::unavailable(
            "teams typing is unavailable in the current flyflor-cli adapter",
        ))
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal("teams message text must not be empty"));
        }
        if outbound_mentions_media(&message.text, message.metadata.as_ref()) {
            return self.send_media_unavailable("outbound");
        }
        if let Some(url) = &self.incoming_webhook_url {
            let mut last_id = None;
            for chunk in split_text_chunks(&message.text, TEAMS_TEXT_LIMIT) {
                let response = post_json(url, None, json!({ "text": chunk }))?;
                classify_teams_response(&response)?;
                last_id = value_string_any(&response, &["id", "messageId"])
                    .or_else(|| Some(format!("teams-{}", now_millis())));
            }
            return Ok(PlatformSendOutcome {
                message_id: last_id,
            });
        }
        let Some(token) = self.graph_access_token.as_ref() else {
            return Err(ChannelError::unavailable(
                "TEAMS_INCOMING_WEBHOOK_URL or TEAMS_GRAPH_ACCESS_TOKEN is required for flyflor-cli Teams reply delivery",
            ));
        };
        let channel = message
            .metadata
            .as_ref()
            .and_then(|value| value.get("channel"));
        let team_id = channel.and_then(|channel| value_string_any(channel, &["teamId"]));
        let channel_id = channel.and_then(|channel| value_string_any(channel, &["channelId"]));
        let path = if let (Some(team_id), Some(channel_id)) = (team_id, channel_id) {
            format!(
                "/v1.0/teams/{}/channels/{}/messages",
                url_path(&team_id),
                url_path(&channel_id)
            )
        } else {
            format!("/v1.0/chats/{}/messages", url_path(&message.route.chat_id))
        };
        let url = format!("{}{}", self.graph_api_base, path);
        let mut last_id = None;
        for chunk in split_text_chunks(&message.text, TEAMS_TEXT_LIMIT) {
            let response = post_json(
                &url,
                Some(token),
                json!({
                    "body": {
                        "contentType": "html",
                        "content": html_escape(&chunk)
                    }
                }),
            )?;
            classify_teams_response(&response)?;
            last_id = value_string_any(&response, &["id", "messageId"])
                .or_else(|| Some(format!("teams-{}", now_millis())));
        }
        Ok(PlatformSendOutcome {
            message_id: last_id,
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "teams {media_kind} media delivery is explicitly unavailable in flyflor-cli"
        )))
    }
}

fn post_json(url: &str, bearer_token: Option<&str>, payload: Value) -> ChannelResult<Value> {
    let body =
        serde_json::to_string(&payload).map_err(|error| ChannelError::fatal(error.to_string()))?;
    let mut args = vec![
        "-sS".to_string(),
        "--max-time".to_string(),
        seconds_arg(TEAMS_TIMEOUT_MS),
        "-X".to_string(),
        "POST".to_string(),
        url.to_string(),
        "-H".to_string(),
        "Content-Type: application/json".to_string(),
    ];
    if let Some(token) = bearer_token {
        args.push("-H".to_string());
        args.push(format!("Authorization: Bearer {token}"));
    }
    args.push("--data".to_string());
    args.push(body);
    curl_json(args)
}

fn curl_json(args: Vec<String>) -> ChannelResult<Value> {
    let output = Command::new("curl")
        .args(&args)
        .output()
        .map_err(|error| ChannelError::unavailable(format!("curl is required: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ChannelError::retryable(format!(
            "teams curl failed with status {}: {}",
            output.status,
            stderr.trim()
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        return Ok(json!({ "ok": true }));
    }
    serde_json::from_str::<Value>(&stdout)
        .or_else(|_| Ok(json!({ "ok": true, "body": stdout.trim() })))
}

fn classify_teams_response(value: &Value) -> ChannelResult<()> {
    if value.get("error").is_none() {
        return Ok(());
    }
    let error = value.get("error").unwrap_or(value);
    let code = value_string_any(error, &["code"]).unwrap_or_default();
    let message =
        value_string_any(error, &["message"]).unwrap_or_else(|| "unknown teams error".to_string());
    match code.as_str() {
        "InvalidAuthenticationToken" | "Unauthorized" | "Forbidden" => {
            Err(ChannelError::session_expired(format!(
                "Teams authorization failed: code={code} message={message}"
            )))
        }
        "TooManyRequests" | "Throttled" => Err(ChannelError::rate_limited(format!(
            "Teams rate limited: code={code} message={message}"
        ))),
        "BadRequest" | "InvalidRequest" => Err(ChannelError::fatal(format!(
            "Teams bad request: code={code} message={message}"
        ))),
        _ => Err(ChannelError::retryable(format!(
            "Teams error: code={code} message={message}"
        ))),
    }
}

fn strip_bot_mentions(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut index = 0usize;
    while let Some(start) = text[index..].find("<at>") {
        let absolute_start = index + start;
        output.push_str(&text[index..absolute_start]);
        if let Some(end) = text[absolute_start..].find("</at>") {
            index = absolute_start + end + "</at>".len();
        } else {
            index = absolute_start;
            break;
        }
    }
    output.push_str(&text[index..]);
    output
        .replace("&nbsp;", " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
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

fn env_bool_any(names: &[&str]) -> bool {
    names.iter().any(|name| {
        env::var(name).is_ok_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "y" | "on"
            )
        })
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

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn url_path(value: &str) -> String {
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

fn outbound_mentions_media(text: &str, metadata: Option<&Value>) -> bool {
    if text.contains("MEDIA:") {
        return true;
    }
    metadata
        .and_then(|value| value.get("media"))
        .is_some_and(|media| !media.is_null())
}

fn seconds_arg(milliseconds: u64) -> String {
    ((milliseconds + 999) / 1000).to_string()
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
    fn normalizes_personal_activity() {
        let adapter = test_adapter();
        let message = adapter
            .normalize_activity(&json!({
                "type": "message",
                "id": "activity-1",
                "text": "<at>Flyflor</at> hello teams",
                "from": {
                    "id": "teams-user",
                    "aadObjectId": "aad-user",
                    "name": "Ada"
                },
                "conversation": {
                    "id": "19:personal@thread.v2",
                    "conversationType": "personal",
                    "tenantId": "tenant-1"
                },
                "serviceUrl": "https://smba.trafficmanager.net/teams/"
            }))
            .unwrap();

        assert_eq!(message.id, "teams-activity-1");
        assert_eq!(message.text, "hello teams");
        assert_eq!(message.route.platform, "teams");
        assert_eq!(message.route.chat_type, ChatType::Direct);
        assert_eq!(message.route.user_id, "aad-user");
        assert_eq!(message.metadata["channel"]["sourceMessageId"], "activity-1");
    }

    #[test]
    fn normalizes_channel_activity_with_team_metadata() {
        let adapter = test_adapter();
        let message = adapter
            .normalize_activity(&json!({
                "type": "message",
                "id": "activity-2",
                "text": "hello channel",
                "from": { "id": "teams-user", "aadObjectId": "aad-user", "name": "Ada" },
                "conversation": {
                    "id": "19:channel@thread.tacv2",
                    "conversationType": "channel"
                },
                "channelData": {
                    "tenant": { "id": "tenant-2" },
                    "team": { "id": "team-1" },
                    "channel": { "id": "channel-1" }
                }
            }))
            .unwrap();

        assert_eq!(message.route.chat_type, ChatType::Group);
        assert_eq!(message.metadata["channel"]["tenantId"], "tenant-2");
        assert_eq!(message.metadata["channel"]["teamId"], "team-1");
        assert_eq!(message.metadata["channel"]["channelId"], "channel-1");
    }

    #[test]
    fn filters_self_allowed_users_and_duplicates() {
        let mut adapter = test_adapter();
        assert!(
            adapter
                .normalize_activity(&json!({
                    "type": "message",
                    "id": "self",
                    "text": "self",
                    "from": { "id": "client-1" },
                    "conversation": { "id": "chat" }
                }))
                .is_none()
        );
        adapter.allowed_users = HashSet::from(["allowed".to_string()]);
        assert!(
            adapter
                .normalize_activity(&json!({
                    "type": "message",
                    "id": "blocked",
                    "text": "blocked",
                    "from": { "id": "blocked-user" },
                    "conversation": { "id": "chat" }
                }))
                .is_none()
        );
        let event = json!({
            "type": "message",
            "id": "dup",
            "text": "hi",
            "from": { "id": "allowed" },
            "conversation": { "id": "chat" }
        });
        assert!(adapter.normalize_activity(&event).is_some());
        assert!(adapter.normalize_activity(&event).is_none());
    }

    #[test]
    fn helpers_preserve_unicode_and_encode_paths() {
        assert_eq!(split_text_chunks("你好世界", 2), vec!["你好", "世界"]);
        assert_eq!(strip_bot_mentions("<at>Bot</at> hi&nbsp;there"), "hi there");
        assert_eq!(html_escape("<b>&\""), "&lt;b&gt;&amp;&quot;");
        assert_eq!(url_path("19:chat@thread.v2"), "19%3Achat%40thread.v2");
    }

    #[test]
    fn classifies_graph_errors() {
        assert!(classify_teams_response(&json!({ "id": "ok" })).is_ok());
        assert_eq!(
            classify_teams_response(&json!({
                "error": { "code": "InvalidAuthenticationToken", "message": "expired" }
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::SessionExpired
        );
        assert_eq!(
            classify_teams_response(&json!({
                "error": { "code": "TooManyRequests", "message": "slow down" }
            }))
            .unwrap_err()
            .kind,
            ChannelErrorKind::RateLimited
        );
    }

    fn test_adapter() -> TeamsAdapter {
        TeamsAdapter {
            client_id: "client-1".to_string(),
            client_secret: "secret".to_string(),
            tenant_id: "tenant-1".to_string(),
            incoming_webhook_url: None,
            graph_access_token: None,
            graph_api_base: DEFAULT_GRAPH_API_BASE.to_string(),
            allowed_users: HashSet::new(),
            allow_all_users: false,
            seen_messages: Mutex::new(HashSet::new()),
        }
    }
}
