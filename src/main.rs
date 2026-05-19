use std::{env, io, mem, time::Duration};

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph, Wrap},
};
use serde::Deserialize;
use unicode_width::UnicodeWidthStr;

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let terminal = ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(stdout))?;
    let result = run(terminal);
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
    result
}

fn run(mut terminal: DefaultTerminal) -> io::Result<()> {
    let mut app = App::new();
    loop {
        terminal.draw(|frame| draw(frame, &mut app))?;
        if let Some(cursor) = app.cursor {
            terminal.show_cursor()?;
            terminal.set_cursor_position(cursor)?;
        }
        if !event::poll(Duration::from_millis(50))? {
            continue;
        }

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => handle_key(&mut app, key),
            Event::Mouse(mouse) => handle_mouse(&mut app, mouse),
            _ => {}
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn handle_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.dev_mode = !app.dev_mode
        }
        KeyCode::F(2) => app.dev_mode = !app.dev_mode,
        KeyCode::Up | KeyCode::Char('k') => app.scroll_left_by(-3),
        KeyCode::Down | KeyCode::Char('j') => app.scroll_left_by(3),
        KeyCode::PageUp => app.scroll_left_by(-(app.left.viewport_height as isize - 2)),
        KeyCode::PageDown | KeyCode::Char(' ') => {
            app.scroll_left_by(app.left.viewport_height as isize - 2)
        }
        KeyCode::Char('g') => app.scroll_left_to(0),
        KeyCode::Char('G') => app.scroll_left_to_max(),
        KeyCode::Backspace => {
            app.input.pop();
        }
        KeyCode::Enter => app.submit_input(),
        KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.input.push(ch);
        }
        _ => {}
    }
}

fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            if in_rect(mouse.column, mouse.row, app.right.scrollbar.hit_area) {
                app.scroll_right_by(-2);
            } else {
                app.scroll_left_by(-3);
            }
        }
        MouseEventKind::ScrollDown => {
            if in_rect(mouse.column, mouse.row, app.right.scrollbar.hit_area) {
                app.scroll_right_by(2);
            } else {
                app.scroll_left_by(3);
            }
        }
        MouseEventKind::Down(MouseButton::Left) => {
            if app.left.scrollbar.contains(mouse.column, mouse.row) {
                app.drag = Some(DragState {
                    target: ScrollTarget::Left,
                    anchor_row: mouse.row,
                    anchor_scroll: app.left.scroll,
                });
            } else if app.right.scrollbar.contains(mouse.column, mouse.row) {
                app.drag = Some(DragState {
                    target: ScrollTarget::Right,
                    anchor_row: mouse.row,
                    anchor_scroll: app.right.scroll,
                });
            } else if app.toggle_thought_at(mouse.column, mouse.row) {
                app.drag = None;
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(drag) = app.drag {
                let delta = mouse.row as isize - drag.anchor_row as isize;
                match drag.target {
                    ScrollTarget::Left => app.drag_scroll_left(drag.anchor_scroll, delta),
                    ScrollTarget::Right => app.drag_scroll_right(drag.anchor_scroll, delta),
                }
            }
        }
        MouseEventKind::Up(MouseButton::Left) => app.drag = None,
        _ => {}
    }
}

fn draw(frame: &mut Frame, app: &mut App) {
    let theme = Theme::default();
    app.cursor = None;
    frame.render_widget(Clear, frame.area());
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.bg)),
        frame.area(),
    );

    let root = frame.area().inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(12)])
        .spacing(1)
        .split(root);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(if root.width >= 150 {
            vec![Constraint::Min(84), Constraint::Length(58)]
        } else if root.width >= 120 {
            vec![Constraint::Min(68), Constraint::Length(46)]
        } else {
            vec![Constraint::Min(44), Constraint::Length(34)]
        })
        .spacing(1)
        .split(rows[1]);

    app.layout = LayoutSnapshot {
        frame: frame.area(),
        content: root,
        top_bar: rows[0],
        left_panel: body[0],
        right_panel: body[1],
    };

    draw_top_bar(frame, rows[0], app, &theme);
    draw_left_panel(frame, body[0], app, &theme);
    draw_right_panel(frame, body[1], app, &theme);

    if app.dev_mode {
        draw_dev_overlay(frame, app, &theme);
    }
}

fn draw_top_bar(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
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
            format!(" flyflor · ready · {} turns", app.turns.len()),
            Style::default().fg(theme.text),
        ),
        if app.dev_mode {
            Span::styled(" · DEV", Style::default().fg(theme.dev))
        } else {
            Span::raw("")
        },
    ]);

    frame.render_widget(Paragraph::new(left), cols[0]);
    frame.render_widget(Paragraph::new(right).alignment(Alignment::Right), cols[1]);
}

fn draw_left_panel(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(8),
            Constraint::Length(4),
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

    let doc_area = rows[1];
    let text_area = Rect::new(
        doc_area.x,
        doc_area.y,
        doc_area.width.saturating_sub(2),
        doc_area.height,
    );
    app.update_left_viewport(text_area, theme);

    let paragraph = Paragraph::new(app.chat_lines.clone()).scroll((app.left.scroll as u16, 0));
    frame.render_widget(paragraph, text_area);
    draw_scrollbar(frame, app.left.scrollbar, theme);

    draw_separator(
        frame,
        Rect::new(rows[2].x, rows[2].y, rows[2].width, 1),
        theme,
    );
    let input_inner = Rect::new(
        rows[2].x,
        rows[2].y + 1,
        rows[2].width,
        rows[2].height.saturating_sub(1),
    );
    if input_inner.height > 0 {
        let input_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(input_inner);

        let input_text = if app.input.is_empty() {
            "ask anything...".to_string()
        } else {
            app.input.clone()
        };
        let prompt = Line::from(vec![
            Span::styled(
                pad_to_width(&input_text, input_rows[0].width.saturating_sub(2) as usize),
                if app.input.is_empty() {
                    Style::default().fg(theme.muted)
                } else {
                    Style::default().fg(theme.text)
                },
            ),
            Span::styled(" >", Style::default().fg(theme.purple).add_modifier(Modifier::BOLD)),
        ]);
        frame.render_widget(Paragraph::new(prompt), input_rows[0]);
        let cursor_x = input_rows[0].x
            + if app.input.is_empty() {
                0
            } else {
                UnicodeWidthStr::width(app.input.as_str()) as u16
            };
        app.cursor = Some(Position::new(cursor_x, input_rows[0].y));

        let help = if input_rows[2].width >= 72 {
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(theme.text)),
                Span::styled(" 发送", Style::default().fg(theme.muted)),
                Span::styled("  |  ↑/↓", Style::default().fg(theme.text)),
                Span::styled(" 历史", Style::default().fg(theme.muted)),
                Span::styled("  |  Click", Style::default().fg(theme.text)),
                Span::styled(" 展开 Thought", Style::default().fg(theme.muted)),
                Span::styled("  |  Ctrl+D", Style::default().fg(theme.text)),
                Span::styled(" DEV", Style::default().fg(theme.muted)),
            ])
        } else {
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(theme.text)),
                Span::styled(" send", Style::default().fg(theme.muted)),
                Span::styled("  |  Click Thought", Style::default().fg(theme.muted)),
            ])
        };
        frame.render_widget(Paragraph::new(help), input_rows[2]);
    }
}

fn draw_right_panel(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 2,
    });
    if inner.width < 24 || inner.height < 14 {
        draw_compact_sidebar(frame, inner, theme);
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(16),
            Constraint::Length(1),
            Constraint::Min(6),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Line::styled("TODO List", Style::default().fg(theme.text))),
        Rect::new(rows[0].x, rows[0].y, rows[0].width, 1),
    );
    let todo_start_y = rows[0].y + 2;
    for (index, item) in app.todos.iter().enumerate() {
        let y = todo_start_y + (index as u16 * 2);
        if y >= rows[0].bottom() {
            break;
        }
        draw_todo_row(
            frame,
            Rect::new(rows[0].x, y, rows[0].width, 1),
            &item.marker,
            &item.label,
            &item.status,
            item.active,
            theme,
        );
    }

    draw_separator(frame, rows[1], theme);

    let scroll_host = rows[2];
    let scroll_text = Rect::new(
        scroll_host.x,
        scroll_host.y,
        scroll_host.width.saturating_sub(2),
        scroll_host.height,
    );
    app.update_right_viewport(scroll_text);
    let content = Paragraph::new(app.right_lines.clone())
        .style(Style::default().fg(theme.text))
        .wrap(Wrap { trim: false })
        .scroll((app.right.scroll as u16, 0));
    frame.render_widget(content, scroll_text);
    draw_scrollbar(frame, app.right.scrollbar, theme);
}

fn draw_compact_sidebar(frame: &mut Frame, area: Rect, theme: &Theme) {
    let compact = Paragraph::new(vec![
        Line::styled("TODO List", Style::default().fg(theme.text)),
        Line::styled("○ 明确需求边界与冲突", Style::default().fg(theme.text)),
        Line::styled("› 设计协议核心架构", Style::default().fg(theme.pink)),
        Line::raw(""),
        Line::styled("MODEL", Style::default().fg(theme.blue)),
        metric_line("model", "flyflor-pro", theme),
        metric_line("provider", "OpenTUI", theme),
        Line::raw(""),
        Line::styled("● healthy", Style::default().fg(theme.green)),
    ]);
    frame.render_widget(compact, area);
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
        .constraints([Constraint::Min(10), Constraint::Length(7)])
        .split(area);

    let accent = if active { theme.pink } else { theme.text };
    let left = Line::from(vec![
        Span::styled(format!("{marker} "), Style::default().fg(accent)),
        Span::styled(label.to_string(), Style::default().fg(accent)),
    ]);
    frame.render_widget(Paragraph::new(left), cols[0]);

    let badge = Paragraph::new(Line::styled(
        center_text(status, cols[1].width as usize),
        Style::default()
            .fg(if active { theme.blue } else { theme.muted })
            .bg(if active {
                theme.status_active_bg
            } else {
                theme.status_idle_bg
            }),
    ));
    frame.render_widget(badge, cols[1]);
}

fn draw_scrollbar(frame: &mut Frame, scrollbar: ScrollbarGeometry, theme: &Theme) {
    if scrollbar.track_height == 0 {
        return;
    }
    for offset in 0..scrollbar.track_height {
        let y = scrollbar.track_top + offset;
        let symbol = if y == scrollbar.thumb_top {
            "●"
        } else {
            "○"
        };
        let color = if symbol == "●" {
            theme.scroll_thumb
        } else {
            theme.scroll_track
        };
        frame.render_widget(
            Paragraph::new(Line::styled(symbol, Style::default().fg(color))),
            Rect::new(scrollbar.x, y, 1, 1),
        );
    }
}

fn draw_dev_overlay(frame: &mut Frame, app: &App, theme: &Theme) {
    let area = Rect::new(
        app.layout.frame.x + app.layout.frame.width.saturating_sub(38),
        app.layout.frame.y + 1,
        36,
        11,
    );
    let panel = Paragraph::new(vec![
        Line::styled(
            "DEV MODE",
            Style::default().fg(theme.dev).add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
        dev_line("frame", rect_value(app.layout.frame), theme),
        dev_line("content", rect_value(app.layout.content), theme),
        dev_line("topbar", rect_value(app.layout.top_bar), theme),
        dev_line("left", rect_value(app.layout.left_panel), theme),
        dev_line("right", rect_value(app.layout.right_panel), theme),
        dev_line(
            "l.scroll",
            format!("{}/{}", app.left.scroll, app.left.max_scroll),
            theme,
        ),
        dev_line(
            "r.scroll",
            format!("{}/{}", app.right.scroll, app.right.max_scroll),
            theme,
        ),
        dev_line(
            "l.view",
            format!("{}x{}", app.left.wrap_width, app.left.viewport_height),
            theme,
        ),
        dev_line(
            "r.view",
            format!("{}x{}", app.right.wrap_width, app.right.viewport_height),
            theme,
        ),
        dev_line("thoughts", app.thought_hitboxes.len().to_string(), theme),
    ])
    .block(Block::default().style(Style::default().bg(theme.overlay)));
    frame.render_widget(Clear, area);
    frame.render_widget(panel, area);
}

fn draw_separator(frame: &mut Frame, area: Rect, theme: &Theme) {
    frame.render_widget(
        Paragraph::new("─".repeat(area.width as usize)).style(Style::default().fg(theme.dim)),
        area,
    );
}

fn metric_line<'a>(key: &'a str, value: &'a str, theme: &Theme) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{key:<12}"), Style::default().fg(theme.muted)),
        Span::styled(value.to_string(), Style::default().fg(theme.text)),
    ])
}

fn dev_line(label: &str, value: String, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<8}"), Style::default().fg(theme.muted)),
        Span::styled(value, Style::default().fg(theme.text)),
    ])
}

fn rect_value(rect: Rect) -> String {
    format!("x{} y{} w{} h{}", rect.x, rect.y, rect.width, rect.height)
}

fn center_text(text: &str, width: usize) -> String {
    let text_width = UnicodeWidthStr::width(text);
    if width <= text_width {
        return text.to_string();
    }
    let left = (width - text_width) / 2;
    let right = width - text_width - left;
    format!("{}{}{}", " ".repeat(left), text, " ".repeat(right))
}

fn in_rect(x: u16, y: u16, rect: Rect) -> bool {
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}

#[derive(Clone, Copy, Default)]
struct ScrollbarGeometry {
    x: u16,
    track_top: u16,
    track_height: u16,
    thumb_top: u16,
    thumb_height: u16,
    hit_area: Rect,
}

impl ScrollbarGeometry {
    fn contains(&self, x: u16, y: u16) -> bool {
        in_rect(x, y, self.hit_area)
    }
}

#[derive(Clone, Copy)]
enum ScrollTarget {
    Left,
    Right,
}

#[derive(Clone, Copy)]
struct DragState {
    target: ScrollTarget,
    anchor_row: u16,
    anchor_scroll: usize,
}

#[derive(Default)]
struct ScrollState {
    scroll: usize,
    viewport_height: usize,
    wrap_width: usize,
    total_visual_lines: usize,
    max_scroll: usize,
    initial_scroll_applied: bool,
    stick_to_bottom: bool,
    scrollbar: ScrollbarGeometry,
}

#[derive(Clone, Copy)]
struct ThoughtRegion {
    turn_index: usize,
    line_index: usize,
}

struct LeftRender {
    lines: Vec<Line<'static>>,
    thought_regions: Vec<ThoughtRegion>,
}

#[derive(Clone, Copy)]
struct ThoughtHitbox {
    turn_index: usize,
    rect: Rect,
}

struct App {
    turns: Vec<Turn>,
    chat_lines: Vec<Line<'static>>,
    thought_hitboxes: Vec<ThoughtHitbox>,
    right_lines: Vec<Line<'static>>,
    right_source: RightPanelData,
    todos: Vec<TodoItem>,
    left: ScrollState,
    right: ScrollState,
    dev_mode: bool,
    should_quit: bool,
    input: String,
    cursor: Option<Position>,
    drag: Option<DragState>,
    layout: LayoutSnapshot,
}

impl App {
    fn new() -> Self {
        let mock = load_mock_data();
        Self {
            turns: mock.turns,
            chat_lines: Vec::new(),
            thought_hitboxes: Vec::new(),
            right_lines: Vec::new(),
            right_source: mock.right_panel,
            todos: mock.todos,
            left: ScrollState::default(),
            right: ScrollState::default(),
            dev_mode: dev_mode_enabled(),
            should_quit: false,
            input: String::new(),
            cursor: None,
            drag: None,
            layout: LayoutSnapshot::default(),
        }
    }

    fn update_left_viewport(&mut self, area: Rect, theme: &Theme) {
        let render = render_turns(&self.turns, area.width.max(1) as usize, theme);
        self.chat_lines = render.lines;
        update_scroll_state_from_rendered(&self.chat_lines, &mut self.left, area);
        self.thought_hitboxes = render
            .thought_regions
            .into_iter()
            .filter_map(|region| {
                let visible_index = region.line_index.checked_sub(self.left.scroll)?;
                if visible_index >= area.height as usize {
                    return None;
                }
                Some(ThoughtHitbox {
                    turn_index: region.turn_index,
                    rect: Rect::new(area.x, area.y + visible_index as u16, area.width, 1),
                })
            })
            .collect();
    }

    fn update_right_viewport(&mut self, area: Rect) {
        self.right_lines = render_right_panel_lines(&self.right_source, area.width.max(1) as usize);
        update_scroll_state(&self.right_lines, &mut self.right, area);
    }

    fn scroll_left_by(&mut self, delta: isize) {
        apply_scroll_delta(&mut self.left, delta);
    }

    fn scroll_right_by(&mut self, delta: isize) {
        apply_scroll_delta(&mut self.right, delta);
    }

    fn scroll_left_to(&mut self, value: usize) {
        self.left.scroll = value.min(self.left.max_scroll);
        self.left.stick_to_bottom = self.left.scroll >= self.left.max_scroll;
    }

    fn scroll_left_to_max(&mut self) {
        self.left.scroll = self.left.max_scroll;
        self.left.stick_to_bottom = true;
    }

    fn drag_scroll_left(&mut self, anchor_scroll: usize, delta_rows: isize) {
        drag_scroll(&mut self.left, anchor_scroll, delta_rows);
    }

    fn drag_scroll_right(&mut self, anchor_scroll: usize, delta_rows: isize) {
        drag_scroll(&mut self.right, anchor_scroll, delta_rows);
    }

    fn toggle_thought_at(&mut self, x: u16, y: u16) -> bool {
        let Some(hit) = self
            .thought_hitboxes
            .iter()
            .find(|hitbox| in_rect(x, y, hitbox.rect))
            .copied()
        else {
            return false;
        };
        if let Some(thought) = self
            .turns
            .get_mut(hit.turn_index)
            .and_then(|turn| turn.thought.as_mut())
        {
            thought.expanded = !thought.expanded;
            self.left.stick_to_bottom = false;
            return true;
        }
        false
    }

    fn submit_input(&mut self) {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return;
        }
        if text == "/exit" {
            self.should_quit = true;
            return;
        }

        let turn_index = self.turns.len() + 1;
        self.turns.push(build_runtime_turn(turn_index, text));
        self.input.clear();
        self.left.stick_to_bottom = true;
    }
}

fn update_scroll_state_from_rendered(lines: &[Line<'_>], state: &mut ScrollState, area: Rect) {
    let previous_max = state.max_scroll;
    let offset_from_bottom = previous_max.saturating_sub(state.scroll);
    state.wrap_width = area.width.max(1) as usize;
    state.viewport_height = area.height.max(1) as usize;
    state.total_visual_lines = lines.len().max(1);
    state.max_scroll = state
        .total_visual_lines
        .saturating_sub(state.viewport_height.max(1));

    if !state.initial_scroll_applied {
        state.scroll = if state.stick_to_bottom {
            state.max_scroll
        } else {
            0
        };
        state.initial_scroll_applied = true;
    } else if state.stick_to_bottom {
        state.scroll = state.max_scroll;
    } else {
        state.scroll = state.max_scroll.saturating_sub(offset_from_bottom);
    }

    state.scrollbar = compute_scrollbar(area, state.scroll, state.max_scroll);
}

fn update_scroll_state(lines: &[Line<'_>], state: &mut ScrollState, area: Rect) {
    let previous_max = state.max_scroll;
    let offset_from_bottom = previous_max.saturating_sub(state.scroll);
    state.wrap_width = area.width.max(1) as usize;
    state.viewport_height = area.height.max(1) as usize;
    state.total_visual_lines = count_visual_lines(lines, state.wrap_width);
    state.max_scroll = state
        .total_visual_lines
        .saturating_sub(state.viewport_height.max(1));

    if !state.initial_scroll_applied {
        state.scroll = state.max_scroll;
        state.initial_scroll_applied = true;
    } else if state.stick_to_bottom {
        state.scroll = state.max_scroll;
    } else {
        state.scroll = state.max_scroll.saturating_sub(offset_from_bottom);
    }

    state.scrollbar = compute_scrollbar(area, state.scroll, state.max_scroll);
}

fn apply_scroll_delta(state: &mut ScrollState, delta: isize) {
    let next = if delta.is_negative() {
        state.scroll.saturating_sub(delta.unsigned_abs())
    } else {
        (state.scroll + delta as usize).min(state.max_scroll)
    };
    state.scroll = next;
    state.stick_to_bottom = state.scroll >= state.max_scroll;
}

fn drag_scroll(state: &mut ScrollState, anchor_scroll: usize, delta_rows: isize) {
    let travel = state
        .scrollbar
        .track_height
        .saturating_sub(state.scrollbar.thumb_height);
    if travel == 0 || state.max_scroll == 0 {
        state.scroll = 0;
        return;
    }

    let delta_scroll =
        ((delta_rows as f32 / travel as f32) * state.max_scroll as f32).round() as isize;
    let next = if delta_scroll.is_negative() {
        anchor_scroll.saturating_sub(delta_scroll.unsigned_abs())
    } else {
        (anchor_scroll + delta_scroll as usize).min(state.max_scroll)
    };
    state.scroll = next;
    state.stick_to_bottom = state.scroll >= state.max_scroll;
}

fn compute_scrollbar(area: Rect, scroll: usize, max_scroll: usize) -> ScrollbarGeometry {
    let track_height = area.height;
    let thumb_height = 1;
    let travel = track_height.saturating_sub(thumb_height);
    let thumb_top = if max_scroll == 0 || travel == 0 {
        area.y
    } else {
        area.y + ((scroll as f32 / max_scroll as f32) * travel as f32).round() as u16
    };
    let x = area.x + area.width.saturating_sub(2);
    ScrollbarGeometry {
        x,
        track_top: area.y,
        track_height,
        thumb_top,
        thumb_height,
        hit_area: Rect::new(x, area.y, 2, area.height),
    }
}

fn count_visual_lines(lines: &[Line<'_>], width: usize) -> usize {
    lines
        .iter()
        .map(|line| wrapped_line_count(line, width))
        .sum::<usize>()
        .max(1)
}

fn wrapped_line_count(line: &Line<'_>, width: usize) -> usize {
    if width == 0 {
        return 1;
    }
    let line_width = line
        .spans
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum::<usize>();
    if line_width == 0 {
        1
    } else {
        line_width.div_ceil(width)
    }
}

fn render_turns(turns: &[Turn], width: usize, theme: &Theme) -> LeftRender {
    let mut lines = Vec::new();
    let mut thought_regions = Vec::new();

    for (turn_index, turn) in turns.iter().enumerate() {
        if turn_index > 0 {
            lines.push(empty_content_line(width, theme));
        }

        lines.extend(render_user_block(&turn.user, width, theme));
        lines.push(thread_line(Line::raw(""), width, theme, ThreadTone::Rail));

        if let Some(thought) = &turn.thought {
            let line_index = lines.len();
            lines.push(render_thought_header(thought, width, theme));
            thought_regions.push(ThoughtRegion {
                turn_index,
                line_index,
            });
            if thought.expanded {
                lines.push(thread_line(Line::raw(""), width, theme, ThreadTone::Rail));
                for line in render_markdown_block(
                    &thought.content,
                    width.saturating_sub(theme.thread_gutter),
                    theme,
                    MarkdownTone::Thought,
                ) {
                    lines.push(thread_line(line, width, theme, ThreadTone::Thought));
                }
            }
        }

        lines.push(thread_line(Line::raw(""), width, theme, ThreadTone::Rail));
        for line in render_markdown_block(
            &turn.answer,
            width.saturating_sub(theme.thread_gutter),
            theme,
            MarkdownTone::Answer,
        ) {
            lines.push(thread_line(line, width, theme, ThreadTone::Rail));
        }
        if !turn.footer.trim().is_empty() {
            lines.push(render_footer_line(&turn.footer, width, theme));
        }
        lines.push(thread_line(Line::raw(""), width, theme, ThreadTone::Rail));
    }

    LeftRender {
        lines,
        thought_regions,
    }
}

fn render_user_block(text: &str, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let content_width = width.saturating_sub(theme.thread_gutter);
    let mut lines = Vec::new();
    for row in wrap_plain_text(text, content_width.saturating_sub(theme.user_pad * 2)) {
        let content = format!(
            "{}{}{}",
            " ".repeat(theme.user_pad),
            row,
            " ".repeat(theme.user_pad)
        );
        let padded = pad_to_width(&content, content_width);
        lines.push(Line::from(vec![
            Span::styled(
                format!(
                    "{}{}",
                    theme.thread_bar_char,
                    " ".repeat(theme.thread_gutter.saturating_sub(1))
                ),
                Style::default().fg(theme.thread_accent),
            ),
            Span::styled(padded, Style::default().bg(theme.user_bg).fg(theme.text)),
        ]));
    }
    lines
}

fn render_thought_header(thought: &ThoughtData, width: usize, theme: &Theme) -> Line<'static> {
    let marker = if thought.expanded { "▼" } else { "▶" };
    let mut summary = thought.summary.clone();
    if summary.trim().is_empty() {
        summary = match thought.duration_ms {
            Some(duration) => format!("Thought for {duration}ms"),
            None => "Thought".to_string(),
        };
    }
    let body_width = width.saturating_sub(theme.thread_gutter);
    let label = truncate_to_width(&format!("{marker}  {summary}"), body_width);
    thread_line(
        Line::styled(
            pad_to_width(&label, body_width),
            Style::default()
                .fg(theme.thought_text)
                .add_modifier(Modifier::BOLD),
        ),
        width,
        theme,
        ThreadTone::Thought,
    )
}

fn thread_line(
    line: Line<'static>,
    width: usize,
    theme: &Theme,
    tone: ThreadTone,
) -> Line<'static> {
    let body_width = width.saturating_sub(theme.thread_gutter);
    let mut spans = vec![Span::styled(
        format!(
            "{}{}",
            match tone {
                ThreadTone::Rail => theme.rail_char,
                ThreadTone::Thought => theme.thought_bar_char,
            },
            " ".repeat(theme.thread_gutter.saturating_sub(1))
        ),
        Style::default().fg(match tone {
            ThreadTone::Rail => theme.rail,
            ThreadTone::Thought => theme.thought_bar,
        }),
    )];
    if line.spans.is_empty() {
        spans.push(Span::raw(" ".repeat(body_width)));
        return Line::from(spans);
    }
    let current_width = line
        .spans
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum::<usize>();
    spans.extend(line.spans);
    if current_width < body_width {
        spans.push(Span::raw(" ".repeat(body_width - current_width)));
    }
    Line::from(spans)
}

fn empty_content_line(width: usize, theme: &Theme) -> Line<'static> {
    thread_line(Line::raw(""), width, theme, ThreadTone::Rail)
}

fn render_footer_line(footer: &str, width: usize, theme: &Theme) -> Line<'static> {
    let body_width = width.saturating_sub(theme.thread_gutter);
    let label = truncate_to_width(footer, body_width.saturating_sub(4));
    let mut spans = vec![
        Span::styled(
            format!(
                "{}{}",
                theme.rail_char,
                " ".repeat(theme.thread_gutter.saturating_sub(1))
            ),
            Style::default().fg(theme.rail),
        ),
        Span::styled(theme.footer_icon.to_string(), Style::default().fg(theme.footer_icon_color)),
        Span::raw(" "),
    ];
    let parts: Vec<&str> = label.split(" · ").collect();
    for (index, part) in parts.iter().enumerate() {
        spans.push(Span::styled(
            part.to_string(),
            Style::default().fg(if index == 0 { theme.footer_primary } else { theme.footer_muted }),
        ));
        if index + 1 < parts.len() {
            spans.push(Span::styled(" · ", Style::default().fg(theme.footer_muted)));
        }
    }
    let used = spans
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum::<usize>()
        .saturating_sub(theme.thread_gutter);
    if used < body_width {
        spans.push(Span::raw(" ".repeat(body_width - used)));
    }
    Line::from(spans)
}

#[derive(Clone, Copy)]
enum MarkdownTone {
    Thought,
    Answer,
}

#[derive(Clone, Copy)]
enum ThreadTone {
    Rail,
    Thought,
}

fn render_markdown_block(
    text: &str,
    width: usize,
    theme: &Theme,
    tone: MarkdownTone,
) -> Vec<Line<'static>> {
    let content_width = width.max(1);
    let mut lines = Vec::new();
    let raw_lines: Vec<&str> = text.lines().collect();
    let mut index = 0;

    while index < raw_lines.len() {
        let line = raw_lines[index];
        let trimmed = line.trim_end();

        if trimmed.is_empty() {
            lines.push(Line::raw(""));
            index += 1;
            continue;
        }

        if let Some(language) = trimmed.strip_prefix("```") {
            let lang = language.trim().to_string();
            let mut code_lines = Vec::new();
            index += 1;
            while index < raw_lines.len() && !raw_lines[index].trim_start().starts_with("```") {
                code_lines.push(raw_lines[index].to_string());
                index += 1;
            }
            if index < raw_lines.len() {
                index += 1;
            }
            lines.extend(render_code_block(&lang, &code_lines, content_width, theme, tone));
            continue;
        }

        if is_table_start(&raw_lines, index) {
            let mut table_lines = Vec::new();
            while index < raw_lines.len() && raw_lines[index].contains('|') {
                table_lines.push(raw_lines[index].to_string());
                index += 1;
            }
            lines.extend(render_table_block(&table_lines, content_width, theme, tone));
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("# ") {
            lines.extend(wrap_inline_text(
                rest,
                "",
                "",
                content_width,
                heading_style(theme, tone, 1),
                theme,
            ));
            index += 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            lines.extend(wrap_inline_text(
                rest,
                "",
                "",
                content_width,
                heading_style(theme, tone, 2),
                theme,
            ));
            index += 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("### ") {
            lines.extend(wrap_inline_text(
                rest,
                "",
                "",
                content_width,
                heading_style(theme, tone, 3),
                theme,
            ));
            index += 1;
            continue;
        }
        if is_hr(trimmed) {
            lines.push(Line::styled(
                "─".repeat(content_width.min(32)),
                Style::default().fg(theme.dim),
            ));
            index += 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("> ") {
            lines.extend(wrap_inline_text(
                rest,
                "│ ",
                "│ ",
                content_width,
                quote_style(theme, tone),
                theme,
            ));
            index += 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("- ") {
            lines.extend(wrap_inline_text(
                rest,
                "• ",
                "  ",
                content_width,
                body_style(theme, tone),
                theme,
            ));
            index += 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("* ") {
            lines.extend(wrap_inline_text(
                rest,
                "• ",
                "  ",
                content_width,
                body_style(theme, tone),
                theme,
            ));
            index += 1;
            continue;
        }
        if let Some((prefix, rest)) = ordered_prefix(trimmed) {
            lines.extend(wrap_inline_text(
                rest,
                &prefix,
                &" ".repeat(prefix_width(&prefix)),
                content_width,
                body_style(theme, tone),
                theme,
            ));
            index += 1;
            continue;
        }

        let mut paragraph = vec![trimmed.to_string()];
        index += 1;
        while index < raw_lines.len() {
            let next = raw_lines[index].trim_end();
            if next.is_empty()
                || next.starts_with('#')
                || next.starts_with("> ")
                || next.starts_with("- ")
                || next.starts_with("* ")
                || next.starts_with("```")
                || ordered_prefix(next).is_some()
                || is_hr(next)
                || is_table_start(&raw_lines, index)
            {
                break;
            }
            paragraph.push(next.to_string());
            index += 1;
        }
        lines.extend(wrap_inline_text(
            &paragraph.join(" "),
            "",
            "",
            content_width,
            body_style(theme, tone),
            theme,
        ));
    }

    if lines.is_empty() {
        vec![Line::raw("")]
    } else {
        lines
    }
}

fn heading_style(theme: &Theme, tone: MarkdownTone, level: usize) -> Style {
    let color = match (tone, level) {
        (MarkdownTone::Answer, 1) => theme.text,
        (MarkdownTone::Answer, 2) => theme.blue,
        (MarkdownTone::Answer, _) => theme.purple,
        (MarkdownTone::Thought, 1) => theme.purple,
        (MarkdownTone::Thought, 2) => theme.blue,
        (MarkdownTone::Thought, _) => theme.muted,
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

fn body_style(theme: &Theme, tone: MarkdownTone) -> Style {
    match tone {
        MarkdownTone::Thought => Style::default().fg(theme.muted),
        MarkdownTone::Answer => Style::default().fg(theme.text),
    }
}

fn quote_style(theme: &Theme, tone: MarkdownTone) -> Style {
    match tone {
        MarkdownTone::Thought => Style::default().fg(theme.purple),
        MarkdownTone::Answer => Style::default().fg(theme.blue),
    }
}

fn code_style(theme: &Theme, tone: MarkdownTone) -> Style {
    match tone {
        MarkdownTone::Thought => Style::default().fg(theme.purple).bg(theme.code_bg),
        MarkdownTone::Answer => Style::default().fg(theme.text).bg(theme.code_bg),
    }
}

fn render_code_block(
    language: &str,
    code_lines: &[String],
    width: usize,
    theme: &Theme,
    tone: MarkdownTone,
) -> Vec<Line<'static>> {
    if language.eq_ignore_ascii_case("mermaid") {
        return render_mermaid_block(code_lines, width, theme, tone);
    }

    let mut lines = Vec::new();
    let label = if language.is_empty() {
        "Code".to_string()
    } else {
        format!("Code · {language}")
    };
    let header_style = Style::default()
        .fg(theme.code_label)
        .add_modifier(Modifier::BOLD);
    lines.push(Line::styled(
        truncate_to_width(&label, width),
        header_style,
    ));
    for row in code_lines {
        let styled = code_style(theme, tone);
        for wrapped in wrap_plain_text(row, width) {
            lines.push(Line::styled(pad_to_width(&wrapped, width), styled));
        }
    }
    lines
}

fn render_mermaid_block(
    code_lines: &[String],
    width: usize,
    theme: &Theme,
    tone: MarkdownTone,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::styled(
        truncate_to_width("Flowchart", width),
        Style::default()
            .fg(theme.mermaid_label)
            .add_modifier(Modifier::BOLD),
    ));

    let graph = parse_mermaid_graph(code_lines);
    if graph.edges.is_empty() {
        for row in code_lines {
            for wrapped in wrap_plain_text(row, width) {
                lines.push(Line::styled(
                    pad_to_width(&wrapped, width),
                    code_style(theme, tone),
                ));
            }
        }
        return lines;
    }

    for line in render_mermaid_ascii(&graph, width) {
        lines.push(Line::styled(
            pad_to_width(&line, width),
            Style::default().fg(theme.mermaid_text).bg(theme.code_bg),
        ));
    }
    lines
}

fn render_table_block(
    table_lines: &[String],
    width: usize,
    theme: &Theme,
    tone: MarkdownTone,
) -> Vec<Line<'static>> {
    if table_lines.len() < 2 {
        return table_lines
            .iter()
            .flat_map(|line| wrap_inline_text(line, "", "", width, body_style(theme, tone), theme))
            .collect();
    }

    let header = split_table_row(&table_lines[0]);
    let align = split_table_row(&table_lines[1]);
    if header.is_empty() || !is_alignment_row(&align) {
        return table_lines
            .iter()
            .flat_map(|line| wrap_inline_text(line, "", "", width, body_style(theme, tone), theme))
            .collect();
    }

    let mut rows = vec![header.clone()];
    for row in table_lines.iter().skip(2) {
        rows.push(split_table_row(row));
    }

    let col_count = rows.iter().map(Vec::len).max().unwrap_or(0);
    if col_count == 0 {
        return vec![Line::raw("")];
    }

    let mut widths = vec![3; col_count];
    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(string_width(cell));
        }
    }

    let separator_width = (col_count.saturating_sub(1)) * 3;
    while widths.iter().sum::<usize>() + separator_width > width {
        let Some((index, max_width)) = widths
            .iter()
            .enumerate()
            .max_by_key(|(_, value)| **value)
            .map(|(index, value)| (index, *value))
        else {
            break;
        };
        if max_width <= 4 {
            break;
        }
        widths[index] -= 1;
    }

    let mut lines = Vec::new();
    lines.push(render_table_row(
        &rows[0],
        &widths,
        Style::default()
            .fg(match tone {
                MarkdownTone::Answer => theme.text,
                MarkdownTone::Thought => theme.purple,
            })
            .add_modifier(Modifier::BOLD),
    ));
    lines.push(Line::styled(
        widths
            .iter()
            .enumerate()
            .map(|(index, col)| {
                if index + 1 == widths.len() {
                    "─".repeat(*col)
                } else {
                    format!("{}─┼─", "─".repeat(*col))
                }
            })
            .collect::<String>(),
        Style::default().fg(theme.dim),
    ));
    for row in rows.iter().skip(1) {
        lines.push(render_table_row(row, &widths, body_style(theme, tone)));
    }
    lines
}

fn render_table_row(cells: &[String], widths: &[usize], style: Style) -> Line<'static> {
    let mut output = String::new();
    for (index, width) in widths.iter().enumerate() {
        if index > 0 {
            output.push_str(" │ ");
        }
        let cell = cells.get(index).cloned().unwrap_or_default();
        output.push_str(&pad_to_width(&truncate_to_width(&cell, *width), *width));
    }
    Line::styled(output, style)
}

fn split_table_row(row: &str) -> Vec<String> {
    row.trim()
        .trim_matches('|')
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect()
}

fn is_alignment_row(cells: &[String]) -> bool {
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let trimmed = cell.trim_matches(':').trim();
            !trimmed.is_empty() && trimmed.chars().all(|ch| ch == '-')
        })
}

fn is_table_start(lines: &[&str], index: usize) -> bool {
    if index + 1 >= lines.len() {
        return false;
    }
    lines[index].contains('|') && is_alignment_row(&split_table_row(lines[index + 1]))
}

fn is_hr(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.len() >= 3 && trimmed.chars().all(|ch| ch == '-')
}

fn ordered_prefix(line: &str) -> Option<(String, &str)> {
    let mut digits = String::new();
    for (index, ch) in line.char_indices() {
        if ch.is_ascii_digit() {
            digits.push(ch);
            continue;
        }
        if ch == '.' && !digits.is_empty() {
            let rest = line.get(index + 1..)?.trim_start();
            return Some((format!("{digits}. "), rest));
        }
        break;
    }
    None
}

fn prefix_width(prefix: &str) -> usize {
    UnicodeWidthStr::width(prefix)
}

fn wrap_inline_text(
    text: &str,
    first_prefix: &str,
    rest_prefix: &str,
    width: usize,
    base_style: Style,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let segments = parse_inline_segments(text, base_style, theme);
    wrap_segments_with_prefix(segments, first_prefix, rest_prefix, width, base_style)
}

#[derive(Clone)]
struct InlineSegment {
    text: String,
    style: Style,
}

fn parse_inline_segments(text: &str, base_style: Style, theme: &Theme) -> Vec<InlineSegment> {
    let mut segments = Vec::new();
    let mut plain = String::new();
    let mut index = 0;

    while index < text.len() {
        let rest = &text[index..];

        if let Some(inner) = capture_wrapped(rest, "**", "**") {
            if !plain.is_empty() {
                segments.push(InlineSegment {
                    text: mem::take(&mut plain),
                    style: base_style,
                });
            }
            segments.push(InlineSegment {
                text: inner.0.to_string(),
                style: base_style.add_modifier(Modifier::BOLD),
            });
            index += inner.1;
            continue;
        }

        if let Some(inner) = capture_wrapped(rest, "`", "`") {
            if !plain.is_empty() {
                segments.push(InlineSegment {
                    text: mem::take(&mut plain),
                    style: base_style,
                });
            }
            segments.push(InlineSegment {
                text: inner.0.to_string(),
                style: base_style.fg(theme.blue).bg(theme.code_bg),
            });
            index += inner.1;
            continue;
        }

        if let Some(inner) = capture_wrapped(rest, "*", "*") {
            if !plain.is_empty() {
                segments.push(InlineSegment {
                    text: mem::take(&mut plain),
                    style: base_style,
                });
            }
            segments.push(InlineSegment {
                text: inner.0.to_string(),
                style: base_style.add_modifier(Modifier::ITALIC),
            });
            index += inner.1;
            continue;
        }

        if let Some((label, consumed)) = capture_link(rest) {
            if !plain.is_empty() {
                segments.push(InlineSegment {
                    text: mem::take(&mut plain),
                    style: base_style,
                });
            }
            segments.push(InlineSegment {
                text: label,
                style: base_style
                    .fg(theme.blue)
                    .add_modifier(Modifier::UNDERLINED),
            });
            segments.push(InlineSegment {
                text: " ↗".to_string(),
                style: Style::default().fg(theme.muted),
            });
            index += consumed;
            continue;
        }

        let Some(ch) = rest.chars().next() else {
            break;
        };
        plain.push(ch);
        index += ch.len_utf8();
    }

    if !plain.is_empty() {
        segments.push(InlineSegment {
            text: plain,
            style: base_style,
        });
    }

    if segments.is_empty() {
        vec![InlineSegment {
            text: String::new(),
            style: base_style,
        }]
    } else {
        segments
    }
}

fn capture_wrapped<'a>(text: &'a str, open: &str, close: &str) -> Option<(&'a str, usize)> {
    if !text.starts_with(open) {
        return None;
    }
    let end = text.get(open.len()..)?.find(close)?;
    let start = open.len();
    let finish = start + end;
    Some((&text[start..finish], finish + close.len()))
}

fn capture_link(text: &str) -> Option<(String, usize)> {
    if !text.starts_with('[') {
        return None;
    }
    let label_end = text.find("](")?;
    let after_label = label_end + 2;
    let href_end = text.get(after_label..)?.find(')')?;
    Some((text[1..label_end].to_string(), after_label + href_end + 1))
}

fn wrap_segments_with_prefix(
    segments: Vec<InlineSegment>,
    first_prefix: &str,
    rest_prefix: &str,
    width: usize,
    prefix_style: Style,
) -> Vec<Line<'static>> {
    let limit = width.max(1);
    let first_prefix_width = prefix_width(first_prefix);
    let rest_prefix_width = prefix_width(rest_prefix);
    let mut lines = Vec::new();
    let mut current_spans = if first_prefix.is_empty() {
        Vec::new()
    } else {
        vec![Span::styled(first_prefix.to_string(), prefix_style)]
    };
    let mut current_width = first_prefix_width;
    let mut current_prefix_width = first_prefix_width;
    let mut first_line = true;
    let mut wrote_anything = false;

    for segment in segments {
        let mut chunk = String::new();
        for ch in segment.text.chars() {
            let ch_width = string_width_char(ch);
            if current_width + ch_width > limit && current_width > current_prefix_width {
                if !chunk.is_empty() {
                    current_spans.push(Span::styled(mem::take(&mut chunk), segment.style));
                }
                lines.push(Line::from(mem::take(&mut current_spans)));
                first_line = false;
                current_prefix_width = rest_prefix_width;
                current_width = current_prefix_width;
                if !rest_prefix.is_empty() {
                    current_spans.push(Span::styled(rest_prefix.to_string(), prefix_style));
                }
            }
            chunk.push(ch);
            current_width += ch_width;
            wrote_anything = true;
        }
        if !chunk.is_empty() {
            current_spans.push(Span::styled(chunk, segment.style));
        }
    }

    if current_spans.is_empty() && !first_line && !rest_prefix.is_empty() {
        current_spans.push(Span::styled(rest_prefix.to_string(), prefix_style));
    }
    if !wrote_anything && current_spans.is_empty() {
        current_spans.push(Span::raw(String::new()));
    }

    lines.push(Line::from(current_spans));
    lines
}

fn string_width_char(ch: char) -> usize {
    UnicodeWidthStr::width(ch.to_string().as_str())
}

fn string_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

fn render_mermaid_ascii(graph: &MermaidGraph, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for (index, edge) in graph.edges.iter().enumerate() {
        let from = graph
            .labels
            .get(&edge.from)
            .cloned()
            .unwrap_or_else(|| edge.from.clone());
        let to = graph
            .labels
            .get(&edge.to)
            .cloned()
            .unwrap_or_else(|| edge.to.clone());

        let prefix = if index == 0 { "┌" } else { "├" };
        let first = format!("{prefix}─ {}", truncate_to_width(&from, width.saturating_sub(3)));
        lines.push(first);
        lines.push(format!("│  {}", truncate_to_width("│", width.saturating_sub(3))));
        lines.push(format!("│  └─▶ {}", truncate_to_width(&to, width.saturating_sub(7))));
        if !edge.label.trim().is_empty() {
            lines.push(format!(
                "│     {}",
                truncate_to_width(&format!("({})", edge.label), width.saturating_sub(7))
            ));
        }
    }
    if lines.is_empty() {
        lines.push("└─ empty flowchart".to_string());
    }
    lines
}

fn parse_mermaid_graph(code_lines: &[String]) -> MermaidGraph {
    let mut graph = MermaidGraph::default();
    for line in code_lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("flowchart") || trimmed.starts_with("graph") {
            continue;
        }
        if let Some((from, label, to)) = parse_mermaid_edge(trimmed) {
            graph.edges.push(MermaidEdge {
                from: from.0.clone(),
                to: to.0.clone(),
                label,
            });
            graph.labels.insert(from.0, from.1);
            graph.labels.insert(to.0, to.1);
        }
    }
    graph
}

fn parse_mermaid_edge(line: &str) -> Option<((String, String), String, (String, String))> {
    let arrow_patterns = ["-->|", "-->", "==>", "->"];
    for pattern in arrow_patterns {
        if let Some(index) = line.find(pattern) {
            let left = line[..index].trim();
            let right = line[index + pattern.len()..].trim();
            if pattern == "-->|" {
                if let Some(end) = right.find('|') {
                    let label = right[..end].trim().to_string();
                    let target = right[end + 1..].trim();
                    return Some((parse_mermaid_node(left), label, parse_mermaid_node(target)));
                }
                continue;
            }
            return Some((
                parse_mermaid_node(left),
                String::new(),
                parse_mermaid_node(right),
            ));
        }
    }
    None
}

fn parse_mermaid_node(node: &str) -> (String, String) {
    let trimmed = node.trim();
    if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            let id = trimmed[..start].trim().to_string();
            let label = trimmed[start + 1..end].trim().to_string();
            return (id.clone(), if label.is_empty() { id } else { label });
        }
    }
    if let Some(start) = trimmed.find('(') {
        if let Some(end) = trimmed.rfind(')') {
            let id = trimmed[..start].trim().to_string();
            let label = trimmed[start + 1..end].trim().to_string();
            return (id.clone(), if label.is_empty() { id } else { label });
        }
    }
    (trimmed.to_string(), trimmed.to_string())
}

fn wrap_plain_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut rows = Vec::new();

    for source_line in text.split('\n') {
        if source_line.is_empty() {
            rows.push(String::new());
            continue;
        }

        let mut current = String::new();
        let mut current_width = 0;
        for ch in source_line.chars() {
            let ch_width = string_width_char(ch);
            if current_width + ch_width > width && !current.is_empty() {
                rows.push(mem::take(&mut current));
                current_width = 0;
            }
            current.push(ch);
            current_width += ch_width;
        }
        if current.is_empty() {
            rows.push(String::new());
        } else {
            rows.push(current);
        }
    }

    if rows.is_empty() {
        rows.push(String::new());
    }
    rows
}

fn render_right_panel_lines(data: &RightPanelData, _width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("Blackboard", Style::default().fg(Color::Rgb(232, 235, 245))),
        Span::styled(
            format!("  [{}]", data.thinking_label),
            Style::default().fg(Color::Rgb(185, 140, 255)),
        ),
    ]));
    lines.push(Line::raw(""));
    lines.push(Line::styled("Questions", Style::default().fg(Color::Rgb(126, 160, 255))));
    for question in &data.questions {
        lines.push(Line::styled(
            question.to_string(),
            Style::default().fg(if question.starts_with('›') {
                Color::Rgb(255, 120, 170)
            } else {
                Color::Rgb(232, 235, 245)
            }),
        ));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled("Blackboard", Style::default().fg(Color::Rgb(232, 235, 245))));
    lines.push(Line::styled(
        format!("   {}", data.blackboard_status),
        Style::default().fg(Color::Rgb(232, 235, 245)),
    ));
    lines.push(Line::styled("   goal:", Style::default().fg(Color::Rgb(232, 235, 245))));
    for line in &data.goal_lines {
        lines.push(Line::styled(
            format!("   {line}"),
            Style::default().fg(Color::Rgb(147, 156, 182)),
        ));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled("MODEL", Style::default().fg(Color::Rgb(126, 160, 255))));
    for stat in &data.model_stats {
        lines.push(Line::styled(
            format!("{:<12}{}", stat.label, stat.value),
            Style::default().fg(Color::Rgb(232, 235, 245)),
        ));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled("TOKENS", Style::default().fg(Color::Rgb(126, 160, 255))));
    for stat in &data.token_stats {
        lines.push(Line::styled(
            format!("{:<12}{}", stat.label, stat.value),
            Style::default().fg(Color::Rgb(232, 235, 245)),
        ));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled("CONTEXT WINDOW", Style::default().fg(Color::Rgb(126, 160, 255))));
    lines.push(Line::styled(
        format!("{}                {}", data.context_total, data.context_percent),
        Style::default().fg(Color::Rgb(232, 235, 245)),
    ));
    lines.push(Line::styled(
        data.context_bar.to_string(),
        Style::default().fg(Color::Rgb(185, 140, 255)),
    ));
    lines.push(Line::styled(
        data.context_usage.to_string(),
        Style::default().fg(Color::Rgb(232, 235, 245)),
    ));
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        data.footer.to_string(),
        Style::default().fg(Color::Rgb(147, 156, 182)),
    ));
    lines
}

fn pad_to_width(text: &str, width: usize) -> String {
    let current = UnicodeWidthStr::width(text);
    if current >= width {
        text.to_string()
    } else {
        format!("{text}{}", " ".repeat(width - current))
    }
}

fn truncate_to_width(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= width {
        return text.to_string();
    }

    let mut output = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let ch_width = string_width_char(ch);
        if used + ch_width + 1 > width {
            break;
        }
        output.push(ch);
        used += ch_width;
    }
    output.push('…');
    output
}

fn build_runtime_turn(index: usize, user_text: String) -> Turn {
    let duration = 320 + ((index * 73) % 480) as u64;
    Turn {
        user: user_text.clone(),
        thought: Some(ThoughtData {
            summary: format!("Thought for {duration}ms"),
            duration_ms: Some(duration),
            expanded: false,
            content: format!(
                "## Intent\n\
梳理用户输入，并生成一段符合当前会话样式的 mock 回答。\n\n\
### Checks\n\
- 保持左侧滚动与 resize 同步\n\
- 点击 Thought 头部可展开\n\
- mermaid 继续走图形化渲染路径\n\n\
> 当前输入：{user_text}\n\n\
```mermaid\n\
flowchart TD\n\
    A[Input] --> B[Mock reasoning]\n\
    B --> C[Compose answer]\n\
    C --> D[Render in TUI]\n\
```"
            ),
        }),
        answer: format!(
            "已收到你的输入：**{user_text}**。\n\n\
下面这条回复会继续复用当前的会话块样式，并测试 markdown 的几个关键块：\n\n\
| field | value | note |\n\
| --- | --- | --- |\n\
| mode | runtime mock | after Enter |\n\
| scroll | sticky bottom | when sending |\n\
| thought | clickable | collapsible |\n\n\
```rust\n\
let response = \"mock assistant reply\";\n\
println!(\"{{}}\", response);\n\
```\n\n\
```mermaid\n\
flowchart LR\n\
    User[User] --> Thought[Thought]\n\
    Thought --> Answer[Answer]\n\
    Answer --> UI[UI]\n\
```\n\n\
[Inspect turn model](https://example.com)"
        ),
        footer: format!("Sisyphus - Ultraworker · DeepSeek V4 Flash · {}.{}s", duration / 1000, (duration % 1000) / 100),
    }
}

struct Theme {
    bg: Color,
    text: Color,
    muted: Color,
    dim: Color,
    blue: Color,
    purple: Color,
    pink: Color,
    green: Color,
    dev: Color,
    overlay: Color,
    scroll_thumb: Color,
    scroll_track: Color,
    status_active_bg: Color,
    status_idle_bg: Color,
    user_bg: Color,
    code_bg: Color,
    rail: Color,
    thread_accent: Color,
    thought_bar: Color,
    thought_text: Color,
    footer_icon_color: Color,
    footer_primary: Color,
    footer_muted: Color,
    code_label: Color,
    mermaid_label: Color,
    mermaid_text: Color,
    rail_char: char,
    thought_bar_char: char,
    thread_bar_char: char,
    footer_icon: char,
    thread_gutter: usize,
    user_pad: usize,
}

#[derive(Clone, Deserialize)]
struct ThoughtData {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    expanded: bool,
    #[serde(default)]
    content: String,
}

#[derive(Clone, Deserialize)]
struct Turn {
    user: String,
    #[serde(default)]
    thought: Option<ThoughtData>,
    answer: String,
    #[serde(default)]
    footer: String,
}

#[derive(Clone, Deserialize)]
struct TodoItem {
    marker: String,
    label: String,
    status: String,
    active: bool,
}

#[derive(Clone, Deserialize)]
struct StatItem {
    label: String,
    value: String,
}

#[derive(Clone, Deserialize)]
struct RightPanelData {
    thinking_label: String,
    blackboard_status: String,
    questions: Vec<String>,
    goal_lines: Vec<String>,
    model_stats: Vec<StatItem>,
    token_stats: Vec<StatItem>,
    context_total: String,
    context_percent: String,
    context_bar: String,
    context_usage: String,
    footer: String,
}

#[derive(Deserialize)]
struct MockData {
    turns: Vec<Turn>,
    todos: Vec<TodoItem>,
    right_panel: RightPanelData,
}

#[derive(Default)]
struct MermaidGraph {
    edges: Vec<MermaidEdge>,
    labels: std::collections::HashMap<String, String>,
}

struct MermaidEdge {
    from: String,
    to: String,
    label: String,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg: Color::Rgb(12, 18, 32),
            text: Color::Rgb(232, 235, 245),
            muted: Color::Rgb(147, 156, 182),
            dim: Color::Rgb(67, 77, 105),
            blue: Color::Rgb(126, 160, 255),
            purple: Color::Rgb(185, 140, 255),
            pink: Color::Rgb(255, 120, 170),
            green: Color::Rgb(91, 228, 155),
            dev: Color::Rgb(96, 165, 250),
            overlay: Color::Rgb(10, 14, 28),
            scroll_thumb: Color::Rgb(218, 220, 228),
            scroll_track: Color::Rgb(107, 116, 144),
            status_active_bg: Color::Rgb(42, 38, 84),
            status_idle_bg: Color::Rgb(28, 34, 55),
            user_bg: Color::Rgb(24, 24, 24),
            code_bg: Color::Rgb(18, 20, 24),
            rail: Color::Rgb(150, 150, 156),
            thread_accent: Color::Rgb(0, 210, 230),
            thought_bar: Color::Rgb(150, 150, 156),
            thought_text: Color::Rgb(137, 137, 145),
            footer_icon_color: Color::Rgb(0, 210, 230),
            footer_primary: Color::Rgb(220, 223, 228),
            footer_muted: Color::Rgb(137, 137, 145),
            code_label: Color::Rgb(175, 180, 196),
            mermaid_label: Color::Rgb(175, 180, 196),
            mermaid_text: Color::Rgb(214, 218, 228),
            rail_char: '│',
            thought_bar_char: '│',
            thread_bar_char: '│',
            footer_icon: '◻',
            thread_gutter: 3,
            user_pad: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mermaid_edges_with_labels() {
        let parsed = parse_mermaid_edge("A[Input] --> B[Render]");
        let (from, label, to) = parsed.expect("edge should parse");
        assert_eq!(from.0, "A");
        assert_eq!(from.1, "Input");
        assert!(label.is_empty());
        assert_eq!(to.0, "B");
        assert_eq!(to.1, "Render");
    }

    #[test]
    fn renders_mermaid_ascii_lines() {
        let graph = parse_mermaid_graph(&[
            "flowchart LR".to_string(),
            "A[User] --> B[Thought]".to_string(),
            "B --> C[Answer]".to_string(),
        ]);
        let lines = render_mermaid_ascii(&graph, 40);
        assert!(!lines.is_empty());
        assert!(lines.iter().any(|line| line.contains("User")));
        assert!(lines.iter().any(|line| line.contains("Answer")));
    }
}

fn load_mock_data() -> MockData {
    serde_json::from_str(include_str!("../mock-data.json")).expect("invalid mock-data.json")
}

#[derive(Clone, Copy, Default)]
struct LayoutSnapshot {
    frame: Rect,
    content: Rect,
    top_bar: Rect,
    left_panel: Rect,
    right_panel: Rect,
}

fn dev_mode_enabled() -> bool {
    env::args().any(|arg| arg == "--dev")
        || env::var("FLYFLOR_DEV")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "on" | "ON"))
            .unwrap_or(false)
}
