use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    i18n::text_key,
    tui::{shared::center_text, theme::Theme},
};

use super::state::TodoState;

pub fn render(frame: &mut Frame, area: Rect, state: &TodoState, theme: &Theme) {
    frame.render_widget(
        Paragraph::new(Line::styled(
            text_key("todo.title"),
            Style::default().fg(theme.text),
        )),
        Rect::new(area.x, area.y, area.width, 1),
    );
    let todo_start_y = area.y + 2;
    for (index, item) in state.items.iter().enumerate() {
        let y = todo_start_y + index as u16;
        if y >= area.bottom() {
            break;
        }
        draw_todo_row(
            frame,
            Rect::new(area.x, y, area.width, 1),
            &item.marker,
            &item.label,
            &item.status,
            item.active,
            theme,
        );
    }
}

fn draw_todo_row(
    frame: &mut Frame,
    area: Rect,
    marker: &str,
    label: &str,
    status: &str,
    active: bool,
    theme: &Theme,
) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(10), Constraint::Length(8)])
        .split(area);

    let accent = if active { theme.pink } else { theme.text };
    let left = Line::from(vec![
        Span::styled(format!("{marker} "), Style::default().fg(accent)),
        Span::styled(label.to_string(), Style::default().fg(accent)),
    ]);
    frame.render_widget(Paragraph::new(left), cols[0]);

    let badge = Paragraph::new(Line::styled(
        center_text(status, cols[1].width as usize),
        Style::default().fg(if active { theme.blue } else { theme.muted }),
    ));
    frame.render_widget(badge, cols[1]);
}
