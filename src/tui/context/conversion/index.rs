use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Margin, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use ratatui_interact::components::{Input, InputStyle};
use unicode_width::UnicodeWidthStr;

use crate::{
    draw_scrollbar,
    tui::{
        context::conversion::state::{ConversionState, slice_by_char},
        shared::draw_separator,
        theme::Theme,
    },
};

pub fn render(frame: &mut Frame, area: Rect, state: &mut ConversionState, theme: &Theme) {
    state.cursor = None;
    state.input_click_region.clear();
    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(inner);

    let tip = Line::from(vec![
        Span::styled("▴", Style::default().fg(theme.pink)),
        Span::styled(
            " Type /exit to quit Flyflor chat.",
            Style::default().fg(theme.pink).add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(Paragraph::new(tip), rows[0]);

    let text_area = Rect::new(
        rows[1].x,
        rows[1].y,
        rows[1].width.saturating_sub(2),
        rows[1].height,
    );
    state.update_viewport(text_area, theme);
    let rendered_lines = apply_selection_styles(state);
    frame.render_widget(
        Paragraph::new(rendered_lines).scroll((state.scroll.scroll as u16, 0)),
        text_area,
    );
    draw_scrollbar(frame, state.scroll.scrollbar, theme);

    render_input(frame, rows[2], state, theme);
}

fn apply_selection_styles(state: &ConversionState) -> Vec<Line<'static>> {
    state
        .chat_lines
        .iter()
        .enumerate()
        .map(|(line_index, line)| {
            let mut line = line.clone();
            let block_selected = state
                .selected_block
                .map(|selected| {
                    state.block_hitboxes.iter().any(|hitbox| {
                        hitbox.block == selected
                            && line_index >= hitbox.start_line
                            && line_index <= hitbox.end_line
                    })
                })
                .unwrap_or(false);
            if block_selected {
                for span in &mut line.spans {
                    span.style = span.style.bg(Color::Rgb(18, 24, 40));
                }
            }
            if let Some((start, end)) = state.selected_line_bounds(line_index) {
                let text = state
                    .plain_lines
                    .get(line_index)
                    .cloned()
                    .unwrap_or_default();
                let before = slice_by_char(&text, 0, start);
                let middle = slice_by_char(&text, start, end);
                let after = slice_by_char(&text, end, text.chars().count());
                return Line::from(vec![
                    Span::raw(before),
                    Span::styled(middle, Style::default().bg(Color::Rgb(60, 76, 120))),
                    Span::raw(after),
                ]);
            }
            line
        })
        .collect()
}

fn render_input(frame: &mut Frame, area: Rect, state: &mut ConversionState, theme: &Theme) {
    draw_separator(frame, Rect::new(area.x, area.y, area.width, 1), theme);
    let input_inner = Rect::new(
        area.x,
        area.y + 1,
        area.width,
        area.height.saturating_sub(1),
    );
    if input_inner.height == 0 {
        return;
    }

    let input_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(input_inner);

    let input_style = InputStyle::default()
        .text_fg(theme.text)
        .cursor_fg(theme.purple)
        .placeholder_fg(theme.muted)
        .focused_border(theme.purple)
        .unfocused_border(theme.dim);

    let region = Input::new(&state.input)
        .placeholder("ask anything...")
        .style(input_style)
        .with_border(false)
        .render_stateful(frame, input_rows[0]);
    state.input_click_region.register(region.area, ());

    if state.input.focused {
        let cursor_x = input_rows[0].x
            + if state.input.is_empty() {
                0
            } else {
                UnicodeWidthStr::width(state.input.text_before_cursor()) as u16
            };
        state.cursor = Some(Position::new(cursor_x, input_rows[0].y));
    }

    let help = if input_rows[1].width >= 72 {
        Line::from(vec![
            Span::styled("Enter", Style::default().fg(theme.text)),
            Span::styled(" 发送", Style::default().fg(theme.muted)),
            Span::styled("  |  Tab", Style::default().fg(theme.text)),
            Span::styled(" 切换 Panel", Style::default().fg(theme.muted)),
            Span::styled("  |  Click", Style::default().fg(theme.text)),
            Span::styled(" 展开 Thought", Style::default().fg(theme.muted)),
            Span::styled("  |  Ctrl+D", Style::default().fg(theme.text)),
            Span::styled(" DEV", Style::default().fg(theme.muted)),
        ])
    } else {
        Line::from(vec![
            Span::styled("Enter", Style::default().fg(theme.text)),
            Span::styled(" send", Style::default().fg(theme.muted)),
            Span::styled("  |  Tab panels", Style::default().fg(theme.muted)),
        ])
    };
    frame.render_widget(Paragraph::new(help), input_rows[1]);
}
