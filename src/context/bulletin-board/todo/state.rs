use crate::TodoItem;

pub struct TodoState {
    pub items: Vec<TodoItem>,
}

impl TodoState {
    pub fn new(items: Vec<TodoItem>) -> Self {
        Self { items }
    }
}
