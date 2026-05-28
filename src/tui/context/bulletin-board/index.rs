use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use crate::{
    draw_scrollbar,
    i18n::text_key,
    tui::{
        context::conversion::state::slice_by_char,
        shared::{draw_separator, metric_line},
        theme::Theme,
    },
    update_scroll_state,
};

use super::{state::BulletinBoardState, todo};

pub fn render(frame: &mut Frame, area: Rect, state: &mut BulletinBoardState, theme: &Theme) {
    state.panel_area = area;
    let inner = Rect::new(
        area.x + 2,
        area.y + 1,
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    );
    if inner.width < 24 || inner.height < 14 {
        render_compact(frame, inner, theme);
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(11),
            Constraint::Length(1),
            Constraint::Min(6),
        ])
        .split(inner);

    todo::index::render(frame, rows[0], &state.todo, theme);
    draw_separator(frame, rows[1], theme);

    let scroll_text = Rect::new(rows[2].x, rows[2].y, rows[2].width, rows[2].height);
    state.content_area = scroll_text;
    state.refresh_lines(theme, scroll_text.width.max(1) as usize);
    update_scroll_state(&state.lines, &mut state.scroll, scroll_text);
    let panel_right = area.x + area.width.saturating_sub(1);
    let shifted_x = state.scroll.scrollbar.x.saturating_add(2).min(panel_right);
    state.scroll.scrollbar.x = shifted_x;
    state.scroll.scrollbar.hit_area = Rect::new(
        shifted_x,
        state.scroll.scrollbar.hit_area.y,
        (area.x + area.width).saturating_sub(shifted_x).max(1),
        state.scroll.scrollbar.hit_area.height,
    );
    let rendered_lines = apply_selection_styles(state);
    frame.render_widget(
        Paragraph::new(rendered_lines)
            .style(Style::default().fg(theme.text))
            .wrap(Wrap { trim: false })
            .scroll((state.scroll.scroll as u16, 0)),
        scroll_text,
    );
    draw_scrollbar(frame, state.scroll.scrollbar, theme);
}

fn render_compact(frame: &mut Frame, area: Rect, theme: &Theme) {
    let compact = Paragraph::new(vec![
        ratatui::text::Line::styled(text_key("todo.title"), Style::default().fg(theme.text)),
        ratatui::text::Line::styled(
            text_key("bulletin.compact.boundary"),
            Style::default().fg(theme.text),
        ),
        ratatui::text::Line::styled(
            text_key("bulletin.compact.protocol"),
            Style::default().fg(theme.pink),
        ),
        ratatui::text::Line::raw(""),
        ratatui::text::Line::styled(
            text_key("bulletin.compact.modelHeader"),
            Style::default().fg(theme.blue),
        ),
        metric_line("model", "flyflor-pro", theme),
        metric_line("provider", "OpenTUI", theme),
        ratatui::text::Line::raw(""),
        ratatui::text::Line::styled(
            text_key("bulletin.compact.healthy"),
            Style::default().fg(theme.green),
        ),
    ]);
    frame.render_widget(compact, area);
}

fn apply_selection_styles(state: &BulletinBoardState) -> Vec<Line<'static>> {
    state
        .lines
        .iter()
        .enumerate()
        .map(|(line_index, line)| {
            let actual_index = state.scroll.scroll + line_index;
            if let Some((start, end)) = state.selected_line_bounds(actual_index) {
                let text = state
                    .plain_lines
                    .get(actual_index)
                    .cloned()
                    .unwrap_or_default();
                let before = slice_by_char(&text, 0, start);
                let middle = slice_by_char(&text, start, end);
                let after = slice_by_char(&text, end, text.chars().count());
                Line::from(vec![
                    Span::raw(before),
                    Span::styled(middle, Style::default().bg(Color::Rgb(60, 76, 120))),
                    Span::raw(after),
                ])
            } else {
                line.clone()
            }
        })
        .collect()
}
