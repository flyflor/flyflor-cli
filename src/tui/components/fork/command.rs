use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForkCreateSource {
    pub user: String,
    pub answer: String,
    pub source_event_id: Option<String>,
    pub source_message_id: Option<String>,
    pub source_ask_id: Option<String>,
}

pub fn fork_create_payload(
    source: &ForkCreateSource,
    active_context_fork_id: Option<&str>,
) -> Option<Value> {
    let source_event_id = source
        .source_event_id
        .clone()
        .or_else(|| source.source_message_id.clone())?;
    let summary = if source.answer.trim().is_empty() {
        source.user.trim().to_string()
    } else {
        truncate_to_width(&source.answer.replace('\n', " "), 240)
    };
    let mut payload = json!({
        "title": truncate_to_width(&source.user.replace('\n', " "), 80),
        "summary": summary,
        "continuitySummary": truncate_to_width(&source.answer.replace('\n', " "), 600),
        "inheritedEventIds": [source_event_id],
        "maxContextTokens": 12000,
    });
    if let Some(event_id) = &source.source_event_id {
        payload["sourceEventId"] = json!(event_id);
    }
    if let Some(parent_id) = active_context_fork_id {
        payload["context"] = fork_message_context(parent_id);
    }
    if let Some(ask_id) = &source.source_ask_id {
        payload["sourceAskId"] = json!(ask_id);
    }
    Some(payload)
}

pub fn fork_message_context(context_fork_id: &str) -> Value {
    json!({ "contextForkId": context_fork_id })
}

fn truncate_to_width(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let keep = width.saturating_sub(1);
    let mut output = text.chars().take(keep).collect::<String>();
    output.push('…');
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_fork_create_payload_with_context_propagation() {
        let source = ForkCreateSource {
            user: "create fork".to_string(),
            answer: "fork answer".to_string(),
            source_event_id: Some("event-1".to_string()),
            source_message_id: Some("message-1".to_string()),
            source_ask_id: Some("ask-1".to_string()),
        };

        let payload = fork_create_payload(&source, Some("parent-fork")).expect("payload");

        assert_eq!(
            payload.get("sourceEventId").and_then(Value::as_str),
            Some("event-1")
        );
        assert_eq!(
            payload.get("sourceAskId").and_then(Value::as_str),
            Some("ask-1")
        );
        assert_eq!(
            payload
                .get("context")
                .and_then(|context| context.get("contextForkId"))
                .and_then(Value::as_str),
            Some("parent-fork")
        );
    }
}
