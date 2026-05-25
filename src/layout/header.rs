use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{Theme, layout::state::HeaderState};

pub fn render(frame: &mut Frame, area: Rect, state: &HeaderState, theme: &Theme) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let left = Line::from(vec![
        Span::styled("◎", Style::default().fg(theme.purple)),
        Span::styled(
            " flyflor-chat",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" · powered by OpenTUI", Style::default().fg(theme.muted)),
    ]);
    let right = Line::from(vec![
        Span::styled("●", Style::default().fg(theme.green)),
        Span::styled(
            format!(" {}", state.status_text),
            Style::default().fg(theme.text),
        ),
        if state.copied {
            Span::styled(" · COPIED", Style::default().fg(theme.green))
        } else {
            Span::raw("")
        },
        if state.dev_mode {
            Span::styled(" · DEV", Style::default().fg(theme.pink))
        } else if state.native_selection_mode {
            Span::styled(" · SELECT", Style::default().fg(theme.blue))
        } else {
            Span::raw("")
        },
    ]);

    frame.render_widget(Paragraph::new(left), cols[0]);
    frame.render_widget(Paragraph::new(right).alignment(Alignment::Right), cols[1]);
}
