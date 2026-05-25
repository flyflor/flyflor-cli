use ratatui::layout::Rect;

use crate::context::{
    bulletin_board::state::BulletinBoardState, conversion::state::ConversionState,
};

#[derive(Default)]
pub struct HeaderState {
    pub turn_count: usize,
    pub dev_mode: bool,
    pub native_selection_mode: bool,
    pub copied: bool,
    pub footer_text: String,
    pub status_text: String,
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
                crate::RightPanelData::default_live(),
                crate::context::bulletin_board::todo::state::TodoState::new(Vec::new()),
            ),
        }
    }
}
