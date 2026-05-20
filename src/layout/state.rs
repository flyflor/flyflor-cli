use ratatui::layout::Rect;

use crate::context::{bulletin_board::state::BulletinBoardState, conversion::state::ConversionState};

#[derive(Default)]
pub struct HeaderState {
    pub turn_count: usize,
    pub dev_mode: bool,
    pub native_selection_mode: bool,
    pub copied: bool,
    pub footer_text: String,
}

pub struct ContextLayoutState {
    pub left_area: Rect,
    pub right_area: Rect,
    pub conversion: ConversionState,
    pub bulletin_board: BulletinBoardState,
}

impl Default for ContextLayoutState {
    fn default() -> Self {
        Self {
            left_area: Rect::default(),
            right_area: Rect::default(),
            conversion: ConversionState::default(),
            bulletin_board: BulletinBoardState::new(
                crate::RightPanelData {
                    thinking_label: String::new(),
                    blackboard_status: String::new(),
                    questions: Vec::new(),
                    goal_lines: Vec::new(),
                    model_stats: Vec::new(),
                    token_stats: Vec::new(),
                    context_total: String::new(),
                    context_percent: String::new(),
                    context_bar: String::new(),
                    context_usage: String::new(),
                    footer: String::new(),
                },
                crate::context::bulletin_board::todo::state::TodoState::new(Vec::new()),
            ),
        }
    }
}
