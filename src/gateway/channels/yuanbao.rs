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

const YUANBAO_TEXT_LIMIT: usize = 4_000;
const YUANBAO_TIMEOUT_MS: u64 = 15_000;

pub struct YuanbaoAdapter {
    app_id: String,
    app_secret: String,
    bot_id: Option<String>,
    reply_webhook_url: Option<String>,
    dm_policy: AccessPolicyMode,
    group_policy: AccessPolicyMode,
    dm_allow_from: HashSet<String>,
    group_allow_from: HashSet<String>,
    seen_messages: Mutex<HashSet<String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccessPolicyMode {
    Open,
    Allowlist,
    Closed,
}

impl YuanbaoAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let app_id = env_first(&["YUANBAO_APP_ID", "YUANBAO_APP_KEY"]);
        if app_id.is_empty() {
            return Err(ChannelError::missing_config(
                "YUANBAO_APP_ID is required for the yuanbao channel",
            ));
        }
        let app_secret = env_first(&["YUANBAO_APP_SECRET"]);
        if app_secret.is_empty() {
            return Err(ChannelError::missing_config(
                "YUANBAO_APP_SECRET is required for the yuanbao channel",
            ));
        }
        Ok(Self {
            app_id,
            app_secret,
            bot_id: env_optional(&["YUANBAO_BOT_ID"]),
            reply_webhook_url: env_optional(&["YUANBAO_REPLY_WEBHOOK_URL"]),
            dm_policy: access_policy_from_env("YUANBAO_DM_POLICY"),
            group_policy: access_policy_from_env("YUANBAO_GROUP_POLICY"),
            dm_allow_from: env_set_any(&["YUANBAO_DM_ALLOW_FROM"]),
            group_allow_from: env_set_any(&["YUANBAO_GROUP_ALLOW_FROM"]),
            seen_messages: Mutex::new(HashSet::new()),
        })
    }

    fn normalize_push(&self, raw: &Value) -> Option<NormalizedInboundMessage> {
        let push = raw
            .get("push")
            .or_else(|| raw.get("payload"))
            .or_else(|| raw.get("body"))
            .unwrap_or(raw);
        let callback_command =
            value_string_any(push, &["callback_command", "CallbackCommand"]).unwrap_or_default();
        if callback_command.contains("Recall") || callback_command.contains("WithDraw") {
            return None;
        }
        let from_account = value_string_any(push, &["from_account", "From_Account"])?;
        let group_code = value_string_any(push, &["group_code", "GroupId", "group_id"]);
        if let Some(group_code) = group_code.as_ref() {
            if !self.group_allowed(group_code) {
                return None;
            }
        } else if !self.dm_allowed(&from_account) {
            return None;
        }
        let msg_body = push
            .get("msg_body")
            .or_else(|| push.get("MsgBody"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let text = extract_text(&msg_body).trim().to_string();
        if text.is_empty() {
            return None;
        }
        let source_message_id =
            value_string_any(push, &["msg_id", "msg_key", "MsgKey", "messageId", "id"])
                .unwrap_or_else(|| format!("yuanbao-{}", now_millis()));
        if self.mark_seen(&source_message_id) {
            return None;
        }
        let chat_id = group_code
            .as_ref()
            .map(|group_code| format!("group:{group_code}"))
            .unwrap_or_else(|| format!("direct:{from_account}"));
        let route = MessageRoute {
            platform: "yuanbao".to_string(),
            chat_id: chat_id.clone(),
            chat_type: if group_code.is_some() {
                ChatType::Group
            } else {
                ChatType::Direct
            },
            user_id: from_account.clone(),
            display_name: value_string_any(push, &["sender_nickname", "nick_name", "Nick"])
                .unwrap_or_else(|| from_account.clone()),
            thread_id: chat_id.clone(),
        };
        let metadata = json!({
            "channel": {
                "platform": "yuanbao",
                "adapter": "yuanbao-json-push-bridge",
                "appId": self.app_id,
                "appSecretConfigured": !self.app_secret.is_empty(),
                "botId": self.bot_id,
                "chatId": route.chat_id,
                "chatType": route.chat_type.as_gateway_str(),
                "fromAccount": from_account,
                "groupCode": group_code,
                "msgSeq": push.get("msg_seq").or_else(|| push.get("MsgSeq")).cloned(),
                "sourceMessageId": source_message_id,
                "traceId": push.get("log_ext").and_then(|log| value_string_any(log, &["trace_id"]))
            }
        });
        Some(NormalizedInboundMessage {
            id: format!("yuanbao-{source_message_id}"),
            text,
            route,
            context: None,
            metadata,
        })
    }

    fn dm_allowed(&self, account: &str) -> bool {
        match self.dm_policy {
            AccessPolicyMode::Open => true,
            AccessPolicyMode::Closed => false,
            AccessPolicyMode::Allowlist => self.dm_allow_from.contains(account),
        }
    }

    fn group_allowed(&self, group_code: &str) -> bool {
        match self.group_policy {
            AccessPolicyMode::Open => true,
            AccessPolicyMode::Closed => false,
            AccessPolicyMode::Allowlist => self.group_allow_from.contains(group_code),
        }
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

impl PlatformAdapter for YuanbaoAdapter {
    fn name(&self) -> &'static str {
        "yuanbao"
    }

    fn capabilities(&self) -> ChannelCapabilityReport {
        ChannelCapabilityReport {
            send: if self.reply_webhook_url.is_some() {
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
        let raw = env_first(&["YUANBAO_INBOUND_EVENT", "YUANBAO_INBOUND_PUSH"]);
        if raw.is_empty() {
            return Ok(Vec::new());
        }
        let value = serde_json::from_str::<Value>(&raw).map_err(|error| {
            ChannelError::fatal(format!("yuanbao inbound JSON is invalid: {error}"))
        })?;
        Ok(self.normalize_push(&value).into_iter().collect())
    }

    fn send_typing(&self, _route: &MessageRoute) -> ChannelResult<()> {
        Err(ChannelError::unavailable(
            "yuanbao typing heartbeat requires the full protobuf websocket runtime",
        ))
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        let Some(url) = self.reply_webhook_url.as_ref() else {
            return Err(ChannelError::unavailable(
                "YUANBAO_REPLY_WEBHOOK_URL is required for flyflor-cli Yuanbao reply delivery",
            ));
        };
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal(
                "yuanbao message text must not be empty",
            ));
        }
        if outbound_mentions_media(&message.text, message.metadata.as_ref()) {
            return self.send_media_unavailable("outbound");
        }
        let mut last_id = None;
        for chunk in split_text_chunks(&message.text, YUANBAO_TEXT_LIMIT) {
            let response = post_json(
                url,
                json!({
                    "text": chunk,
                    "route": {
                        "chatId": message.route.chat_id,
                        "chatType": message.route.chat_type.as_gateway_str(),
                        "threadId": message.route.thread_id,
                        "userId": message.route.user_id
                    },
                    "metadata": message.metadata
                }),
            )?;
            classify_yuanbao_response(&response)?;
            last_id = value_string_any(&response, &["msg_id", "messageId", "id"])
                .or_else(|| Some(format!("yuanbao-{}", now_millis())));
        }
        Ok(PlatformSendOutcome {
            message_id: last_id,
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "yuanbao {media_kind} media delivery requires the full COS/protobuf runtime"
        )))
    }
}

fn extract_text(items: &[Value]) -> String {
    items
        .iter()
        .filter_map(|item| {
            let msg_type = value_string_any(item, &["msg_type", "MsgType"])?;
            let content = item
                .get("msg_content")
                .or_else(|| item.get("MsgContent"))
                .unwrap_or(item);
            match msg_type.as_str() {
                "TIMTextElem" => value_raw_string_any(content, &["text", "Text"]),
                "TIMCustomElem" => value_string_any(content, &["desc", "Desc", "data", "Data"])
                    .map(|text| format!("[custom] {text}")),
                "TIMImageElem" => Some("[image]".to_string()),
                "TIMFileElem" => value_string_any(content, &["file_name", "FileName"])
                    .map(|name| format!("[file: {name}]"))
                    .or_else(|| Some("[file]".to_string())),
                _ => None,
            }
        })
        .collect::<Vec<_>>()
        .join("")
}

fn post_json(url: &str, payload: Value) -> ChannelResult<Value> {
    let body =
        serde_json::to_string(&payload).map_err(|error| ChannelError::fatal(error.to_string()))?;
    let output = Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            &seconds_arg(YUANBAO_TIMEOUT_MS),
            "-X",
            "POST",
            url,
            "-H",
            "Content-Type: application/json",
            "--data",
            &body,
        ])
        .output()
        .map_err(|error| ChannelError::unavailable(format!("curl is required: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ChannelError::retryable(format!(
            "yuanbao curl failed with status {}: {}",
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

fn classify_yuanbao_response(value: &Value) -> ChannelResult<()> {
    let code = value
        .get("code")
        .or_else(|| value.get("errcode"))
        .and_then(|value| value.as_i64().or_else(|| value.as_str()?.parse().ok()))
        .unwrap_or(0);
    if code == 0 && value.get("error").is_none() {
        return Ok(());
    }
    let message = value
        .get("message")
        .or_else(|| value.get("errmsg"))
        .or_else(|| value.get("error"))
        .and_then(Value::as_str)
        .unwrap_or("unknown yuanbao error");
    match code {
        4001..=4003 | 4012..=4014 | 4018 | 4019 | 4021 => Err(ChannelError::session_expired(
            format!("Yuanbao authorization failed: code={code} message={message}"),
        )),
        4010 | 4011 | 4099 | 429 => Err(ChannelError::retryable(format!(
            "Yuanbao retryable error: code={code} message={message}"
        ))),
        _ => Err(ChannelError::fatal(format!(
            "Yuanbao error: code={code} message={message}"
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

fn value_raw_string_any(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    })
}

fn access_policy_from_env(name: &str) -> AccessPolicyMode {
    match env::var(name)
        .unwrap_or_else(|_| "open".to_string())
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "allowlist" | "allow" => AccessPolicyMode::Allowlist,
        "closed" | "deny" | "off" => AccessPolicyMode::Closed,
        _ => AccessPolicyMode::Open,
    }
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

fn seconds_arg(milliseconds: u64) -> String {
    ((milliseconds + 999) / 1000).to_string()
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
    fn normalizes_group_json_push() {
        let adapter = test_adapter();
        let message = adapter
            .normalize_push(&json!({
                "callback_command": "Group.CallbackAfterSendMsg",
                "from_account": "user-1",
                "sender_nickname": "Ada",
                "group_code": "group-1",
                "group_name": "Ops",
                "msg_id": "msg-1",
                "msg_body": [
                    { "msg_type": "TIMTextElem", "msg_content": { "text": "hello " } },
                    { "MsgType": "TIMTextElem", "MsgContent": { "text": "yuanbao" } }
                ],
                "log_ext": { "trace_id": "trace-1" }
            }))
            .unwrap();

        assert_eq!(message.id, "yuanbao-msg-1");
        assert_eq!(message.text, "hello yuanbao");
        assert_eq!(message.route.chat_id, "group:group-1");
        assert_eq!(message.route.chat_type, ChatType::Group);
        assert_eq!(message.metadata["channel"]["traceId"], "trace-1");
    }

    #[test]
    fn normalizes_direct_pascal_case_push() {
        let adapter = test_adapter();
        let message = adapter
            .normalize_push(&json!({
                "From_Account": "user-1",
                "MsgKey": "msg-2",
                "MsgBody": [
                    { "MsgType": "TIMTextElem", "MsgContent": { "text": "direct" } }
                ]
            }))
            .unwrap();

        assert_eq!(message.route.chat_id, "direct:user-1");
        assert_eq!(message.route.chat_type, ChatType::Direct);
        assert_eq!(message.text, "direct");
    }

    #[test]
    fn policy_and_dedup_filters_work() {
        let mut adapter = test_adapter();
        adapter.group_policy = AccessPolicyMode::Allowlist;
        adapter.group_allow_from = HashSet::from(["allowed-group".to_string()]);
        assert!(
            adapter
                .normalize_push(&json!({
                    "from_account": "user-1",
                    "group_code": "blocked-group",
                    "msg_id": "blocked",
                    "msg_body": [{ "msg_type": "TIMTextElem", "msg_content": { "text": "blocked" } }]
                }))
                .is_none()
        );
        let event = json!({
            "from_account": "user-1",
            "group_code": "allowed-group",
            "msg_id": "dup",
            "msg_body": [{ "msg_type": "TIMTextElem", "msg_content": { "text": "ok" } }]
        });
        assert!(adapter.normalize_push(&event).is_some());
        assert!(adapter.normalize_push(&event).is_none());
    }

    #[test]
    fn extracts_non_text_markers_and_chunks_unicode() {
        assert_eq!(
            extract_text(&[
                json!({ "msg_type": "TIMImageElem", "msg_content": {} }),
                json!({ "msg_type": "TIMFileElem", "msg_content": { "file_name": "a.pdf" } }),
            ]),
            "[image][file: a.pdf]"
        );
        assert_eq!(split_text_chunks("你好世界", 2), vec!["你好", "世界"]);
    }

    #[test]
    fn classifies_yuanbao_errors() {
        assert!(classify_yuanbao_response(&json!({ "code": 0 })).is_ok());
        assert_eq!(
            classify_yuanbao_response(&json!({ "code": 4001, "message": "auth" }))
                .unwrap_err()
                .kind,
            ChannelErrorKind::SessionExpired
        );
        assert_eq!(
            classify_yuanbao_response(&json!({ "code": 4010, "message": "retry" }))
                .unwrap_err()
                .kind,
            ChannelErrorKind::Retryable
        );
    }

    fn test_adapter() -> YuanbaoAdapter {
        YuanbaoAdapter {
            app_id: "app-1".to_string(),
            app_secret: "secret".to_string(),
            bot_id: Some("bot-1".to_string()),
            reply_webhook_url: None,
            dm_policy: AccessPolicyMode::Open,
            group_policy: AccessPolicyMode::Open,
            dm_allow_from: HashSet::new(),
            group_allow_from: HashSet::new(),
            seen_messages: Mutex::new(HashSet::new()),
        }
    }
}
