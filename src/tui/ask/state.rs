use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AskMenu {
    pub turn_index: usize,
    pub selected: usize,
    pub continuation: Value,
    pub questions: Vec<AskQuestion>,
    pub items: Vec<AskChoice>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AskQuestion {
    pub id: String,
    pub prompt: String,
    pub recommended_choice_id: Option<String>,
    pub choices: Vec<AskChoice>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AskChoice {
    pub id: String,
    pub label: String,
    pub value: Option<String>,
    pub description: Option<String>,
    pub question_id: Option<String>,
    pub recommended: bool,
    pub is_other: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AskSelection {
    pub question_id: Option<String>,
    pub choice_id: String,
    pub text: String,
    pub value: Option<String>,
    pub is_other: bool,
}
