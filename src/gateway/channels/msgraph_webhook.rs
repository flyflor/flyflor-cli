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

const MSGRAPH_TIMEOUT_MS: u64 = 15_000;

pub struct MsGraphWebhookAdapter {
    tenant_id: String,
    client_id: String,
    client_secret: String,
    client_state: Option<String>,
    accepted_resources: Vec<String>,
    reply_webhook_url: Option<String>,
    seen_receipts: Mutex<HashSet<String>>,
}

impl MsGraphWebhookAdapter {
    pub fn from_env() -> ChannelResult<Self> {
        let tenant_id = env_first(&["MSGRAPH_TENANT_ID", "FLYFLOR_MSGRAPH_TENANT_ID"]);
        if tenant_id.is_empty() {
            return Err(ChannelError::missing_config(
                "MSGRAPH_TENANT_ID is required for the msgraph-webhook channel",
            ));
        }
        let client_id = env_first(&["MSGRAPH_CLIENT_ID", "FLYFLOR_MSGRAPH_CLIENT_ID"]);
        if client_id.is_empty() {
            return Err(ChannelError::missing_config(
                "MSGRAPH_CLIENT_ID is required for the msgraph-webhook channel",
            ));
        }
        let client_secret = env_first(&["MSGRAPH_CLIENT_SECRET", "FLYFLOR_MSGRAPH_CLIENT_SECRET"]);
        if client_secret.is_empty() {
            return Err(ChannelError::missing_config(
                "MSGRAPH_CLIENT_SECRET is required for the msgraph-webhook channel",
            ));
        }
        Ok(Self {
            tenant_id,
            client_id,
            client_secret,
            client_state: env_optional(&["MSGRAPH_WEBHOOK_SECRET", "MSGRAPH_CLIENT_STATE"]),
            accepted_resources: env_list_any(&["MSGRAPH_ACCEPTED_RESOURCES"]),
            reply_webhook_url: env_optional(&["MSGRAPH_REPLY_WEBHOOK_URL"]),
            seen_receipts: Mutex::new(HashSet::new()),
        })
    }

    fn normalize_notification(&self, notification: &Value) -> Option<NormalizedInboundMessage> {
        if !self.verify_client_state(notification) {
            return None;
        }
        let resource = value_string_any(notification, &["resource"])?;
        if !self.resource_accepted(&resource) {
            return None;
        }
        let receipt = receipt_key(notification);
        if self.mark_seen(&receipt) {
            return None;
        }
        let subscription_id = value_string_any(notification, &["subscriptionId"])
            .unwrap_or_else(|| "unknown".to_string());
        let change_type = value_string_any(notification, &["changeType"])
            .unwrap_or_else(|| "updated".to_string());
        let text = render_notification(notification);
        let route = MessageRoute {
            platform: "msgraph-webhook".to_string(),
            chat_id: format!("msgraph:{subscription_id}"),
            chat_type: ChatType::Direct,
            user_id: "msgraph".to_string(),
            display_name: "Microsoft Graph".to_string(),
            thread_id: format!("msgraph:{subscription_id}"),
        };
        let metadata = json!({
            "channel": {
                "platform": "msgraph-webhook",
                "adapter": "msgraph-webhook-notification",
                "tenantId": self.tenant_id,
                "clientId": self.client_id,
                "clientSecretConfigured": !self.client_secret.is_empty(),
                "chatId": route.chat_id,
                "chatType": route.chat_type.as_gateway_str(),
                "changeType": change_type,
                "resource": resource,
                "sourceMessageId": receipt,
                "subscriptionId": subscription_id
            }
        });
        Some(NormalizedInboundMessage {
            id: format!("msgraph-webhook-{}", stable_id(&receipt)),
            text,
            route,
            context: None,
            metadata,
        })
    }

    fn verify_client_state(&self, notification: &Value) -> bool {
        let Some(expected) = self.client_state.as_ref() else {
            return true;
        };
        value_string_any(notification, &["clientState"])
            .as_ref()
            .is_some_and(|provided| constant_time_eq(provided.as_bytes(), expected.as_bytes()))
    }

    fn resource_accepted(&self, resource: &str) -> bool {
        if self.accepted_resources.is_empty() {
            return true;
        }
        let resource = normalize_resource(resource);
        self.accepted_resources.iter().any(|pattern| {
            let pattern = normalize_resource(pattern);
            if pattern.ends_with('*') {
                let prefix = pattern.trim_end_matches('*').trim_end_matches('/');
                resource == prefix || resource.starts_with(&format!("{prefix}/"))
            } else {
                resource == pattern || resource.starts_with(&format!("{pattern}/"))
            }
        })
    }

    fn mark_seen(&self, receipt: &str) -> bool {
        let Ok(mut seen) = self.seen_receipts.lock() else {
            return false;
        };
        if seen.contains(receipt) {
            true
        } else {
            seen.insert(receipt.to_string());
            false
        }
    }
}

impl PlatformAdapter for MsGraphWebhookAdapter {
    fn name(&self) -> &'static str {
        "msgraph-webhook"
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
        let raw = env_first(&[
            "MSGRAPH_WEBHOOK_INBOUND_EVENT",
            "MSGRAPH_WEBHOOK_INBOUND_MESSAGE",
        ]);
        if raw.is_empty() {
            return Ok(Vec::new());
        }
        let value = serde_json::from_str::<Value>(&raw).map_err(|error| {
            ChannelError::fatal(format!("msgraph-webhook inbound JSON is invalid: {error}"))
        })?;
        let notifications = value
            .get("value")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_else(|| vec![value]);
        Ok(notifications
            .iter()
            .filter_map(|notification| self.normalize_notification(notification))
            .collect())
    }

    fn send_typing(&self, _route: &MessageRoute) -> ChannelResult<()> {
        Err(ChannelError::unavailable(
            "msgraph-webhook typing is unavailable for change notification replies",
        ))
    }

    fn send_message(&self, message: OutboundMessage) -> ChannelResult<PlatformSendOutcome> {
        let Some(url) = self.reply_webhook_url.as_ref() else {
            return Err(ChannelError::unavailable(
                "MSGRAPH_REPLY_WEBHOOK_URL is required for flyflor-cli msgraph-webhook reply delivery",
            ));
        };
        if message.text.trim().is_empty() {
            return Err(ChannelError::fatal(
                "msgraph-webhook message text must not be empty",
            ));
        }
        let response = post_json(
            url,
            json!({
                "text": message.text,
                "route": {
                    "chatId": message.route.chat_id,
                    "threadId": message.route.thread_id
                },
                "metadata": message.metadata
            }),
        )?;
        classify_msgraph_response(&response)?;
        Ok(PlatformSendOutcome {
            message_id: value_string_any(&response, &["id", "messageId"])
                .or_else(|| Some(format!("msgraph-webhook-{}", now_millis()))),
        })
    }

    fn send_media_unavailable(&self, media_kind: &str) -> ChannelResult<PlatformSendOutcome> {
        Err(ChannelError::unavailable(format!(
            "msgraph-webhook {media_kind} media delivery is explicitly unavailable in flyflor-cli"
        )))
    }
}

fn post_json(url: &str, payload: Value) -> ChannelResult<Value> {
    let body =
        serde_json::to_string(&payload).map_err(|error| ChannelError::fatal(error.to_string()))?;
    curl_json([
        "-sS",
        "--max-time",
        &seconds_arg(MSGRAPH_TIMEOUT_MS),
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
        return Err(ChannelError::retryable(format!(
            "msgraph-webhook curl failed with status {}: {}",
            output.status,
            stderr.trim()
        )));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let text = text.trim();
    if text.is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str::<Value>(text).map_err(|error| {
        ChannelError::retryable(format!(
            "msgraph-webhook returned invalid JSON: {error}; body={text}"
        ))
    })
}

fn classify_msgraph_response(value: &Value) -> ChannelResult<()> {
    let Some(error) = value.get("error") else {
        return Ok(());
    };
    let code = error
        .get("code")
        .and_then(|value| value.as_i64().or_else(|| value.as_str()?.parse().ok()))
        .unwrap_or(0);
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("unknown msgraph error");
    match code {
        401 | 403 => Err(ChannelError::session_expired(format!(
            "MSGraph authorization failed: code={code} message={message}"
        ))),
        429 => Err(ChannelError::rate_limited(format!(
            "MSGraph rate limited: code={code} message={message}"
        ))),
        400 | 404 => Err(ChannelError::fatal(format!(
            "MSGraph bad request: code={code} message={message}"
        ))),
        _ => Err(ChannelError::retryable(format!(
            "MSGraph error: code={code} message={message}"
        ))),
    }
}

fn render_notification(notification: &Value) -> String {
    let rendered = serde_json::to_string_pretty(notification).unwrap_or_else(|_| "{}".to_string());
    format!(
        "Microsoft Graph change notification:\n\n```json\n{}\n```",
        rendered.chars().take(4_000).collect::<String>()
    )
}

fn receipt_key(notification: &Value) -> String {
    value_string_any(notification, &["id"])
        .map(|id| format!("id:{id}"))
        .unwrap_or_else(|| format!("msgraph-{}", now_millis()))
}

fn normalize_resource(value: &str) -> String {
    value.trim().trim_matches('/').to_string()
}

fn stable_id(value: &str) -> String {
    value.replace(['/', ':', '.'], "-")
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

fn env_list_any(names: &[&str]) -> Vec<String> {
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

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right.iter())
        .fold(0_u8, |acc, (a, b)| acc | (a ^ b))
        == 0
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
    fn normalizes_valid_msgraph_notification() {
        let adapter = test_adapter();
        let message = adapter
            .normalize_notification(&json!({
                "id": "notif-1",
                "subscriptionId": "sub-1",
                "changeType": "updated",
                "resource": "communications/onlineMeetings/meeting-1",
                "clientState": "expected"
            }))
            .unwrap();

        assert_eq!(message.id, "msgraph-webhook-id-notif-1");
        assert!(message.text.contains("Microsoft Graph change notification"));
        assert_eq!(message.route.platform, "msgraph-webhook");
        assert_eq!(message.route.chat_id, "msgraph:sub-1");
        assert_eq!(message.metadata["channel"]["sourceMessageId"], "id:notif-1");
    }

    #[test]
    fn rejects_bad_client_state_resource_and_duplicate() {
        let adapter = test_adapter();
        assert!(
            adapter
                .normalize_notification(&json!({
                    "id": "bad-secret",
                    "subscriptionId": "sub-1",
                    "resource": "communications/onlineMeetings/meeting-1",
                    "clientState": "wrong"
                }))
                .is_none()
        );
        assert!(
            adapter
                .normalize_notification(&json!({
                    "id": "bad-resource",
                    "subscriptionId": "sub-1",
                    "resource": "users/u/messages",
                    "clientState": "expected"
                }))
                .is_none()
        );
        let value = json!({
            "id": "dup",
            "subscriptionId": "sub-1",
            "resource": "communications/onlineMeetings/meeting-1",
            "clientState": "expected"
        });
        assert!(adapter.normalize_notification(&value).is_some());
        assert!(adapter.normalize_notification(&value).is_none());
    }

    #[test]
    fn classifies_msgraph_errors() {
        assert_eq!(
            classify_msgraph_response(&json!({ "error": { "code": 403, "message": "denied" } }))
                .unwrap_err()
                .kind,
            ChannelErrorKind::SessionExpired
        );
        assert_eq!(
            classify_msgraph_response(&json!({ "error": { "code": 429, "message": "busy" } }))
                .unwrap_err()
                .kind,
            ChannelErrorKind::RateLimited
        );
        assert_eq!(
            classify_msgraph_response(&json!({ "error": { "code": 400, "message": "bad" } }))
                .unwrap_err()
                .kind,
            ChannelErrorKind::Fatal
        );
    }

    fn test_adapter() -> MsGraphWebhookAdapter {
        MsGraphWebhookAdapter {
            tenant_id: "tenant-1".to_string(),
            client_id: "client-1".to_string(),
            client_secret: "secret".to_string(),
            client_state: Some("expected".to_string()),
            accepted_resources: vec!["communications/onlineMeetings".to_string()],
            reply_webhook_url: Some("http://127.0.0.1/reply".to_string()),
            seen_receipts: Mutex::new(HashSet::new()),
        }
    }
}
