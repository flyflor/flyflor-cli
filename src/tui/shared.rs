use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::tui::theme::Theme;

pub fn metric_line(key: &str, value: &str, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key:<12}"), Style::default().fg(theme.muted)),
        Span::styled(value.to_string(), Style::default().fg(theme.text)),
    ])
}

pub fn draw_separator(frame: &mut Frame, area: Rect, theme: &Theme) {
    if area.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new("─".repeat(area.width as usize)).style(Style::default().fg(theme.dim)),
        area,
    );
}

pub fn in_rect(x: u16, y: u16, area: Rect) -> bool {
    x >= area.x
        && x < area.x.saturating_add(area.width)
        && y >= area.y
        && y < area.y.saturating_add(area.height)
}

pub fn wrap_plain_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut rows = Vec::new();

    for source_line in text.split('\n') {
        if source_line.is_empty() {
            rows.push(String::new());
            continue;
        }

        let mut current = String::new();
        let mut current_width = 0;
        for ch in source_line.chars() {
            let ch_width = ch.width().unwrap_or(0).max(1);
            if current_width + ch_width > width && !current.is_empty() {
                rows.push(std::mem::take(&mut current));
                current_width = 0;
            }
            current.push(ch);
            current_width += ch_width;
        }
        if current.is_empty() {
            rows.push(String::new());
        } else {
            rows.push(current);
        }
    }

    if rows.is_empty() {
        rows.push(String::new());
    }
    rows
}

pub fn center_text(text: &str, width: usize) -> String {
    let text_width = UnicodeWidthStr::width(text);
    if text_width >= width {
        return text.to_string();
    }
    let left = (width - text_width) / 2;
    let right = width - text_width - left;
    format!("{}{}{}", " ".repeat(left), text, " ".repeat(right))
}

#[allow(dead_code)]
pub fn selection_bg() -> Color {
    Color::Rgb(60, 76, 120)
}
