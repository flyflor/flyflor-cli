use ratatui::{
    layout::{Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

use crate::{Theme, pad_to_width, string_width_char, wrap_plain_text};

pub(crate) fn render_input_lines(input: &str, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let content_width = width.saturating_sub(2).max(1);
    if input.is_empty() {
        return vec![Line::from(vec![
            Span::styled(
                pad_to_width("ask anything...", content_width),
                Style::default().fg(theme.muted),
            ),
            Span::styled(
                " >",
                Style::default()
                    .fg(theme.purple)
                    .add_modifier(Modifier::BOLD),
            ),
        ])];
    }

    let mut lines = Vec::new();
    for source_line in input.split('\n') {
        let wrapped = wrap_plain_text(source_line, content_width);
        for row in wrapped {
            lines.push(Line::from(vec![Span::styled(
                pad_to_width(&row, content_width),
                Style::default().fg(theme.text),
            )]));
        }
    }
    if input.ends_with('\n') {
        lines.push(Line::raw(""));
    }
    if let Some(last) = lines.last_mut() {
        last.spans.push(Span::styled(
            " >",
            Style::default()
                .fg(theme.purple)
                .add_modifier(Modifier::BOLD),
        ));
    }
    lines
}

pub(crate) fn input_cursor_position(
    input: &str,
    cursor_index: usize,
    area: Rect,
    scroll: usize,
) -> Option<Position> {
    let content_width = area.width.saturating_sub(2).max(1) as usize;
    let (visual_line, visual_col) =
        input_visual_cursor(input, cursor_index.min(input.len()), content_width);

    let visible_line = visual_line.saturating_sub(scroll);
    if visible_line >= area.height as usize {
        return None;
    }
    Some(Position::new(
        area.x + visual_col.min(area.width.saturating_sub(1) as usize) as u16,
        area.y + visible_line as u16,
    ))
}

pub(crate) fn input_visual_cursor(
    input: &str,
    cursor_index: usize,
    content_width: usize,
) -> (usize, usize) {
    let mut visual_line = 0usize;
    let mut current_line_start = 0usize;
    for line in input.split_inclusive('\n') {
        let has_newline = line.ends_with('\n');
        let line_end = current_line_start + line.len() - usize::from(has_newline);
        if cursor_index <= line_end {
            let prefix = &input[current_line_start..cursor_index];
            let width = UnicodeWidthStr::width(prefix);
            return (visual_line + width / content_width, width % content_width);
        }
        let width = UnicodeWidthStr::width(&input[current_line_start..line_end]);
        visual_line += width.div_ceil(content_width).max(1);
        if has_newline {
            current_line_start += line.len();
        }
    }
    if current_line_start <= input.len() {
        let prefix = &input[current_line_start..cursor_index.min(input.len())];
        let width = UnicodeWidthStr::width(prefix);
        return (visual_line + width / content_width, width % content_width);
    }
    (visual_line, 0)
}

pub(crate) fn input_line_start_and_column(input: &str, index: usize) -> (usize, usize) {
    let index = index.min(input.len());
    let line_start = input[..index]
        .rfind('\n')
        .map(|position| position + 1)
        .unwrap_or(0);
    let column = UnicodeWidthStr::width(&input[line_start..index]);
    (line_start, column)
}

pub(crate) fn input_index_for_column(
    input: &str,
    start: usize,
    end: usize,
    target_column: usize,
) -> usize {
    let mut width = 0usize;
    for (offset, ch) in input[start..end].char_indices() {
        let next = width + string_width_char(ch);
        if next > target_column {
            return start + offset;
        }
        width = next;
    }
    end
}

pub(crate) fn normalize_paste_text(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}
