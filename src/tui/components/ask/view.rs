use super::state::AskMenu;

pub fn visible_item_count(menu: &AskMenu, max_items: usize) -> usize {
    menu.current_question()
        .map(|question| question.choices.len().min(max_items))
        .unwrap_or(0)
}
