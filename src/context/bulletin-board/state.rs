use ratatui::layout::Rect;
use ratatui::text::{Line, Span};

use crate::{RightPanelData, ScrollState, Theme, in_rect, wrap_plain_text};

use crate::context::conversion::state::slice_by_char;

use super::todo::state::TodoState;

pub struct BulletinBoardState {
    pub todo: TodoState,
    pub questions: QuestionsState,
    pub details: DetailsState,
    pub lines: Vec<Line<'static>>,
    pub scroll: ScrollState,
    pub panel_area: Rect,
    pub content_area: Rect,
    pub plain_lines: Vec<String>,
    pub selection: Option<TextSelection>,
    pub focused: bool,
}

impl BulletinBoardState {
    pub fn new(right_panel: RightPanelData, todo: TodoState) -> Self {
        Self {
            todo,
            questions: QuestionsState {
                thinking_label: right_panel.thinking_label,
                questions: right_panel.questions,
            },
            details: DetailsState {
                blackboard_status: right_panel.blackboard_status,
                goal_lines: right_panel.goal_lines,
                model_stats: right_panel.model_stats,
                token_stats: right_panel.token_stats,
                context_total: right_panel.context_total,
                context_percent: right_panel.context_percent,
                context_bar: right_panel.context_bar,
                context_usage: right_panel.context_usage,
                footer: right_panel.footer,
            },
            lines: Vec::new(),
            scroll: ScrollState::default(),
            panel_area: Rect::default(),
            content_area: Rect::default(),
            plain_lines: Vec::new(),
            selection: None,
            focused: false,
        }
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    pub fn apply_right_panel_data(&mut self, right_panel: RightPanelData) {
        self.questions.thinking_label = right_panel.thinking_label;
        self.questions.questions = right_panel.questions;
        self.details.blackboard_status = right_panel.blackboard_status;
        self.details.goal_lines = right_panel.goal_lines;
        self.details.model_stats = right_panel.model_stats;
        self.details.token_stats = right_panel.token_stats;
        self.details.context_total = right_panel.context_total;
        self.details.context_percent = right_panel.context_percent;
        self.details.context_bar = right_panel.context_bar;
        self.details.context_usage = right_panel.context_usage;
        self.details.footer = right_panel.footer;
    }

    pub fn refresh_lines(&mut self, theme: &Theme, width: usize) {
        let mut lines = Vec::new();
        let mut plain_lines = Vec::new();
        push_plain_line(
            &mut lines,
            &mut plain_lines,
            "Blackboard  ".to_string(),
            format!("[{}]", self.questions.thinking_label),
            Style::default().fg(theme.text),
            Style::default().fg(theme.purple),
            width,
        );
        push_single_line(
            &mut lines,
            &mut plain_lines,
            "Questions",
            Style::default().fg(theme.blue),
        );
        for question in &self.questions.questions {
            let style = Style::default().fg(if question.starts_with('›') {
                theme.pink
            } else {
                theme.text
            });
            for wrapped in wrap_plain_text(question, width.max(1)) {
                push_single_line(&mut lines, &mut plain_lines, &wrapped, style);
            }
        }
        push_single_line(
            &mut lines,
            &mut plain_lines,
            "Blackboard",
            Style::default().fg(theme.text),
        );
        for wrapped in wrap_plain_text(&format!("   {}", self.details.blackboard_status), width.max(1)) {
            push_single_line(
                &mut lines,
                &mut plain_lines,
                &wrapped,
                Style::default().fg(theme.text),
            );
        }
        push_single_line(
            &mut lines,
            &mut plain_lines,
            "   goal:",
            Style::default().fg(theme.text),
        );
        for line in &self.details.goal_lines {
            for wrapped in wrap_plain_text(&format!("   {line}"), width.max(1)) {
                push_single_line(
                    &mut lines,
                    &mut plain_lines,
                    &wrapped,
                    Style::default().fg(theme.muted),
                );
            }
        }
        push_single_line(&mut lines, &mut plain_lines, "MODEL", Style::default().fg(theme.blue));
        for stat in &self.details.model_stats {
            let text = format!("{:<12}{}", stat.label, stat.value);
            push_metric_line(&mut lines, &mut plain_lines, &text, theme);
        }
        push_single_line(&mut lines, &mut plain_lines, "TOKENS", Style::default().fg(theme.blue));
        for stat in &self.details.token_stats {
            let text = format!("{:<12}{}", stat.label, stat.value);
            push_metric_line(&mut lines, &mut plain_lines, &text, theme);
        }
        push_single_line(
            &mut lines,
            &mut plain_lines,
            "CONTEXT WINDOW",
            Style::default().fg(theme.blue),
        );
        let context_summary = format!(
            "{}                {}",
            self.details.context_total, self.details.context_percent
        );
        push_single_line(
            &mut lines,
            &mut plain_lines,
            &context_summary,
            Style::default().fg(theme.text),
        );
        push_single_line(
            &mut lines,
            &mut plain_lines,
            &self.details.context_bar,
            Style::default().fg(theme.purple),
        );
        push_single_line(
            &mut lines,
            &mut plain_lines,
            &self.details.context_usage,
            Style::default().fg(theme.text),
        );
        self.lines = lines;
        self.plain_lines = plain_lines;
    }

    pub fn scroll_by(&mut self, delta: isize) {
        crate::apply_scroll_delta(&mut self.scroll, delta);
    }

    pub fn page_up(&mut self) {
        let delta = -(self.scroll.viewport_height as isize - 2);
        crate::apply_scroll_delta(&mut self.scroll, delta);
    }

    pub fn page_down(&mut self) {
        let delta = self.scroll.viewport_height as isize - 2;
        crate::apply_scroll_delta(&mut self.scroll, delta);
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
        crate::drag_scroll(&mut self.scroll, anchor_scroll, delta_rows);
    }

    pub fn contains_scrollbar(&self, x: u16, y: u16) -> bool {
        self.scroll.scrollbar.contains(x, y)
    }

    pub fn contains_panel(&self, x: u16, y: u16) -> bool {
        in_rect(x, y, self.panel_area)
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    pub fn has_active_selection(&self) -> bool {
        self.selection.is_some_and(|selection| selection.is_range())
    }

    pub fn selection_point_at(&self, x: u16, y: u16) -> Option<SelectionPoint> {
        if !in_rect(x, y, self.content_area) {
            return None;
        }
        let line_index = self.scroll.scroll + y.checked_sub(self.content_area.y)? as usize;
        let line = self.plain_lines.get(line_index)?;
        let relative_x = x.saturating_sub(self.content_area.x) as usize;
        Some(SelectionPoint {
            line: line_index,
            column: relative_x.min(line.chars().count()),
        })
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

pub struct QuestionsState {
    pub thinking_label: String,
    pub questions: Vec<String>,
}

pub struct DetailsState {
    pub blackboard_status: String,
    pub goal_lines: Vec<String>,
    pub model_stats: Vec<crate::StatItem>,
    pub token_stats: Vec<crate::StatItem>,
    pub context_total: String,
    pub context_percent: String,
    pub context_bar: String,
    pub context_usage: String,
    pub footer: String,
}

use ratatui::style::Style;

fn metric_line_owned(key: &str, value: &str, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key:<12}"), Style::default().fg(theme.muted)),
        Span::styled(value.to_string(), Style::default().fg(theme.text)),
    ])
}

fn push_single_line(
    lines: &mut Vec<Line<'static>>,
    plain_lines: &mut Vec<String>,
    text: &str,
    style: Style,
) {
    lines.push(Line::styled(text.to_string(), style));
    plain_lines.push(text.to_string());
}

fn push_plain_line(
    lines: &mut Vec<Line<'static>>,
    plain_lines: &mut Vec<String>,
    prefix: String,
    suffix: String,
    prefix_style: Style,
    suffix_style: Style,
    width: usize,
) {
    let full = format!("{prefix}{suffix}");
    for wrapped in wrap_plain_text(&full, width.max(1)) {
        let suffix_start = prefix.chars().count().min(wrapped.chars().count());
        let left = slice_by_char(&wrapped, 0, suffix_start);
        let right = slice_by_char(&wrapped, suffix_start, wrapped.chars().count());
        lines.push(Line::from(vec![
            Span::styled(left, prefix_style),
            Span::styled(right, suffix_style),
        ]));
        plain_lines.push(wrapped);
    }
}

fn push_metric_line(
    lines: &mut Vec<Line<'static>>,
    plain_lines: &mut Vec<String>,
    text: &str,
    theme: &Theme,
) {
    let key = text.chars().take(12).collect::<String>();
    let value = text.chars().skip(12).collect::<String>();
    lines.push(metric_line_owned(key.trim_end(), &value, theme));
    plain_lines.push(text.to_string());
}
