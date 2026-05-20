use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

use crate::{
    Theme,
    context::{bulletin_board, conversion},
    layout::state::ContextLayoutState,
};

pub fn render(frame: &mut Frame, area: Rect, state: &mut ContextLayoutState, theme: &Theme) {
    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(if area.width >= 150 {
            vec![Constraint::Min(84), Constraint::Length(58)]
        } else if area.width >= 120 {
            vec![Constraint::Min(68), Constraint::Length(46)]
        } else {
            vec![Constraint::Min(44), Constraint::Length(34)]
        })
        .spacing(1)
        .split(area);

    state.left_area = panels[0];
    state.right_area = panels[1];

    conversion::index::render(frame, panels[0], &mut state.conversion, theme);
    bulletin_board::index::render(frame, panels[1], &mut state.bulletin_board, theme);
}
