use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    i18n::text_key,
    tui::{layout::state::HeaderState, theme::Theme},
};

pub fn render(frame: &mut Frame, area: Rect, state: &HeaderState, theme: &Theme) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let left = Line::from(vec![
        Span::styled("◎", Style::default().fg(theme.purple)),
        Span::styled(
            format!(" {}", text_key("layout.header.title")),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" · {}", text_key("layout.header.poweredBy")),
            Style::default().fg(theme.muted),
        ),
    ]);
    let right = Line::from(vec![
        Span::styled("●", Style::default().fg(theme.green)),
        Span::styled(
            format!(" {}", state.status_text),
            Style::default().fg(theme.text),
        ),
        if state.copied {
            Span::styled(
                format!(" · {}", text_key("layout.header.copied")),
                Style::default().fg(theme.green),
            )
        } else {
            Span::raw("")
        },
        if state.dev_mode {
            Span::styled(
                format!(" · {}", text_key("layout.header.dev")),
                Style::default().fg(theme.pink),
            )
        } else if state.native_selection_mode {
            Span::styled(
                format!(" · {}", text_key("layout.header.select")),
                Style::default().fg(theme.blue),
            )
        } else {
            Span::raw("")
        },
    ]);

    frame.render_widget(Paragraph::new(left), cols[0]);
    frame.render_widget(Paragraph::new(right).alignment(Alignment::Right), cols[1]);
}
