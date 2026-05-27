use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Position, Rect},
    text::Line,
};
use ratatui_interact::{
    components::InputState,
    events::{get_char, is_backspace, is_delete, is_enter},
    traits::ClickRegionRegistry,
};

use crate::{
    ScrollState, ThoughtHitbox, Turn, apply_scroll_delta, drag_scroll, render_turns,
    tui::{shared::in_rect, theme::Theme},
    update_scroll_state_from_rendered,
};

#[derive(Default)]
pub struct ConversionState {
    pub turns: Vec<Turn>,
    pub chat_lines: Vec<Line<'static>>,
    pub plain_lines: Vec<String>,
    pub thought_hitboxes: Vec<ThoughtHitbox>,
    pub block_hitboxes: Vec<BlockHitbox>,
    pub selected_block: Option<SelectableBlock>,
    pub selection: Option<TextSelection>,
    pub scroll: ScrollState,
    pub input: InputState,
    pub input_click_region: ClickRegionRegistry<()>,
    pub cursor: Option<Position>,
    pub content_area: Rect,
    pub scroll_focused: bool,
    pub input_focused: bool,
}

impl ConversionState {
    pub fn new(turns: Vec<Turn>) -> Self {
        Self {
            turns,
            ..Self::default()
        }
    }

    pub fn set_scroll_focused(&mut self, focused: bool) {
        self.scroll_focused = focused;
    }

    pub fn set_input_focused(&mut self, focused: bool) {
        self.input_focused = focused;
        self.input.focused = focused;
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    pub fn has_active_selection(&self) -> bool {
        self.selection.is_some_and(|selection| selection.is_range())
    }

    pub fn update_viewport(&mut self, area: Rect, theme: &Theme) {
        self.content_area = area;
        let render = render_turns(&self.turns, area.width.max(1) as usize, theme, 0);
        self.chat_lines = render.lines;
        self.plain_lines = self.chat_lines.iter().map(line_to_plain_text).collect();
        update_scroll_state_from_rendered(&self.chat_lines, &mut self.scroll, area);
        self.thought_hitboxes = render
            .thought_regions
            .into_iter()
            .filter_map(|region| {
                let visible_index = region.line_index.checked_sub(self.scroll.scroll)?;
                if visible_index >= area.height as usize {
                    return None;
                }
                Some(ThoughtHitbox {
                    turn_index: region.turn_index,
                    rect: Rect::new(area.x, area.y + visible_index as u16, area.width, 1),
                })
            })
            .collect();
        self.block_hitboxes =
            build_block_hitboxes(&self.turns, &render.block_regions, &self.scroll, area);
    }

    pub fn selection_point_at(&self, x: u16, y: u16) -> Option<SelectionPoint> {
        if !in_rect(x, y, self.content_area) {
            return None;
        }
        let first_visible_line = self.scroll.scroll;
        let relative_y = y.checked_sub(self.content_area.y)?;
        let line_index = first_visible_line + relative_y as usize;
        let line = self.plain_lines.get(line_index)?;
        let relative_x = x.saturating_sub(self.content_area.x);
        let column = relative_x as usize;
        let clamped = column.min(line.chars().count());
        Some(SelectionPoint {
            line: line_index,
            column: clamped,
        })
    }

    pub fn contains_content(&self, x: u16, y: u16) -> bool {
        in_rect(x, y, self.content_area)
    }

    pub fn scroll_by(&mut self, delta: isize) {
        apply_scroll_delta(&mut self.scroll, delta);
    }

    pub fn page_up(&mut self) {
        let delta = -(self.scroll.viewport_height as isize - 2);
        apply_scroll_delta(&mut self.scroll, delta);
    }

    pub fn page_down(&mut self) {
        let delta = self.scroll.viewport_height as isize - 2;
        apply_scroll_delta(&mut self.scroll, delta);
    }

    pub fn scroll_to(&mut self, value: usize) {
        self.scroll.scroll = value.min(self.scroll.max_scroll);
        self.scroll.stick_to_bottom = self.scroll.scroll >= self.scroll.max_scroll;
    }

    pub fn scroll_to_max(&mut self) {
        self.scroll.scroll = self.scroll.max_scroll;
        self.scroll.stick_to_bottom = true;
    }

    pub fn drag_scroll(&mut self, anchor_scroll: usize, delta_rows: isize) {
        drag_scroll(&mut self.scroll, anchor_scroll, delta_rows);
    }

    pub fn toggle_thought_at(&mut self, x: u16, y: u16) -> bool {
        let Some(hit) = self
            .thought_hitboxes
            .iter()
            .find(|hitbox| in_rect(x, y, hitbox.rect))
            .copied()
        else {
            return false;
        };
        if let Some(thought) = self
            .turns
            .get_mut(hit.turn_index)
            .and_then(|turn| turn.thought.as_mut())
        {
            thought.expanded = !thought.expanded;
            self.scroll.stick_to_bottom = false;
            return true;
        }
        false
    }

    pub fn select_block_at(&mut self, x: u16, y: u16) -> bool {
        let Some(hit) = self
            .block_hitboxes
            .iter()
            .find(|hitbox| in_rect(x, y, hitbox.rect))
            .cloned()
        else {
            self.selected_block = None;
            return false;
        };
        self.selected_block = Some(hit.block);
        true
    }

    pub fn start_selection(&mut self, x: u16, y: u16) -> bool {
        let Some(anchor) = self.selection_point_at(x, y) else {
            self.selection = None;
            return false;
        };
        self.selection = Some(TextSelection::new(anchor));
        true
    }

    pub fn update_selection(&mut self, x: u16, y: u16) -> bool {
        let Some(point) = self.selection_point_at(x, y) else {
            return false;
        };
        if let Some(selection) = &mut self.selection {
            selection.focus = point;
            return true;
        }
        false
    }

    pub fn selection_text(&self) -> Option<String> {
        let selection = self.selection?;
        if !selection.is_range() {
            return None;
        }
        let (start, end) = selection.normalized();
        let mut output = Vec::new();
        for line_index in start.line..=end.line {
            let line = self.plain_lines.get(line_index)?;
            let start_col = if line_index == start.line {
                start.column.min(line.chars().count())
            } else {
                0
            };
            let end_col = if line_index == end.line {
                end.column.min(line.chars().count())
            } else {
                line.chars().count()
            };
            output.push(slice_by_char(line, start_col, end_col));
        }
        Some(output.join("\n"))
    }

    pub fn copy_selection(&self) -> Result<bool, String> {
        let Some(text) = self.selection_text() else {
            return Ok(false);
        };
        let mut clipboard =
            arboard::Clipboard::new().map_err(|err| format!("clipboard init failed: {err}"))?;
        clipboard
            .set_text(text)
            .map_err(|err| format!("clipboard write failed: {err}"))?;
        Ok(true)
    }

    pub fn selected_line_bounds(&self, line_index: usize) -> Option<(usize, usize)> {
        let selection = self.selection?;
        if !selection.is_range() {
            return None;
        }
        let line = self.plain_lines.get(line_index)?;
        let line_len = line.chars().count();
        let (start, end) = selection.normalized();
        if line_index < start.line || line_index > end.line {
            return None;
        }
        let start_col = if line_index == start.line {
            start.column.min(line_len)
        } else {
            0
        };
        let end_col = if line_index == end.line {
            end.column.min(line_len)
        } else {
            line_len
        };
        Some((start_col, end_col))
    }

    pub fn handle_click(&self, x: u16, y: u16) -> bool {
        self.input_click_region.handle_click(x, y).is_some()
    }

    pub fn handle_input_key(&mut self, key: KeyEvent) -> InputAction {
        if is_enter(&key) {
            let text = self.input.text.trim().to_string();
            if text.is_empty() {
                return InputAction::None;
            }
            if text == "/exit" {
                return InputAction::Quit;
            }
            self.input.clear();
            self.scroll.stick_to_bottom = true;
            return InputAction::Submit(text);
        }
        if is_backspace(&key) {
            self.input.delete_char_backward();
            return InputAction::None;
        }
        if is_delete(&key) {
            self.input.delete_char_forward();
            return InputAction::None;
        }

        match key.code {
            KeyCode::Char('c')
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                let _ = self.copy_selection();
            }
            KeyCode::Left => self.input.move_left(),
            KeyCode::Right => self.input.move_right(),
            KeyCode::Home => self.input.move_home(),
            KeyCode::End => self.input.move_end(),
            _ => {
                if let Some(ch) = get_char(&key) {
                    self.input.insert_char(ch);
                }
            }
        }
        InputAction::None
    }
}

pub enum InputAction {
    None,
    Quit,
    Submit(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SelectableBlock {
    User(usize),
    Thought(usize),
    Answer(usize),
    Footer(usize),
}

#[derive(Clone)]
pub struct BlockHitbox {
    pub block: SelectableBlock,
    pub rect: Rect,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Clone, Copy)]
pub struct SelectionPoint {
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Copy)]
pub struct TextSelection {
    pub anchor: SelectionPoint,
    pub focus: SelectionPoint,
}

impl TextSelection {
    pub fn new(anchor: SelectionPoint) -> Self {
        Self {
            anchor,
            focus: anchor,
        }
    }

    pub fn is_range(&self) -> bool {
        self.anchor.line != self.focus.line || self.anchor.column != self.focus.column
    }

    pub fn normalized(&self) -> (SelectionPoint, SelectionPoint) {
        if (self.anchor.line, self.anchor.column) <= (self.focus.line, self.focus.column) {
            (self.anchor, self.focus)
        } else {
            (self.focus, self.anchor)
        }
    }
}

fn build_block_hitboxes(
    turns: &[Turn],
    regions: &[crate::BlockRegion],
    scroll: &ScrollState,
    area: Rect,
) -> Vec<BlockHitbox> {
    let mut hitboxes = Vec::new();
    for region in regions {
        let Some(start_visible) = region.start_line.checked_sub(scroll.scroll) else {
            continue;
        };
        let Some(end_visible) = region.end_line.checked_sub(scroll.scroll) else {
            continue;
        };
        if start_visible >= area.height as usize {
            continue;
        }
        let visible_end = end_visible.min(area.height.saturating_sub(1) as usize);
        let height = visible_end.saturating_sub(start_visible) + 1;
        if height == 0 {
            continue;
        }
        let block = match region.kind {
            crate::BlockKind::User => SelectableBlock::User(region.turn_index),
            crate::BlockKind::Thought => SelectableBlock::Thought(region.turn_index),
            crate::BlockKind::Answer => SelectableBlock::Answer(region.turn_index),
            crate::BlockKind::Footer => SelectableBlock::Footer(region.turn_index),
        };
        if turns.get(region.turn_index).is_none() {
            continue;
        }
        hitboxes.push(BlockHitbox {
            block,
            rect: Rect::new(
                area.x,
                area.y + start_visible as u16,
                area.width,
                height as u16,
            ),
            start_line: region.start_line,
            end_line: region.end_line,
        });
    }
    hitboxes
}

fn line_to_plain_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<Vec<_>>()
        .join("")
}

pub(crate) fn slice_by_char(text: &str, start: usize, end: usize) -> String {
    text.chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}
