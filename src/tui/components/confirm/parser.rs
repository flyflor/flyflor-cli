use serde_json::{Value, json};

use super::state::ConfirmRecord;

pub fn records_from_snapshot_payload(payload: &Value) -> Vec<ConfirmRecord> {
    let data = payload.get("data").unwrap_or(payload);
    let confirms = match data {
        Value::Array(items) => items.iter().collect::<Vec<_>>(),
        Value::Object(_) => vec![data],
        _ => Vec::new(),
    };
    confirms.into_iter().filter_map(record_from_value).collect()
}

pub fn record_from_value(confirm: &Value) -> Option<ConfirmRecord> {
    let answer = confirm.get("confirmAnswer").unwrap_or(confirm).clone();
    let summary = confirm_answer_summary(&answer)
        .or_else(|| value_string(confirm, "sourceSurface"))
        .or_else(|| value_string(confirm, "sourceKey"))
        .or_else(|| value_string(confirm, "askEventId"))
        .unwrap_or_else(|| "confirm read-model restored".to_string());
    let event_id = confirm
        .get("event")
        .and_then(|event| value_string(event, "id"))
        .or_else(|| value_string(confirm, "snapshotId"))
        .or_else(|| value_string(confirm, "askEventId"))
        .unwrap_or_else(|| "confirm-snapshot".to_string());
    Some(ConfirmRecord {
        id: format!("confirm-snapshot-{event_id}"),
        status: value_string(confirm, "status").unwrap_or_else(|| "answered".to_string()),
        summary,
        ask_event_id: value_string(confirm, "askEventId"),
        snapshot_id: value_string(confirm, "snapshotId"),
        source_key: value_string(confirm, "sourceKey"),
        source_surface: value_string(confirm, "sourceSurface"),
        answer,
        raw: confirm.clone(),
    })
}

pub fn runtime_event_from_record(record: &ConfirmRecord) -> Value {
    json!({
        "id": record.id,
        "type": "confirm.answered",
        "payload": {
            "summary": record.summary,
            "status": record.status,
            "askEventId": record.ask_event_id,
            "snapshotId": record.snapshot_id,
            "sourceKey": record.source_key,
            "sourceSurface": record.source_surface,
            "confirmAnswer": record.answer
        }
    })
}

fn confirm_answer_summary(answer: &Value) -> Option<String> {
    if let Some(summary) = value_string(answer, "summary")
        .or_else(|| value_string(answer, "answerText"))
        .or_else(|| value_string(answer, "text"))
        .or_else(|| value_string(answer, "choiceId"))
    {
        return Some(summary);
    }
    let choices = answer
        .get("choiceIds")
        .or_else(|| answer.get("choices"))?
        .as_array()?;
    let summary = choices
        .iter()
        .filter_map(Value::as_str)
        .filter(|choice| !choice.trim().is_empty())
        .take(3)
        .collect::<Vec<_>>()
        .join(", ");
    (!summary.is_empty()).then_some(summary)
}

fn value_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_records_keep_confirm_separate_from_ask() {
        let records = records_from_snapshot_payload(&json!({
            "data": [{
                "askEventId": "ask-1",
                "snapshotId": "confirm-1",
                "sourceSurface": "citizen-permission",
                "status": "answered",
                "confirmAnswer": {
                    "choiceIds": ["continue-tools", "keep-budget"]
                },
                "event": { "id": "event-1" }
            }]
        }));

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, "confirm-snapshot-event-1");
        assert_eq!(records[0].summary, "continue-tools, keep-budget");
        let event = runtime_event_from_record(&records[0]);
        assert_eq!(
            event.get("type").and_then(Value::as_str),
            Some("confirm.answered")
        );
        assert!(event.get("askAnswer").is_none());
    }
}
