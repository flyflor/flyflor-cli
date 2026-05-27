use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AskMenu {
    pub turn_index: usize,
    pub active_question: usize,
    pub continuation: Value,
    pub questions: Vec<AskQuestion>,
    pub selected_by_question: Vec<usize>,
    pub freeform_by_question: Vec<Option<String>>,
    pub editing_other: bool,
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

impl AskMenu {
    pub fn new(turn_index: usize, continuation: Value, questions: Vec<AskQuestion>) -> Self {
        let selected_by_question = vec![0; questions.len()];
        let freeform_by_question = vec![None; questions.len()];
        Self {
            turn_index,
            active_question: 0,
            continuation,
            questions,
            selected_by_question,
            freeform_by_question,
            editing_other: false,
        }
    }

    pub fn current_question(&self) -> Option<&AskQuestion> {
        self.questions.get(self.active_question)
    }

    pub fn current_choice(&self) -> Option<&AskChoice> {
        let question = self.current_question()?;
        let selected = self
            .selected_by_question
            .get(self.active_question)
            .copied()
            .unwrap_or(0);
        question.choices.get(selected)
    }

    pub fn move_choice(&mut self, delta: isize) -> bool {
        if self.editing_other {
            return false;
        }
        let Some(question) = self.questions.get(self.active_question) else {
            return false;
        };
        if question.choices.is_empty() {
            return false;
        }
        let current = self
            .selected_by_question
            .get(self.active_question)
            .copied()
            .unwrap_or(0);
        let len = question.choices.len() as isize;
        let next = (current as isize + delta).rem_euclid(len) as usize;
        if let Some(selected) = self.selected_by_question.get_mut(self.active_question) {
            *selected = next;
        }
        true
    }

    pub fn select_current_choice(&mut self, index: usize) -> bool {
        if self.editing_other {
            return false;
        }
        let Some(question) = self.questions.get(self.active_question) else {
            return false;
        };
        if index >= question.choices.len() {
            return false;
        }
        if let Some(selected) = self.selected_by_question.get_mut(self.active_question) {
            *selected = index;
            return true;
        }
        false
    }

    pub fn advance_question(&mut self) -> bool {
        self.editing_other = false;
        if self.active_question + 1 < self.questions.len() {
            self.active_question += 1;
            true
        } else {
            false
        }
    }

    pub fn set_current_freeform(&mut self, text: String) {
        if let Some(slot) = self.freeform_by_question.get_mut(self.active_question) {
            *slot = Some(text);
        }
    }

    pub fn start_current_other_input(&mut self) -> bool {
        if !self.current_choice().is_some_and(|choice| choice.is_other) {
            return false;
        }
        self.editing_other = true;
        true
    }

    pub fn is_editing_other(&self) -> bool {
        self.editing_other
    }

    pub fn answers(&self) -> Vec<AskSelection> {
        self.questions
            .iter()
            .enumerate()
            .filter_map(|(index, question)| {
                let selected = self.selected_by_question.get(index).copied().unwrap_or(0);
                let choice = question.choices.get(selected)?;
                let freeform = self
                    .freeform_by_question
                    .get(index)
                    .and_then(|value| value.clone());
                let text = if choice.is_other {
                    freeform.unwrap_or_else(|| choice.label.clone())
                } else {
                    choice.value.clone().unwrap_or_else(|| choice.label.clone())
                };
                Some(AskSelection {
                    question_id: Some(question.id.clone()),
                    choice_id: choice.id.clone(),
                    value: if choice.is_other {
                        Some(text.clone())
                    } else {
                        choice.value.clone()
                    },
                    text,
                    is_other: choice.is_other,
                })
            })
            .collect()
    }
}
