use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    text::Line,
    widgets::Paragraph,
};

use crate::tui::{layout::state::HeaderState, theme::Theme};

pub fn render(frame: &mut Frame, area: Rect, state: &HeaderState, theme: &Theme) {
    frame.render_widget(
        Paragraph::new(Line::styled(
            state.footer_text.clone(),
            Style::default().fg(theme.footer_muted),
        ))
        .alignment(Alignment::Right),
        area,
    );
}
