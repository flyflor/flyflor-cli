use super::state::AskMenu;

pub fn visible_item_count(menu: &AskMenu, max_items: usize) -> usize {
    menu.items.len().min(max_items)
}
