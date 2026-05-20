use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Clear, Paragraph},
};
use ratatui_interact::events::{is_backtab, is_tab};

use crate::{
    Theme,
    layout::{context, footer, header},
    state::{AppState, FocusTarget},
};

#[derive(Clone, Copy)]
enum ScrollTarget {
    Conversion,
    BulletinBoard,
}

#[derive(Clone, Copy)]
enum DragMode {
    Scroll(ScrollTarget),
    ConversionSelection,
    BulletinBoardSelection,
}

#[derive(Clone, Copy)]
struct DragState {
    mode: DragMode,
    anchor_row: u16,
    anchor_scroll: usize,
}

#[derive(Clone, Copy, Default)]
pub struct LayoutSnapshot {
    pub frame: Rect,
    pub content: Rect,
    pub top_bar: Rect,
    pub left_panel: Rect,
    pub right_panel: Rect,
}

pub struct App {
    pub state: AppState,
    drag: Option<DragState>,
    layout: LayoutSnapshot,
}

impl App {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            drag: None,
            layout: LayoutSnapshot::default(),
        }
    }

    fn is_copy_shortcut(key: &KeyEvent) -> bool {
        let is_c = matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'));
        if !is_c {
            return false;
        }
        if key.modifiers.contains(KeyModifiers::SUPER) {
            return true;
        }
        key.modifiers.contains(KeyModifiers::CONTROL) && key.modifiers.contains(KeyModifiers::SHIFT)
    }

    fn copy_active_selection(&mut self) -> bool {
        let copied = if self.state.layout.conversion.has_active_selection() {
            self.state.layout.conversion.copy_selection()
        } else if self.state.layout.bulletin_board.has_active_selection() {
            self.state.layout.bulletin_board.copy_selection()
        } else {
            return false;
        };
        match copied {
            Ok(true) => {
                self.state.layout.conversion.clear_selection();
                self.state.layout.bulletin_board.clear_selection();
                self.state.copied_notice = true;
                true
            }
            _ => false,
        }
    }

    pub fn sync_focus(&mut self) {
        let current = self.state.focus.current().copied();
        self.state
            .layout
            .conversion
            .set_scroll_focused(current == Some(FocusTarget::ConversionScroll));
        self.state
            .layout
            .conversion
            .set_input_focused(current == Some(FocusTarget::ConversionInput));
        self.state
            .layout
            .bulletin_board
            .set_focused(current == Some(FocusTarget::BulletinBoard));
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        self.state.copied_notice = false;

        if self.state.native_selection_mode {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => self.state.native_selection_mode = false,
                KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.state.native_selection_mode = false;
                }
                KeyCode::Char('q') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.state.should_quit = true
                }
                _ => {}
            }
            return;
        }

        if Self::is_copy_shortcut(&key) {
            let _ = self.copy_active_selection();
            return;
        }

        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            let _ = self.copy_active_selection();
            return;
        }

        match key.code {
            KeyCode::Char('q') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.state.should_quit = true
            }
            KeyCode::F(6) => self.state.native_selection_mode = true,
            KeyCode::Char('y') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.state.native_selection_mode = true
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.state.dev_mode = !self.state.dev_mode
            }
            KeyCode::F(2) => self.state.dev_mode = !self.state.dev_mode,
            _ => {}
        }

        if is_tab(&key) {
            self.state.focus.next();
            self.sync_focus();
            return;
        }
        if is_backtab(&key) {
            self.state.focus.prev();
            self.sync_focus();
            return;
        }

        match self.state.focus.current().copied() {
            Some(FocusTarget::ConversionScroll) => match key.code {
                KeyCode::Up | KeyCode::Char('k') => self.state.layout.conversion.scroll_by(-3),
                KeyCode::Down | KeyCode::Char('j') => self.state.layout.conversion.scroll_by(3),
                KeyCode::PageUp => self.state.layout.conversion.page_up(),
                KeyCode::PageDown | KeyCode::Char(' ') => self.state.layout.conversion.page_down(),
                KeyCode::Char('g') => self.state.layout.conversion.scroll_to(0),
                KeyCode::Char('G') => self.state.layout.conversion.scroll_to_max(),
                KeyCode::Enter => {
                    self.state.focus.set(FocusTarget::ConversionInput);
                    self.sync_focus();
                }
                _ => {}
            },
            Some(FocusTarget::ConversionInput) => {
                if self.state.layout.conversion.handle_input_key(key) {
                    self.state.should_quit = true;
                    return;
                }
                if matches!(key.code, KeyCode::Up | KeyCode::Char('k'))
                    && key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    self.state.focus.set(FocusTarget::ConversionScroll);
                    self.sync_focus();
                }
            }
            Some(FocusTarget::BulletinBoard) => match key.code {
                KeyCode::Up | KeyCode::Char('k') => self.state.layout.bulletin_board.scroll_by(-2),
                KeyCode::Down | KeyCode::Char('j') => self.state.layout.bulletin_board.scroll_by(2),
                KeyCode::PageUp => self.state.layout.bulletin_board.page_up(),
                KeyCode::PageDown | KeyCode::Char(' ') => self.state.layout.bulletin_board.page_down(),
                KeyCode::Char('g') => self.state.layout.bulletin_board.scroll_to(0),
                KeyCode::Char('G') => self.state.layout.bulletin_board.scroll_to_max(),
                _ => {}
            },
            None => {}
        }
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        self.state.copied_notice = false;

        if self.state.native_selection_mode {
            return;
        }
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                if self.state.layout.bulletin_board.contains_panel(mouse.column, mouse.row) {
                    self.state.layout.bulletin_board.scroll_by(-2);
                    self.state.focus.set(FocusTarget::BulletinBoard);
                } else if self.state.layout.conversion.contains_content(mouse.column, mouse.row) {
                    self.state.layout.conversion.scroll_by(-3);
                    self.state.focus.set(FocusTarget::ConversionScroll);
                }
                self.sync_focus();
            }
            MouseEventKind::ScrollDown => {
                if self.state.layout.bulletin_board.contains_panel(mouse.column, mouse.row) {
                    self.state.layout.bulletin_board.scroll_by(2);
                    self.state.focus.set(FocusTarget::BulletinBoard);
                } else if self.state.layout.conversion.contains_content(mouse.column, mouse.row) {
                    self.state.layout.conversion.scroll_by(3);
                    self.state.focus.set(FocusTarget::ConversionScroll);
                }
                self.sync_focus();
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if self
                    .state
                    .layout
                    .conversion
                    .scroll
                    .scrollbar
                    .contains(mouse.column, mouse.row)
                {
                    self.drag = Some(DragState {
                        mode: DragMode::Scroll(ScrollTarget::Conversion),
                        anchor_row: mouse.row,
                        anchor_scroll: self.state.layout.conversion.scroll.scroll,
                    });
                    self.state.focus.set(FocusTarget::ConversionScroll);
                } else if self
                    .state
                    .layout
                    .bulletin_board
                    .contains_scrollbar(mouse.column, mouse.row)
                {
                    self.drag = Some(DragState {
                        mode: DragMode::Scroll(ScrollTarget::BulletinBoard),
                        anchor_row: mouse.row,
                        anchor_scroll: self.state.layout.bulletin_board.scroll.scroll,
                    });
                    self.state.focus.set(FocusTarget::BulletinBoard);
                } else if self
                    .state
                    .layout
                    .conversion
                    .toggle_thought_at(mouse.column, mouse.row)
                {
                    self.drag = None;
                    self.state.focus.set(FocusTarget::ConversionScroll);
                } else if self
                    .state
                    .layout
                    .conversion
                    .start_selection(mouse.column, mouse.row)
                {
                    self.state.layout.bulletin_board.clear_selection();
                    self.drag = Some(DragState {
                        mode: DragMode::ConversionSelection,
                        anchor_row: mouse.row,
                        anchor_scroll: self.state.layout.conversion.scroll.scroll,
                    });
                    self.state.focus.set(FocusTarget::ConversionScroll);
                } else if self
                    .state
                    .layout
                    .bulletin_board
                    .start_selection(mouse.column, mouse.row)
                {
                    self.state.layout.conversion.clear_selection();
                    self.drag = Some(DragState {
                        mode: DragMode::BulletinBoardSelection,
                        anchor_row: mouse.row,
                        anchor_scroll: self.state.layout.bulletin_board.scroll.scroll,
                    });
                    self.state.focus.set(FocusTarget::BulletinBoard);
                } else if self
                    .state
                    .layout
                    .conversion
                    .select_block_at(mouse.column, mouse.row)
                {
                    self.drag = None;
                    self.state.focus.set(FocusTarget::ConversionScroll);
                } else if self
                    .state
                    .layout
                    .conversion
                    .handle_click(mouse.column, mouse.row)
                {
                    self.drag = None;
                    self.state.focus.set(FocusTarget::ConversionInput);
                }
                self.sync_focus();
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(drag) = self.drag {
                    match drag.mode {
                        DragMode::Scroll(target) => {
                            let delta = mouse.row as isize - drag.anchor_row as isize;
                            match target {
                                ScrollTarget::Conversion => self
                                    .state
                                    .layout
                                    .conversion
                                    .drag_scroll(drag.anchor_scroll, delta),
                                ScrollTarget::BulletinBoard => self
                                    .state
                                    .layout
                                    .bulletin_board
                                    .drag_scroll(drag.anchor_scroll, delta),
                            }
                        }
                        DragMode::ConversionSelection => {
                            self.state
                                .layout
                                .conversion
                                .update_selection(mouse.column, mouse.row);
                        }
                        DragMode::BulletinBoardSelection => {
                            self.state
                                .layout
                                .bulletin_board
                                .update_selection(mouse.column, mouse.row);
                        }
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if matches!(
                    self.drag.map(|drag| drag.mode),
                    Some(DragMode::ConversionSelection | DragMode::BulletinBoardSelection)
                ) {
                    let _ = self.copy_active_selection();
                }
                self.drag = None;
            }
            _ => {}
        }
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        let theme = Theme::default();
        self.sync_focus();
        self.state.cursor = None;
        frame.render_widget(Clear, frame.area());
        frame.render_widget(
            Block::default().style(Style::default().bg(theme.bg)),
            frame.area(),
        );

        let root = frame.area().inner(Margin {
            vertical: 0,
            horizontal: 1,
        });
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(10), Constraint::Length(1)])
            .split(root);

        self.layout = LayoutSnapshot {
            frame: frame.area(),
            content: root,
            top_bar: rows[0],
            left_panel: Rect::default(),
            right_panel: Rect::default(),
        };

        self.state.header.turn_count = self.state.layout.conversion.turns.len();
        self.state.header.dev_mode = self.state.dev_mode;
        self.state.header.native_selection_mode = self.state.native_selection_mode;
        self.state.header.copied = self.state.copied_notice;
        self.state.header.footer_text = self
            .state
            .layout
            .bulletin_board
            .details
            .footer
            .clone();

        header::render(frame, rows[0], &self.state.header, &theme);
        context::render(frame, rows[1], &mut self.state.layout, &theme);
        footer::render(frame, rows[2], &self.state.header, &theme);
        self.layout.left_panel = self.state.layout.left_area;
        self.layout.right_panel = self.state.layout.right_area;
        self.state.cursor = self.state.layout.conversion.cursor;

        if self.state.dev_mode {
            self.draw_dev_overlay(frame, &theme);
        }
    }

    fn draw_dev_overlay(&self, frame: &mut Frame, theme: &Theme) {
        let area = Rect::new(
            self.layout.frame.x + self.layout.frame.width.saturating_sub(38),
            self.layout.frame.y + 1,
            36,
            11,
        );
        let panel = Paragraph::new(vec![
            Line::styled(
                "DEV MODE",
                Style::default().fg(theme.dev).add_modifier(Modifier::BOLD),
            ),
            Line::raw(""),
            dev_line("frame", rect_value(self.layout.frame), theme),
            dev_line("content", rect_value(self.layout.content), theme),
            dev_line("topbar", rect_value(self.layout.top_bar), theme),
            dev_line("left", rect_value(self.layout.left_panel), theme),
            dev_line("right", rect_value(self.layout.right_panel), theme),
            dev_line(
                "l.scroll",
                format!(
                    "{}/{}",
                    self.state.layout.conversion.scroll.scroll,
                    self.state.layout.conversion.scroll.max_scroll
                ),
                theme,
            ),
            dev_line(
                "r.scroll",
                format!(
                    "{}/{}",
                    self.state.layout.bulletin_board.scroll.scroll,
                    self.state.layout.bulletin_board.scroll.max_scroll
                ),
                theme,
            ),
            dev_line(
                "l.view",
                format!(
                    "{}x{}",
                    self.state.layout.conversion.scroll.wrap_width,
                    self.state.layout.conversion.scroll.viewport_height
                ),
                theme,
            ),
            dev_line(
                "r.view",
                format!(
                    "{}x{}",
                    self.state.layout.bulletin_board.scroll.wrap_width,
                    self.state.layout.bulletin_board.scroll.viewport_height
                ),
                theme,
            ),
            dev_line(
                "thoughts",
                self.state.layout.conversion.thought_hitboxes.len().to_string(),
                theme,
            ),
        ])
        .block(Block::default().style(Style::default().bg(theme.overlay)));
        frame.render_widget(Clear, area);
        frame.render_widget(panel, area);
    }
}

fn dev_line(label: &str, value: String, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        ratatui::text::Span::styled(format!("{label:<8}"), Style::default().fg(theme.muted)),
        ratatui::text::Span::styled(value, Style::default().fg(theme.text)),
    ])
}

fn rect_value(rect: Rect) -> String {
    format!("x{} y{} w{} h{}", rect.x, rect.y, rect.width, rect.height)
}
