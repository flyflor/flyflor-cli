use serde_json::{Value, json};

use super::state::AskSelection;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AskAnswer {
    pub question_id: Option<String>,
    pub choice_id: String,
    pub text: String,
    pub value: Option<String>,
    pub is_other: bool,
}

impl AskAnswer {
    pub fn other(text: String) -> Self {
        Self::other_for_question(text, None)
    }

    pub fn other_for_question(text: String, question_id: Option<String>) -> Self {
        Self::other_for_choice(text, question_id, "other".to_string())
    }

    pub fn other_for_choice(text: String, question_id: Option<String>, choice_id: String) -> Self {
        Self {
            question_id,
            choice_id,
            value: Some(text.clone()),
            text,
            is_other: true,
        }
    }
}

impl From<AskSelection> for AskAnswer {
    fn from(selection: AskSelection) -> Self {
        Self {
            question_id: selection.question_id,
            choice_id: selection.choice_id,
            text: selection.text,
            value: selection.value,
            is_other: selection.is_other,
        }
    }
}

pub fn ask_message_metadata(continuation: Value, answer: &AskAnswer) -> Value {
    json!({
        "continuation": continuation,
        "askAnswer": ask_answer_metadata(answer)
    })
}

pub fn ask_message_metadata_many(continuation: Value, answers: &[AskAnswer]) -> Value {
    let mut ask_answer = json!({
        "answers": answers.iter().map(ask_answer_metadata).collect::<Vec<_>>()
    });
    if answers.len() == 1 {
        if let Some(object) = ask_answer.as_object_mut() {
            if let Some(single) = answers.first().map(ask_answer_metadata) {
                if let Some(single_object) = single.as_object() {
                    for (key, value) in single_object {
                        object.insert(key.clone(), value.clone());
                    }
                }
            }
        }
    }
    json!({
        "continuation": continuation,
        "askAnswer": ask_answer
    })
}

pub fn ask_answer_metadata(answer: &AskAnswer) -> Value {
    json!({
        "questionId": answer.question_id,
        "choiceId": answer.choice_id,
        "text": answer.text,
        "value": answer.value,
        "isOther": answer.is_other
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn builds_metadata_with_ask_answer_and_continuation() {
        let answer = AskAnswer {
            question_id: Some("q1".to_string()),
            choice_id: "fast".to_string(),
            text: "Fast".to_string(),
            value: Some("fast".to_string()),
            is_other: false,
        };

        let metadata = ask_message_metadata(
            json!({ "mode": "continue", "snapshotId": "ask-1" }),
            &answer,
        );

        assert_eq!(
            metadata
                .get("continuation")
                .and_then(|continuation| continuation.get("snapshotId"))
                .and_then(Value::as_str),
            Some("ask-1")
        );
        assert_eq!(
            metadata
                .get("askAnswer")
                .and_then(|ask_answer| ask_answer.get("choiceId"))
                .and_then(Value::as_str),
            Some("fast")
        );
    }

    #[test]
    fn freeform_answer_can_keep_question_id() {
        let answer = AskAnswer::other_for_question("Custom".to_string(), Some("q1".to_string()));
        let metadata = ask_answer_metadata(&answer);

        assert_eq!(
            metadata.get("questionId").and_then(Value::as_str),
            Some("q1")
        );
        assert_eq!(
            metadata.get("choiceId").and_then(Value::as_str),
            Some("other")
        );
        assert_eq!(metadata.get("isOther").and_then(Value::as_bool), Some(true));
    }

    #[test]
    fn freeform_answer_can_keep_inbound_other_choice_id() {
        let answer = AskAnswer::other_for_choice(
            "Custom".to_string(),
            Some("q1".to_string()),
            "custom".to_string(),
        );
        let metadata = ask_answer_metadata(&answer);

        assert_eq!(
            metadata.get("questionId").and_then(Value::as_str),
            Some("q1")
        );
        assert_eq!(
            metadata.get("choiceId").and_then(Value::as_str),
            Some("custom")
        );
        assert_eq!(metadata.get("isOther").and_then(Value::as_bool), Some(true));
    }

    #[test]
    fn builds_multi_answer_metadata_with_legacy_single_compat() {
        let answers = vec![AskAnswer {
            question_id: Some("q1".to_string()),
            choice_id: "continue".to_string(),
            text: "Continue".to_string(),
            value: Some("continue".to_string()),
            is_other: false,
        }];
        let metadata = ask_message_metadata_many(json!({ "snapshotId": "ask-1" }), &answers);
        let ask_answer = metadata.get("askAnswer").expect("askAnswer");

        assert_eq!(
            ask_answer
                .get("answers")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            ask_answer.get("choiceId").and_then(Value::as_str),
            Some("continue")
        );
    }

    #[test]
    fn builds_multi_answer_metadata_without_flattening_questions() {
        let answers = vec![
            AskAnswer {
                question_id: Some("q1".to_string()),
                choice_id: "a".to_string(),
                text: "A".to_string(),
                value: Some("A".to_string()),
                is_other: false,
            },
            AskAnswer {
                question_id: Some("q2".to_string()),
                choice_id: "other".to_string(),
                text: "custom".to_string(),
                value: Some("custom".to_string()),
                is_other: true,
            },
        ];
        let metadata = ask_message_metadata_many(json!({ "snapshotId": "ask-1" }), &answers);
        let ask_answer = metadata.get("askAnswer").expect("askAnswer");

        assert_eq!(
            ask_answer
                .get("answers")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2)
        );
        assert!(ask_answer.get("choiceId").is_none());
    }
}
