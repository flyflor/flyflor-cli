use serde_json::{Value, json};

use crate::i18n::text_key;

use super::state::AskSelection;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AskAnswer {
    pub question_id: Option<String>,
    pub choice_id: String,
    pub text: String,
    pub value: Option<String>,
    pub is_other: bool,
}

const CITIZEN_PERMISSION_CHOICES: [&str; 3] = ["continue-tools", "keep-budget", "keep-subagents"];

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

pub fn ask_message_metadata_many(continuation: Value, answers: &[AskAnswer]) -> Value {
    if let Some(permission) = citizen_permission_metadata(answers) {
        return json!({
            "continuation": continuation,
            "citizenPermission": permission,
            "confirmAnswer": confirm_answer_metadata(answers)
        });
    }
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
    let metadata = json!({
        "continuation": continuation,
        "askAnswer": ask_answer
    });
    metadata
}

pub fn ask_message_text(answers: &[AskAnswer]) -> String {
    if citizen_permission_choices(answers).is_empty() {
        return answers
            .iter()
            .map(|answer| answer.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
    }
    text_key("ask.confirmSubmitted")
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

fn citizen_permission_metadata(answers: &[AskAnswer]) -> Option<Value> {
    let choices = citizen_permission_choices(answers);
    if choices.is_empty() {
        return None;
    }
    Some(json!({
        "kind": "execution-policy",
        "choices": choices
    }))
}

fn confirm_answer_metadata(answers: &[AskAnswer]) -> Value {
    let mut confirm_answer = json!({
        "answers": answers.iter().map(ask_answer_metadata).collect::<Vec<_>>()
    });
    if answers.len() == 1 {
        if let Some(object) = confirm_answer.as_object_mut() {
            if let Some(single) = answers.first().map(ask_answer_metadata) {
                if let Some(single_object) = single.as_object() {
                    for (key, value) in single_object {
                        object.insert(key.clone(), value.clone());
                    }
                }
            }
        }
    }
    confirm_answer
}

pub fn is_citizen_permission_answers(answers: &[AskAnswer]) -> bool {
    !citizen_permission_choices(answers).is_empty()
}

fn citizen_permission_choices(answers: &[AskAnswer]) -> Vec<String> {
    answers
        .iter()
        .filter_map(citizen_permission_choice)
        .collect()
}

fn citizen_permission_choice(answer: &AskAnswer) -> Option<String> {
    [
        Some(answer.choice_id.as_str()),
        answer.value.as_deref(),
        Some(answer.text.as_str()),
    ]
    .into_iter()
    .flatten()
    .map(|value| value.trim().to_ascii_lowercase())
    .find(|value| {
        CITIZEN_PERMISSION_CHOICES
            .iter()
            .any(|choice| *choice == value.as_str())
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn builds_metadata_with_ask_answer_array_and_continuation() {
        let answers = vec![AskAnswer {
            question_id: Some("q1".to_string()),
            choice_id: "fast".to_string(),
            text: "Fast".to_string(),
            value: Some("fast".to_string()),
            is_other: false,
        }];

        let metadata = ask_message_metadata_many(
            json!({ "mode": "continue", "snapshotId": "ask-1" }),
            &answers,
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
                .and_then(|ask_answer| ask_answer.get("answers"))
                .and_then(Value::as_array)
                .and_then(|answers| answers.first())
                .and_then(|answer| answer.get("choiceId"))
                .and_then(Value::as_str),
            Some("fast")
        );
    }

    #[test]
    fn freeform_answer_metadata_keeps_question_and_choice_id() {
        let answer = AskAnswer {
            question_id: Some("q1".to_string()),
            choice_id: "custom".to_string(),
            text: "Custom".to_string(),
            value: Some("Custom".to_string()),
            is_other: true,
        };
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
    fn builds_single_answer_metadata_with_top_level_compat() {
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
            ask_answer.get("questionId").and_then(Value::as_str),
            Some("q1")
        );
        assert_eq!(
            ask_answer.get("choiceId").and_then(Value::as_str),
            Some("continue")
        );
        assert_eq!(
            ask_answer.get("text").and_then(Value::as_str),
            Some("Continue")
        );
        assert_eq!(
            ask_answer.get("value").and_then(Value::as_str),
            Some("continue")
        );
        assert_eq!(
            ask_answer.get("isOther").and_then(Value::as_bool),
            Some(false)
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

    #[test]
    fn citizen_permission_answer_adds_confirm_metadata_and_safe_text() {
        let answers = vec![AskAnswer {
            question_id: Some("permission".to_string()),
            choice_id: "continue-tools".to_string(),
            text: "continue-tools".to_string(),
            value: Some("continue-tools".to_string()),
            is_other: false,
        }];

        let metadata = ask_message_metadata_many(json!({ "snapshotId": "ask-1" }), &answers);

        assert_eq!(ask_message_text(&answers), "已提交执行授权策略");
        assert!(metadata.get("askAnswer").is_none());
        assert_eq!(
            metadata
                .get("citizenPermission")
                .and_then(|permission| permission.get("kind"))
                .and_then(Value::as_str),
            Some("execution-policy")
        );
        assert_eq!(
            metadata
                .get("citizenPermission")
                .and_then(|permission| permission.get("choices"))
                .and_then(Value::as_array)
                .and_then(|choices| choices.first())
                .and_then(Value::as_str),
            Some("continue-tools")
        );
        assert_eq!(
            metadata
                .get("confirmAnswer")
                .and_then(|answer| answer.get("answers"))
                .and_then(Value::as_array)
                .and_then(|answers| answers.first())
                .and_then(|answer| answer.get("choiceId"))
                .and_then(Value::as_str),
            Some("continue-tools")
        );
        assert!(is_citizen_permission_answers(&answers));
    }
}
