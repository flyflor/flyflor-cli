use serde_json::{Value, json};

use super::state::{AskChoice, AskMenu, AskQuestion};

pub const OTHER_CHOICE_ID: &str = "other";
pub const OTHER_LABEL: &str = "Other 自由输入";

pub fn ask_menu_from_turn_metadata(
    turn_index: usize,
    metadata: &Value,
) -> Option<(usize, AskMenu)> {
    ask_menu_from_metadata(turn_index, metadata).map(|menu| (turn_index, menu))
}

pub fn ask_menu_from_metadata(turn_index: usize, metadata: &Value) -> Option<AskMenu> {
    let continuation = continuation_from_metadata(metadata)?;
    let ask = metadata
        .get("ask")
        .or_else(|| metadata.get("continuation"))?;
    let mut questions = questions_from_ask(ask);
    if questions.is_empty() {
        let choices = choices_from_value(ask, None, None);
        if !choices.is_empty() {
            questions.push(AskQuestion {
                id: value_string(ask, "id")
                    .or_else(|| value_string(ask, "askId"))
                    .or_else(|| value_string(ask, "snapshotId"))
                    .unwrap_or_else(|| "ask".to_string()),
                prompt: value_string(ask, "prompt")
                    .or_else(|| value_string(ask, "question"))
                    .unwrap_or_else(|| "ASK".to_string()),
                recommended_choice_id: recommended_choice_id(ask),
                choices,
            });
        }
    }

    let mut items = flatten_questions(&questions);
    let selected = items
        .iter()
        .position(|item| item.recommended)
        .unwrap_or(0);
    items.push(other_choice());
    Some(AskMenu {
        turn_index,
        selected,
        continuation,
        questions,
        items,
    })
}

pub fn continuation_from_metadata(metadata: &Value) -> Option<Value> {
    metadata
        .get("ask")
        .and_then(continuation_from_value)
        .or_else(|| {
            metadata
                .get("continuation")
                .and_then(continuation_from_value)
        })
}

pub fn continuation_from_value(value: &Value) -> Option<Value> {
    if let Some(snapshot_id) = value_string(value, "snapshotId") {
        return Some(json!({ "mode": "continue", "snapshotId": snapshot_id }));
    }
    if let Some(continuation_id) = value_string(value, "continuationId") {
        return Some(json!({ "mode": "continue", "continuationId": continuation_id }));
    }
    None
}

fn questions_from_ask(ask: &Value) -> Vec<AskQuestion> {
    let Some(questions) = ask.get("questions").and_then(Value::as_array) else {
        return Vec::new();
    };
    questions
        .iter()
        .enumerate()
        .filter_map(question_from_value)
        .collect()
}

fn question_from_value((index, value): (usize, &Value)) -> Option<AskQuestion> {
    match value {
        Value::String(prompt) => Some(AskQuestion {
            id: format!("question-{}", index + 1),
            prompt: prompt.clone(),
            recommended_choice_id: None,
            choices: Vec::new(),
        }),
        Value::Object(_) => {
            let id = value_string(value, "id")
                .or_else(|| value_string(value, "questionId"))
                .unwrap_or_else(|| format!("question-{}", index + 1));
            let recommended_choice_id = recommended_choice_id(value);
            let choices = choices_from_value(value, Some(&id), recommended_choice_id.as_deref());
            Some(AskQuestion {
                id,
                prompt: value_string(value, "prompt")
                    .or_else(|| value_string(value, "question"))
                    .or_else(|| value_string(value, "label"))
                    .unwrap_or_else(|| format!("Question {}", index + 1)),
                recommended_choice_id,
                choices,
            })
        }
        _ => None,
    }
}

fn flatten_questions(questions: &[AskQuestion]) -> Vec<AskChoice> {
    questions
        .iter()
        .flat_map(|question| question.choices.iter().cloned())
        .collect()
}

fn choices_from_value(
    value: &Value,
    question_id: Option<&str>,
    recommended_choice_id: Option<&str>,
) -> Vec<AskChoice> {
    for key in ["choices", "options"] {
        if let Some(array) = value.get(key).and_then(Value::as_array) {
            let items = array
                .iter()
                .enumerate()
                .filter_map(|(index, item)| {
                    choice_from_value(index, item, question_id, recommended_choice_id)
                })
                .collect::<Vec<_>>();
            if !items.is_empty() {
                return items;
            }
        }
    }
    Vec::new()
}

fn choice_from_value(
    index: usize,
    value: &Value,
    question_id: Option<&str>,
    recommended_choice_id: Option<&str>,
) -> Option<AskChoice> {
    match value {
        Value::String(text) => {
            let id = format!("choice-{}", index + 1);
            Some(AskChoice {
                recommended: recommended_choice_id == Some(id.as_str()),
                id,
                label: text.clone(),
                value: Some(text.clone()),
                description: None,
                question_id: question_id.map(str::to_string),
                is_other: false,
            })
        }
        Value::Object(_) => {
            let label = value_string(value, "label")
                .or_else(|| value_string(value, "title"))
                .or_else(|| value_string(value, "text"))
                .or_else(|| value_string(value, "value"))?;
            let id = value_string(value, "id")
                .or_else(|| value_string(value, "choiceId"))
                .unwrap_or_else(|| format!("choice-{}", index + 1));
            let answer = value_string(value, "value")
                .or_else(|| value_string(value, "text"))
                .unwrap_or_else(|| label.clone());
            Some(AskChoice {
                recommended: recommended_choice_id == Some(id.as_str()),
                id,
                label,
                value: Some(answer),
                description: value_string(value, "description")
                    .or_else(|| value_string(value, "detail")),
                question_id: question_id.map(str::to_string),
                is_other: false,
            })
        }
        _ => None,
    }
}

fn other_choice() -> AskChoice {
    AskChoice {
        id: OTHER_CHOICE_ID.to_string(),
        label: OTHER_LABEL.to_string(),
        value: None,
        description: Some("自由输入".to_string()),
        question_id: None,
        recommended: false,
        is_other: true,
    }
}

fn recommended_choice_id(value: &Value) -> Option<String> {
    value_string(value, "recommendedChoiceId")
        .or_else(|| value_string(value, "recommended"))
        .or_else(|| value_string(value, "defaultChoiceId"))
}

fn value_string(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn parses_multi_question_recommended_and_other() {
        let metadata = json!({
            "ask": {
                "snapshotId": "ask-1",
                "questions": [
                    {
                        "id": "q1",
                        "prompt": "Pick mode",
                        "recommendedChoiceId": "fast",
                        "choices": [
                            { "id": "safe", "label": "Safe", "description": "More checks" },
                            { "id": "fast", "label": "Fast", "description": "Less waiting" }
                        ]
                    },
                    {
                        "id": "q2",
                        "prompt": "Pick scope",
                        "choices": ["All"]
                    }
                ]
            }
        });

        let menu = ask_menu_from_metadata(7, &metadata).expect("ask menu");

        assert_eq!(menu.turn_index, 7);
        assert_eq!(menu.questions.len(), 2);
        assert_eq!(menu.items.len(), 4);
        assert_eq!(menu.selected, 1);
        assert_eq!(menu.items[1].id, "fast");
        assert!(menu.items[1].recommended);
        assert_eq!(menu.items[1].description.as_deref(), Some("Less waiting"));
        assert!(menu.items.last().expect("other").is_other);
    }
}
