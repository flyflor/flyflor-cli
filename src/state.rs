use ratatui::layout::Position;
use ratatui_interact::state::FocusManager;

use crate::{
    context::{
        bulletin_board::{state::BulletinBoardState, todo::state::TodoState},
        conversion::state::ConversionState,
    },
    layout::state::{ContextLayoutState, HeaderState},
    RightPanelData, Turn, TodoItem,
};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum FocusTarget {
    ConversionScroll,
    ConversionInput,
    BulletinBoard,
}

pub struct AppState {
    pub header: HeaderState,
    pub layout: ContextLayoutState,
    pub focus: FocusManager<FocusTarget>,
    pub dev_mode: bool,
    pub native_selection_mode: bool,
    pub copied_notice: bool,
    pub should_quit: bool,
    pub cursor: Option<Position>,
}

impl AppState {
    pub fn new(
        turns: Vec<Turn>,
        right_panel: RightPanelData,
        todos: Vec<TodoItem>,
        dev_mode: bool,
        native_selection_mode: bool,
    ) -> Self {
        let mut focus = FocusManager::new();
        focus.register(FocusTarget::ConversionScroll);
        focus.register(FocusTarget::ConversionInput);
        focus.register(FocusTarget::BulletinBoard);
        focus.set(FocusTarget::ConversionInput);

        Self {
            header: HeaderState::default(),
            layout: ContextLayoutState {
                conversion: ConversionState::new(turns),
                bulletin_board: BulletinBoardState::new(right_panel, TodoState::new(todos)),
                ..ContextLayoutState::default()
            },
            focus,
            dev_mode,
            native_selection_mode,
            copied_notice: false,
            should_quit: false,
            cursor: None,
        }
    }
}
