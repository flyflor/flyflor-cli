use std::{
    collections::HashMap,
    env,
    fs::{OpenOptions, create_dir_all},
    io,
    io::ErrorKind,
    io::IsTerminal,
    io::Write,
    mem,
    net::TcpStream,
    panic,
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use arboard::Clipboard;
use base64::Engine as _;
use crossterm::{
    event::{
        self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
        MouseEventKind,
    },
    terminal::{disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph, Wrap},
};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tungstenite::{Error as WsError, Message, connect, stream::MaybeTlsStream};
use unicode_width::UnicodeWidthStr;

mod context;
mod layout;
mod shared;
mod tui;

use shared::{
    center_text, draw_separator, in_rect, metric_line, top_bar_title, working_light_line,
    working_light_phase, working_shimmer_style, ws_url,
};
use tui::ask::{
    command::AskAnswer,
    parser::{ask_menu_from_turn_metadata, continuation_from_metadata, continuation_from_value},
    state::AskMenu,
    view::visible_item_count,
};
use tui::fork::{
    command::{ForkCreateSource, fork_create_payload},
    state::ActiveForkSession,
    view::session_summary,
};
use tui::gateway::{
    client::GatewayClientBootstrap,
    command::{GatewayCommandBuilder, GatewayMessagePayload},
    envelope::EnvelopeFactory,
};
use tui::plan::{
    menu::default_plan_menu,
    state::{PlanAction, PlanMenu, PlanPendingAction},
};
use tui::run_timeline::{state::RunTimeline, view::run_panel_lines};
use tui::terminal::{
    TerminalMode, enter_terminal, leave_terminal, mouse_capture_enabled_from_env_args,
};

const CLIPBOARD_INIT_TIMEOUT: Duration = Duration::from_millis(500);
const OSC52_MAX_BYTES: usize = 100 * 1024;
const DEFAULT_WS_URL: &str = "ws://127.0.0.1:8787/ws";
const DEFAULT_CONTEXT_BAR_WIDTH: usize = 32;

fn main() -> io::Result<()> {
    install_panic_logger();
    log_event("main start");
    let terminal_mode = TerminalMode {
        use_mouse_capture: mouse_capture_enabled_from_env_args(),
    };
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    enter_terminal(&mut stdout, terminal_mode)?;
    let terminal = ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(stdout))?;
    let result = run(terminal, terminal_mode);
    disable_raw_mode()?;
    leave_terminal(&mut io::stdout(), terminal_mode)?;
    if let Err(error) = &result {
        log_event(format!("main error {error}"));
    }
    log_event("main exit");
    result
}

fn install_panic_logger() {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        log_event(format!("panic {info}"));
        default_hook(info);
    }));
}

fn log_event(message: impl AsRef<str>) {
    let path = log_path();
    if let Some(parent) = path.parent() {
        let _ = create_dir_all(parent);
    }
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    let _ = writeln!(
        file,
        "{} rust {}",
        iso8601_from_millis(now_millis()),
        message.as_ref()
    );
}

fn log_path() -> PathBuf {
    env::var("FLYFLOR_LOG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(".flyflor-cli/logs/dev.log"))
}

fn run(mut terminal: DefaultTerminal, terminal_mode: TerminalMode) -> io::Result<()> {
    let mut app = App::new();
    loop {
        app.drain_socket_events();
        app.drain_clipboard_events();
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
            Event::Paste(text) => app.insert_paste_text(&text),
            Event::Mouse(mouse) if terminal_mode.use_mouse_capture => handle_mouse(&mut app, mouse),
            _ => {}
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn handle_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('c')
            if key.modifiers.contains(KeyModifiers::CONTROL)
                || key.modifiers.contains(KeyModifiers::SUPER) =>
        {
            app.handle_ctrl_c();
        }
        KeyCode::Char('v')
            if key.modifiers.contains(KeyModifiers::CONTROL)
                || key.modifiers.contains(KeyModifiers::SUPER) =>
        {
            app.paste_from_clipboard()
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.dev_mode = !app.dev_mode
        }
        KeyCode::F(2) => app.dev_mode = !app.dev_mode,
        KeyCode::BackTab => app.toggle_interaction_mode(),
        KeyCode::Esc => app.close_menus(),
        KeyCode::Tab if app.handle_menu_confirm_or_next(false) => {}
        KeyCode::Enter
            if key.modifiers.contains(KeyModifiers::ALT)
                || key.modifiers.contains(KeyModifiers::CONTROL)
                || key.modifiers.contains(KeyModifiers::SHIFT) =>
        {
            app.insert_input_text("\n")
        }
        KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.insert_input_text("\n")
        }
        KeyCode::Up if app.move_active_menu(-1) => {}
        KeyCode::Down if app.move_active_menu(1) => {}
        KeyCode::Up => app.move_input_cursor_vertical(-1),
        KeyCode::Down => app.move_input_cursor_vertical(1),
        KeyCode::Char('y') if app.should_copy_with_y() => app.copy_selection_to_clipboard(),
        KeyCode::Right => app.move_input_cursor_right(),
        KeyCode::Left => app.move_input_cursor_left(),
        KeyCode::PageUp => app.scroll_left_by(-(app.left.viewport_height as isize - 2)),
        KeyCode::PageDown => app.scroll_left_by(app.left.viewport_height as isize - 2),
        KeyCode::Backspace => {
            app.backspace_input();
            app.refresh_command_palette();
        }
        KeyCode::Enter if app.handle_menu_confirm_or_next(true) => {}
        KeyCode::Enter => app.submit_input(),
        KeyCode::Char(ch) if app.select_active_menu_number(ch) => {}
        KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.insert_input_char(ch);
            app.refresh_command_palette();
        }
        _ => {}
    }
}

fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            if app.is_right_todo_scroll_hit(mouse.column, mouse.row) {
                app.scroll_right_todo_by(-3);
            } else {
                app.scroll_left_by(-3);
            }
        }
        MouseEventKind::ScrollDown => {
            if app.is_right_todo_scroll_hit(mouse.column, mouse.row) {
                app.scroll_right_todo_by(3);
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
                    target: ScrollTarget::RightTodo,
                    anchor_row: mouse.row,
                    anchor_scroll: app.right.scroll,
                });
            } else if app.toggle_context_row_at(mouse.column, mouse.row) {
                app.drag = None;
            } else if app.begin_selection_at(mouse.column, mouse.row) {
                app.drag = None;
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(drag) = app.drag {
                let delta = mouse.row as isize - drag.anchor_row as isize;
                match drag.target {
                    ScrollTarget::Left => app.drag_scroll_left(drag.anchor_scroll, delta),
                    ScrollTarget::RightTodo => {
                        app.drag_scroll_right_todo(drag.anchor_scroll, delta)
                    }
                }
            } else {
                app.update_selection_at(mouse.column, mouse.row);
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            app.drag = None;
            app.finish_selection_at(mouse.column, mouse.row);
        }
        _ => {}
    }
}

fn draw(frame: &mut Frame, app: &mut App) {
    let theme = Theme::default();
    app.cursor = None;
    frame.render_widget(Clear, frame.area());
    let root = layout::shell::content_root(frame.area());
    let header_height = if app.is_working() { 2 } else { 1 };
    let layout = layout::shell::app_layout(root, header_height, &app.input);

    app.layout = LayoutSnapshot {
        left_panel: layout.left_main,
        right_panel: layout.right_main,
    };

    layout::shell::draw_shell(frame, &layout, &theme);
    draw_top_bar(frame, layout.header, app, &theme);
    draw_left_panel(frame, layout.left_main, app, &theme);
    draw_right_panel(frame, layout.right_main, app, &theme);
    draw_left_composer(frame, layout.left_composer, app, &theme);
    draw_footer(frame, layout.footer_text, app, &theme);

    let _ = app.dev_mode;
}

fn draw_top_bar(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let top_area = Rect::new(area.x, area.y, area.width, 1);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(top_area);

    let left = Line::from(vec![
        Span::styled("◎", Style::default().fg(theme.purple)),
        Span::styled(
            format!(" {}", top_bar_title()),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
    ]);
    let (status_dot, status_text) = match app.history_status {
        HistoryStatus::Loading => (theme.blue, "loading history"),
        HistoryStatus::Live => (theme.green, "connected"),
        HistoryStatus::Offline => (theme.muted, "mock history"),
        HistoryStatus::Error => (theme.pink, "history unavailable"),
    };
    let right = Line::from(vec![
        Span::styled("●", Style::default().fg(status_dot)),
        Span::styled(
            format!(
                " flyflor · {} · {} · {status_text} · {} turns",
                app.interaction_mode.label(),
                app.fork_session_label(),
                app.turns.len()
            ),
            Style::default().fg(theme.text),
        ),
    ]);

    frame.render_widget(Paragraph::new(left), cols[0]);
    frame.render_widget(Paragraph::new(right).alignment(Alignment::Right), cols[1]);
    if app.is_working() && area.height > 1 {
        let phase = working_light_phase(now_millis());
        frame.render_widget(
            Paragraph::new(working_light_line(area.width as usize, phase, &theme)),
            Rect::new(area.x, area.y + 1, area.width, 1),
        );
    }
}

fn fork_footer_suffix(app: &App) -> String {
    app.active_fork
        .as_ref()
        .map(|fork| {
            let label = session_summary(Some(fork)).unwrap_or(&fork.fork_id);
            format!("   fork {}", truncate_to_width(label, 18))
        })
        .unwrap_or_default()
}

fn draw_left_panel(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    let text_area = Rect::new(
        inner.x,
        inner.y,
        inner.width.saturating_sub(2),
        inner.height,
    );
    app.update_left_viewport(text_area, theme);

    let paragraph = Paragraph::new(app.visible_chat_lines(theme));
    frame.render_widget(paragraph, text_area);
    draw_scrollbar(frame, app.left.scrollbar, theme);
}

fn draw_left_composer(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let inner = area.inner(Margin {
        vertical: 0,
        horizontal: 1,
    });
    if inner.height == 0 {
        return;
    }
    draw_separator(frame, Rect::new(inner.x, inner.y, inner.width, 1), theme);
    let input_inner = Rect::new(
        inner.x,
        inner.y + 1,
        inner.width,
        inner.height.saturating_sub(1),
    );
    if input_inner.height > 0 {
        let input_lines = render_input_lines(&app.input, input_inner.width as usize, theme);
        let visible_len = input_inner.height as usize;
        let scroll = input_lines.len().saturating_sub(visible_len);
        frame.render_widget(
            Paragraph::new(input_lines).scroll((scroll as u16, 0)),
            input_inner,
        );
        app.cursor =
            input_cursor_position(&app.input, app.input_cursor_index(), input_inner, scroll);

        draw_composer_menu(frame, input_inner, app, theme);
    }
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    if area.height == 0 {
        return;
    }
    frame.render_widget(Paragraph::new(composer_footer_line(app, theme)), area);
}

fn draw_composer_menu(frame: &mut Frame, input_inner: Rect, app: &App, theme: &Theme) {
    let Some(menu) = app.active_menu_lines(theme) else {
        return;
    };
    let Some(area) = composer_menu_area(input_inner, menu.len()) else {
        return;
    };
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(menu).style(Style::default().bg(theme.overlay)),
        area,
    );
}

fn composer_menu_area(input_inner: Rect, menu_len: usize) -> Option<Rect> {
    if menu_len == 0 || input_inner.y == 0 {
        return None;
    }
    let width = input_inner.width.min(96);
    let height = (menu_len as u16).min(input_inner.y).min(14);
    if width < 12 || height == 0 {
        return None;
    }
    Some(Rect::new(
        input_inner.x,
        input_inner.y.saturating_sub(height),
        width,
        height,
    ))
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

    let text_width = inner.width.saturating_sub(2).max(1);
    let data = app.current_right_panel_data();
    let bottom_height = right_bottom_height(&data, text_width as usize, inner.height);
    let layout = right_panel_layout(inner, bottom_height);
    app.update_right_viewport(right_todo_body_area(layout.todo));
    if let Some(todo) = app.right_sections.first() {
        let title = render_right_section_title(todo, text_width as usize, false);
        let title_area = Rect::new(
            layout.todo.x,
            layout.todo.y,
            layout.todo.width,
            layout.todo.height.min(1),
        );
        frame.render_widget(Paragraph::new(vec![title]), title_area);
        let body_area = right_todo_body_area(layout.todo);
        let todo_content = Paragraph::new(todo.lines.clone())
            .style(Style::default().fg(theme.text))
            .wrap(Wrap { trim: false })
            .scroll((app.right.scroll as u16, 0));
        frame.render_widget(todo_content, body_area);
    }
    draw_scrollbar(frame, app.right.scrollbar, theme);
    let content =
        Paragraph::new(app.visible_right_lines(theme, layout.bottom_text.height as usize))
            .style(Style::default().fg(theme.text))
            .wrap(Wrap { trim: false });
    frame.render_widget(content, layout.bottom_text);
}

fn draw_compact_sidebar(frame: &mut Frame, area: Rect, theme: &Theme) {
    let compact = Paragraph::new(vec![
        Line::styled("ACT 计划", Style::default().fg(theme.text)),
        Line::styled("○ 暂无计划", Style::default().fg(theme.muted)),
        Line::raw(""),
        Line::styled("CONTEXT WINDOW", Style::default().fg(theme.blue)),
        metric_line("model", "未知模型", theme),
        metric_line("usage", "未收到上下文窗口", theme),
    ]);
    frame.render_widget(compact, area);
}

fn move_index(current: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    let next = current as isize + delta;
    next.rem_euclid(len as isize) as usize
}

fn slash_commands() -> Vec<SlashCommand> {
    vec![
        SlashCommand {
            name: "exit",
            title: "退出 TUI",
            detail: "关闭 FlyFlor CLI",
            kind: SlashCommandKind::Exit,
        },
        SlashCommand {
            name: "help",
            title: "命令帮助",
            detail: "显示可用 slash 命令",
            kind: SlashCommandKind::Help,
        },
        SlashCommand {
            name: "yolo",
            title: "切换 YOLO",
            detail: "危险：外模式会绕过沙箱/高权限执行",
            kind: SlashCommandKind::Yolo,
        },
        SlashCommand {
            name: "model",
            title: "模型状态",
            detail: "只读显示 provider / 上下文窗口 / 最大输出",
            kind: SlashCommandKind::Model,
        },
        SlashCommand {
            name: "status",
            title: "刷新状态",
            detail: "请求 gateway.status.get",
            kind: SlashCommandKind::Status,
        },
        SlashCommand {
            name: "history",
            title: "刷新历史",
            detail: "请求 history.list",
            kind: SlashCommandKind::History,
        },
        SlashCommand {
            name: "fork",
            title: "新建 fork",
            detail: "从最近回答创建 context fork",
            kind: SlashCommandKind::Fork,
        },
        SlashCommand {
            name: "ask",
            title: "回答 ASK",
            detail: "打开待回答 ASK 选项菜单",
            kind: SlashCommandKind::Ask,
        },
        SlashCommand {
            name: "blackboard",
            title: "Blackboard",
            detail: "查看 blackboard 摘要",
            kind: SlashCommandKind::Blackboard,
        },
        SlashCommand {
            name: "todo",
            title: "刷新 TODO",
            detail: "请求 task.list",
            kind: SlashCommandKind::Todo,
        },
        SlashCommand {
            name: "memory",
            title: "回忆摘要",
            detail: "查看 recall / memory 摘要",
            kind: SlashCommandKind::Memory,
        },
        SlashCommand {
            name: "recall",
            title: "回忆摘要",
            detail: "同 /memory",
            kind: SlashCommandKind::Memory,
        },
    ]
}

fn slash_commands_for_fork(in_fork: bool) -> Vec<SlashCommand> {
    let mut commands = slash_commands();
    if in_fork {
        commands.push(SlashCommand {
            name: "exit fork",
            title: "退出 fork",
            detail: "回到父级/root 对话",
            kind: SlashCommandKind::ExitFork,
        });
    }
    commands
}

fn render_command_palette_lines(menu: &CommandPalette, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled(
        format!("命令菜单 /{}", menu.query),
        Style::default().fg(theme.blue).add_modifier(Modifier::BOLD),
    )];
    if menu.items.is_empty() {
        lines.push(Line::styled(
            "  未知命令，Enter 显示提示",
            Style::default().fg(theme.muted),
        ));
        return lines;
    }
    for (index, command) in menu.items.iter().take(7).enumerate() {
        let selected = index == menu.selected;
        lines.push(Line::from(vec![
            Span::styled(
                if selected { "› " } else { "  " },
                Style::default().fg(if selected { theme.pink } else { theme.muted }),
            ),
            Span::styled(
                format!("/{} ", command.name),
                Style::default()
                    .fg(if selected { theme.text } else { theme.muted })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(command.title.to_string(), Style::default().fg(theme.text)),
            Span::styled(
                format!(" · {}", command.detail),
                Style::default().fg(theme.muted),
            ),
        ]));
    }
    lines
}

fn render_ask_menu_lines(menu: &AskMenu, theme: &Theme) -> Vec<Line<'static>> {
    let completed = menu
        .questions
        .iter()
        .enumerate()
        .filter(|(index, question)| {
            let selected = menu.selected_by_question.get(*index).copied().unwrap_or(0);
            question.choices.get(selected).is_some_and(|choice| {
                !choice.is_other
                    || menu
                        .freeform_by_question
                        .get(*index)
                        .and_then(|value| value.as_deref())
                        .is_some_and(|value| !value.trim().is_empty())
            })
        })
        .count();
    let mut lines = vec![Line::from(vec![
        Span::styled(
            "ASK",
            Style::default().fg(theme.blue).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "  问题 {}/{} · 完成 {completed}/{} · ↑↓ 选择 · 1-9 快选 · Enter 下一题/发送 · Esc 关闭",
                menu.active_question + 1,
                menu.questions.len().max(1),
                menu.questions.len()
            ),
            Style::default().fg(theme.muted),
        ),
    ])];
    for (question_index, question) in menu.questions.iter().enumerate() {
        let active = question_index == menu.active_question;
        let selected = menu
            .selected_by_question
            .get(question_index)
            .copied()
            .unwrap_or(0);
        let editing_other = active && menu.is_editing_other();
        lines.push(Line::from(vec![
            Span::styled(
                if active { "› " } else { "  " },
                Style::default().fg(if active { theme.pink } else { theme.muted }),
            ),
            Span::styled(
                format!("{}. {}", question_index + 1, question.prompt),
                Style::default()
                    .fg(if active { theme.text } else { theme.muted })
                    .add_modifier(if active {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
        ]));
        if active {
            for (choice_index, item) in question
                .choices
                .iter()
                .take(visible_item_count(menu, 7))
                .enumerate()
            {
                let choice_active = choice_index == selected;
                let freeform = if choice_active && item.is_other {
                    menu.freeform_by_question
                        .get(question_index)
                        .and_then(|value| value.as_deref())
                        .unwrap_or("")
                } else {
                    ""
                };
                lines.push(Line::from(vec![
                    Span::styled(
                        if choice_active { "  › " } else { "    " },
                        Style::default().fg(if choice_active {
                            theme.pink
                        } else {
                            theme.muted
                        }),
                    ),
                    Span::styled(
                        format!("{}. ", choice_index + 1),
                        Style::default().fg(theme.muted),
                    ),
                    Span::styled(
                        if editing_other && choice_active && item.is_other {
                            format!("{}: {}", item.label, freeform)
                        } else {
                            item.label.clone()
                        },
                        Style::default().fg(if item.is_other {
                            theme.green
                        } else {
                            theme.text
                        }),
                    ),
                    Span::styled(
                        if item.recommended { " [Recommend]" } else { "" },
                        Style::default()
                            .fg(theme.green)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        item.description
                            .as_ref()
                            .map(|description| format!(" · {description}"))
                            .unwrap_or_default(),
                        Style::default().fg(theme.muted),
                    ),
                ]));
            }
        } else if let Some(choice) = question.choices.get(selected) {
            let answer = menu
                .freeform_by_question
                .get(question_index)
                .and_then(|value| value.as_deref())
                .unwrap_or(choice.label.as_str());
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled("已选 ", Style::default().fg(theme.muted)),
                Span::styled(
                    truncate_to_width(answer, 54),
                    Style::default().fg(if choice.is_other {
                        theme.green
                    } else {
                        theme.text
                    }),
                ),
                Span::styled(
                    if choice.recommended {
                        " [Recommend]"
                    } else {
                        ""
                    },
                    Style::default().fg(theme.green),
                ),
            ]));
        }
    }
    if menu.questions.is_empty() {
        lines.push(Line::styled(
            "ASK payload is empty",
            Style::default().fg(theme.muted),
        ));
    }
    lines
}

fn render_plan_menu_lines(menu: &PlanMenu, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled(
        "计划操作 · Enter 确认 · Esc 关闭",
        Style::default().fg(theme.blue).add_modifier(Modifier::BOLD),
    )];
    for (index, item) in menu.items.iter().enumerate() {
        let selected = index == menu.selected;
        lines.push(Line::from(vec![
            Span::styled(
                if selected { "› " } else { "  " },
                Style::default().fg(if selected { theme.pink } else { theme.muted }),
            ),
            Span::styled(item.label.clone(), Style::default().fg(theme.text)),
        ]));
    }
    lines
}

fn draw_scrollbar(frame: &mut Frame, scrollbar: ScrollbarGeometry, theme: &Theme) {
    if scrollbar.track_height == 0 {
        return;
    }
    for offset in 0..scrollbar.track_height {
        let y = scrollbar.track_top + offset;
        let active = y >= scrollbar.thumb_top && y < scrollbar.thumb_top + scrollbar.thumb_height;
        let symbol = if active { "●" } else { "○" };
        let color = if active {
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
    RightTodo,
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
struct ContextRowRegion {
    turn_index: usize,
    row_index: usize,
    line_index: usize,
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
struct ThoughtHitbox {
    turn_index: usize,
    rect: Rect,
}

#[derive(Clone, Copy)]
enum BlockKind {
    User,
    Thought,
    Answer,
    Footer,
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
struct BlockRegion {
    kind: BlockKind,
    turn_index: usize,
    start_line: usize,
    end_line: usize,
}

#[allow(dead_code)]
struct LeftRender {
    lines: Vec<Line<'static>>,
    context_row_regions: Vec<ContextRowRegion>,
    thought_regions: Vec<ContextRowRegion>,
    block_regions: Vec<BlockRegion>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectionTarget {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SelectionPoint {
    target: SelectionTarget,
    line_index: usize,
    column: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct TranscriptSelection {
    anchor: Option<SelectionPoint>,
    head: Option<SelectionPoint>,
    dragging: bool,
}

impl TranscriptSelection {
    fn clear(&mut self) {
        self.anchor = None;
        self.head = None;
        self.dragging = false;
    }

    fn ordered_endpoints(&self) -> Option<(SelectionPoint, SelectionPoint)> {
        let anchor = self.anchor?;
        let head = self.head?;
        if anchor.target != head.target {
            return None;
        }
        if (head.line_index, head.column) < (anchor.line_index, anchor.column) {
            Some((head, anchor))
        } else {
            Some((anchor, head))
        }
    }
}

#[derive(Clone, Copy)]
struct ContextRowHitbox {
    turn_index: usize,
    row_index: usize,
    rect: Rect,
}

struct App {
    turns: Vec<Turn>,
    chat_lines: Vec<Line<'static>>,
    chat_render_key: Option<ChatRenderKey>,
    chat_context_regions: Vec<ContextRowRegion>,
    context_row_hitboxes: Vec<ContextRowHitbox>,
    right_lines: Vec<Line<'static>>,
    right_sections: Vec<RightPanelSection>,
    right_source: RightPanelData,
    fork_memory: ForkMemorySnapshot,
    todos: Vec<TodoItem>,
    left: ScrollState,
    right: ScrollState,
    dev_mode: bool,
    should_quit: bool,
    input: String,
    input_cursor: Option<usize>,
    cursor: Option<Position>,
    drag: Option<DragState>,
    layout: LayoutSnapshot,
    history_status: HistoryStatus,
    socket_connected: bool,
    socket_tx: Sender<SocketCommand>,
    socket_rx: Receiver<SocketEvent>,
    clipboard_tx: Sender<Result<String, String>>,
    clipboard_rx: Receiver<Result<String, String>>,
    selection: TranscriptSelection,
    composer_notice: Option<ComposerNotice>,
    active_context_fork_id: Option<String>,
    active_fork: Option<ActiveForkSession>,
    root_turns: Vec<Turn>,
    fork_stack: Vec<(ActiveForkSession, Vec<Turn>)>,
    pending_turns: HashMap<String, usize>,
    interaction_mode: InteractionMode,
    yolo_mode: bool,
    task_todos: Option<Vec<TodoItem>>,
    run_timeline: RunTimeline,
    model_context_window_tokens: Option<usize>,
    model_name: Option<String>,
    model_provider: Option<String>,
    max_output_tokens: Option<usize>,
    hot_context_tokens: Option<usize>,
    context_window_percent: Option<f64>,
    context_status: Option<String>,
    remaining_context_tokens: Option<usize>,
    cache_read_tokens: Option<usize>,
    cache_write_tokens: Option<usize>,
    command_palette: Option<CommandPalette>,
    ask_menu: Option<AskMenu>,
    plan_menu: Option<PlanMenu>,
    pending_plan_action: Option<PlanPendingAction>,
    pending_fork_create: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ChatRenderKey {
    width: usize,
    animation_phase: usize,
    turns_len: usize,
    last_turn_hash: u64,
    expanded_hash: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InteractionMode {
    Act,
    Plan,
    Yolo,
}

impl InteractionMode {
    fn next(self) -> Self {
        match self {
            Self::Act => Self::Plan,
            Self::Plan => Self::Yolo,
            Self::Yolo => Self::Act,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Act | Self::Yolo => "act",
            Self::Plan => "plan",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Act => "ACT",
            Self::Plan => "PLAN",
            Self::Yolo => "YOLO",
        }
    }

    fn yolo(self) -> bool {
        matches!(self, Self::Yolo)
    }

    fn color(self, theme: &Theme) -> Color {
        match self {
            Self::Act => theme.text,
            Self::Plan => theme.pink,
            Self::Yolo => theme.danger,
        }
    }
}

#[derive(Clone)]
struct CommandPalette {
    query: String,
    selected: usize,
    items: Vec<SlashCommand>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SlashCommandKind {
    Exit,
    Help,
    Yolo,
    Model,
    Status,
    History,
    Fork,
    Ask,
    Blackboard,
    Todo,
    Memory,
    ExitFork,
}

#[derive(Clone, Copy)]
struct SlashCommand {
    name: &'static str,
    title: &'static str,
    detail: &'static str,
    kind: SlashCommandKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlanState {
    Empty,
    Generating,
    AwaitingConfirmation,
    Running,
    Abandoned,
}

impl PlanState {
    fn label(self) -> &'static str {
        match self {
            Self::Empty => "暂无计划",
            Self::Generating => "计划生成中",
            Self::AwaitingConfirmation => "等待确认",
            Self::Running => "执行中",
            Self::Abandoned => "已放弃",
        }
    }
}

#[derive(Clone)]
struct RightPanelSection {
    title: String,
    lines: Vec<Line<'static>>,
}

#[derive(Clone, Copy)]
enum ComposerNotice {
    ExitHint,
}

#[derive(Clone, Copy)]
enum HistoryStatus {
    Loading,
    Live,
    Offline,
    Error,
}

impl App {
    fn new() -> Self {
        let mock = load_mock_data();
        let demo_mode = tui::demo_enabled();
        let (socket_tx, socket_rx) = spawn_socket_worker();
        let (clipboard_tx, clipboard_rx) = mpsc::channel();
        let history_status = if history_enabled() {
            HistoryStatus::Loading
        } else {
            HistoryStatus::Offline
        };
        Self {
            turns: mock.turns,
            chat_lines: Vec::new(),
            chat_render_key: None,
            chat_context_regions: Vec::new(),
            context_row_hitboxes: Vec::new(),
            right_lines: Vec::new(),
            right_sections: Vec::new(),
            right_source: mock.right_panel,
            fork_memory: mock.fork_memory,
            todos: mock.todos.clone(),
            left: ScrollState::default(),
            right: ScrollState::default(),
            dev_mode: dev_mode_enabled(),
            should_quit: false,
            input: String::new(),
            input_cursor: None,
            cursor: None,
            drag: None,
            layout: LayoutSnapshot::default(),
            history_status,
            socket_connected: false,
            socket_tx,
            socket_rx,
            clipboard_tx,
            clipboard_rx,
            selection: TranscriptSelection::default(),
            composer_notice: None,
            active_context_fork_id: demo_mode.then(|| "fork-demo-1".to_string()),
            active_fork: demo_mode.then(|| ActiveForkSession {
                fork_id: "fork-demo-1".to_string(),
                parent_fork_id: None,
                root_id: Some("root-demo".to_string()),
                summary: Some("demo fork".to_string()),
            }),
            root_turns: Vec::new(),
            fork_stack: Vec::new(),
            pending_turns: HashMap::new(),
            interaction_mode: if demo_mode {
                InteractionMode::Yolo
            } else {
                InteractionMode::Act
            },
            yolo_mode: demo_mode,
            task_todos: if demo_mode {
                Some(mock.todos.clone())
            } else {
                None
            },
            run_timeline: RunTimeline::new(),
            model_context_window_tokens: demo_mode.then_some(12000),
            model_name: demo_mode.then(|| "demo-model".to_string()),
            model_provider: demo_mode.then(|| "demo".to_string()),
            max_output_tokens: demo_mode.then_some(2048),
            hot_context_tokens: demo_mode.then_some(3360),
            context_window_percent: None,
            context_status: None,
            remaining_context_tokens: demo_mode.then_some(8640),
            cache_read_tokens: demo_mode.then_some(128),
            cache_write_tokens: demo_mode.then_some(32),
            command_palette: None,
            ask_menu: None,
            plan_menu: None,
            pending_plan_action: None,
            pending_fork_create: false,
        }
        .with_demo_state(demo_mode)
    }

    fn with_demo_state(mut self, demo_mode: bool) -> Self {
        if demo_mode {
            self.interaction_mode = InteractionMode::Yolo;
            self.yolo_mode = true;
            self.pending_fork_create = true;
            self.root_turns = self.turns.clone();
        }
        self
    }

    fn drain_clipboard_events(&mut self) {
        while let Ok(result) = self.clipboard_rx.try_recv() {
            match result {
                Ok(text) if !text.is_empty() => {
                    self.insert_paste_text(&text);
                }
                Ok(_) => {}
                Err(error) => {
                    log_event(format!("clipboard paste failed {error}"));
                    self.right_source.blackboard_status =
                        format!("clipboard unavailable · {error}");
                }
            }
        }
    }

    fn insert_paste_text(&mut self, text: &str) {
        self.insert_input_text(&normalize_paste_text(text));
        self.refresh_command_palette();
    }

    fn drain_socket_events(&mut self) {
        while let Ok(event) = self.socket_rx.try_recv() {
            self.apply_socket_event(event);
        }
    }

    fn apply_socket_event(&mut self, event: SocketEvent) {
        match event {
            SocketEvent::HistoryLoaded(turns) if !turns.is_empty() => {
                self.socket_connected = true;
                self.turns = turns;
                self.history_status = HistoryStatus::Live;
                self.left.initial_scroll_applied = false;
                self.left.stick_to_bottom = true;
            }
            SocketEvent::HistoryLoaded(_) | SocketEvent::Connected => {
                self.socket_connected = true;
                self.history_status = HistoryStatus::Live;
            }
            SocketEvent::TaskListLoaded(todos) => {
                self.task_todos = Some(todos);
            }
            SocketEvent::StatusLoaded(status) => {
                self.model_context_window_tokens = status.context_window_tokens;
                self.model_name = status.model_name.clone();
                self.model_provider = status.model_provider.clone();
                self.max_output_tokens = status.max_output_tokens;
                self.hot_context_tokens = status.hot_context_tokens;
                self.context_window_percent = status.context_window_percent;
                self.context_status = status.context_status.clone();
                self.remaining_context_tokens = status.remaining_context_tokens;
                self.cache_read_tokens = status.cache_read_tokens;
                self.cache_write_tokens = status.cache_write_tokens;
            }
            SocketEvent::ForkMemoryLoaded(snapshot) => {
                self.fork_memory = snapshot;
            }
            SocketEvent::RuntimeEvent(event) => {
                let item = self.run_timeline.apply_event_publish(&event);
                self.attach_execution_row_to_active_turn();
                let status = self.apply_runtime_event_side_effects(&event);
                if let Some(job_id) = job_id_from_event_payload(&event) {
                    let _ = self
                        .socket_tx
                        .send(SocketCommand::ExecutionJobDetailGet { job_id });
                }
                if let Some(status) = status {
                    self.right_source.blackboard_status = status;
                } else if let Some(item) = item {
                    self.right_source.blackboard_status = format!("runtime event · {}", item.title);
                }
            }
            SocketEvent::ExecutionJobSnapshot(snapshot) => {
                self.run_timeline.apply_execution_job_snapshot(&snapshot);
                self.attach_execution_row_to_active_turn();
                self.right_source.blackboard_status = "execution job snapshot updated".to_string();
            }
            SocketEvent::ContextSnapshotLoaded(snapshot) => {
                if let Some(turn) = turn_from_context_snapshot(&snapshot) {
                    self.turns.push(turn);
                    let latest = self.turns.len().saturating_sub(1);
                    if let Some((_, menu)) = self
                        .turns
                        .get(latest)
                        .and_then(|turn| ask_menu_from_turn(latest, turn))
                    {
                        self.ask_menu = Some(menu);
                    }
                    self.left.stick_to_bottom = true;
                }
            }
            SocketEvent::TurnDelta { message_id, delta } => {
                if let Some(turn) = self.pending_turn_mut(&message_id) {
                    turn.answer.push_str(&delta);
                    turn.footer = "flyflor · streaming".to_string();
                    self.left.stick_to_bottom = true;
                }
            }
            SocketEvent::TurnFinal {
                message_id,
                text,
                metadata,
            } => {
                if let Some(turn_index) = self.resolve_turn_final_index(&message_id) {
                    if let Some(turn) = self.turns.get_mut(turn_index) {
                        turn.answer = turn_final_visible_text(&text, metadata.as_ref());
                        turn.metadata = metadata;
                        turn.context_rows = context_rows_from_metadata(&turn.metadata);
                        turn.footer =
                            format!("flyflor · final · {}", iso8601_from_millis(now_millis()));
                        if let Some(fork_id) = latest_context_fork_id(&turn.metadata) {
                            self.active_context_fork_id = Some(fork_id);
                        }
                        self.left.stick_to_bottom = true;
                    }
                    if let Some((_, menu)) = self
                        .turns
                        .get(turn_index)
                        .and_then(|turn| ask_menu_from_turn(turn_index, turn))
                    {
                        self.ask_menu = Some(menu);
                    }
                }
                let _ = self.socket_tx.send(SocketCommand::ForkMemoryGet);
            }
            SocketEvent::ForkCreated {
                fork_id,
                parent_id,
                root_id,
                summary,
            } => {
                self.pending_fork_create = false;
                let parent_fork_id = if let Some(active_fork) = self.active_fork.clone() {
                    let parent_fork_id = parent_id.or_else(|| self.active_context_fork_id.clone());
                    self.fork_stack.push((active_fork, self.turns.clone()));
                    parent_fork_id
                } else {
                    self.root_turns = self.turns.clone();
                    None
                };
                self.active_context_fork_id = Some(fork_id.clone());
                self.active_fork = Some(ActiveForkSession {
                    fork_id: fork_id.clone(),
                    parent_fork_id,
                    root_id,
                    summary: summary.clone(),
                });
                self.turns.clear();
                self.pending_turns.clear();
                self.chat_render_key = None;
                self.left.initial_scroll_applied = false;
                self.left.stick_to_bottom = true;
                self.right_source.blackboard_status = format!("已进入 fork 对话 · {fork_id}");
                let _ = self.socket_tx.send(SocketCommand::HistoryList {
                    context_fork_id: self.active_context_fork_id.clone(),
                });
                if let Some(summary) = summary {
                    log_event(format!("fork created id={fork_id} summary={summary}"));
                } else {
                    log_event(format!("fork created id={fork_id}"));
                }
            }
            SocketEvent::TurnError {
                message_id,
                message,
            } => {
                log_event(format!(
                    "turn error message_id={message_id} message={message}"
                ));
                if let Some(turn_index) = self.pending_turns.remove(&message_id) {
                    if let Some(turn) = self.turns.get_mut(turn_index) {
                        turn.answer = format!("请求失败：{message}");
                        turn.footer = "flyflor · turn error".to_string();
                        self.left.stick_to_bottom = true;
                    }
                } else {
                    self.right_source.blackboard_status = format!("turn error · {message}");
                }
                self.pending_fork_create = false;
            }
            SocketEvent::Disconnected(message) => {
                log_event(format!("socket disconnected {message}"));
                self.socket_connected = false;
                self.history_status = HistoryStatus::Error;
                self.right_source.blackboard_status = format!("socket unavailable · {message}");
                self.pending_fork_create = false;
                self.fail_pending_turns(&format!("socket unavailable: {message}"));
            }
        }
    }

    fn apply_runtime_event_side_effects(&mut self, event: &Value) -> Option<String> {
        let event_type = runtime_event_type(event)?;
        match event_type {
            "memory.task_plan.written" | "memory.task_plan.decided" => {
                if self.socket_tx.send(SocketCommand::TaskList).is_err() {
                    Some("task refresh failed · socket worker is not running".to_string())
                } else {
                    Some("task plan updated · refreshing".to_string())
                }
            }
            "blackboard.message.appended" => {
                let text = event_text_from_payload(event)
                    .unwrap_or_else(|| "收到 blackboard.message.appended".to_string());
                self.right_source.blackboard_stream.push(format!(
                    "流式记录：{}",
                    truncate_to_width(&text.replace('\n', " "), 120)
                ));
                Some("blackboard 正在更新".to_string())
            }
            "blackboard.turn.end" => {
                let summary = event_text_from_payload(event)
                    .unwrap_or_else(|| "收到 blackboard.turn.end".to_string());
                self.right_source.blackboard_stream.push(format!(
                    "回合结束：{}",
                    truncate_to_width(&summary.replace('\n', " "), 120)
                ));
                Some("blackboard turn 已结束".to_string())
            }
            _ => None,
        }
    }

    fn attach_execution_row_to_active_turn(&mut self) {
        let Some(turn_index) = self
            .pending_turns
            .values()
            .copied()
            .max()
            .or_else(|| self.turns.len().checked_sub(1))
        else {
            return;
        };
        let mut rows = execution_context_rows(&self.run_timeline);
        if rows.is_empty() {
            return;
        }
        let Some(turn) = self.turns.get_mut(turn_index) else {
            return;
        };
        let previous = turn
            .context_rows
            .iter()
            .filter(|row| row.kind == ContextRowKind::Execution)
            .filter_map(|row| execution_row_identity(row).map(|key| (key, row.expanded)))
            .collect::<Vec<_>>();
        for row in &mut rows {
            if let Some(key) = execution_row_identity(row)
                && let Some((_, expanded)) = previous
                    .iter()
                    .find(|(previous_key, _)| previous_key == &key)
            {
                row.expanded = *expanded;
            }
        }
        turn.context_rows
            .retain(|row| row.kind != ContextRowKind::Execution);
        turn.context_rows.extend(rows);
    }

    fn fail_pending_turns(&mut self, message: &str) {
        let pending = mem::take(&mut self.pending_turns);
        for (_, turn_index) in pending {
            if let Some(turn) = self.turns.get_mut(turn_index)
                && turn.answer.trim().is_empty()
            {
                turn.answer = format!("请求失败：{message}");
                turn.footer = "flyflor · send error".to_string();
            }
        }
        self.left.stick_to_bottom = true;
    }

    fn pending_turn_mut(&mut self, message_id: &str) -> Option<&mut Turn> {
        let turn_index = *self.pending_turns.get(message_id)?;
        self.turns.get_mut(turn_index)
    }

    fn resolve_turn_final_index(&mut self, message_id: &str) -> Option<usize> {
        if let Some(turn_index) = self.pending_turns.remove(message_id) {
            return Some(turn_index);
        }
        if self.pending_turns.len() == 1 {
            let Some((pending_id, turn_index)) = self
                .pending_turns
                .iter()
                .map(|(id, index)| (id.clone(), *index))
                .next()
            else {
                return None;
            };
            self.pending_turns.remove(&pending_id);
            log_event(format!(
                "turn final message_id mismatch final={message_id} pending={pending_id}"
            ));
            return Some(turn_index);
        }
        None
    }

    fn is_working(&self) -> bool {
        !self.pending_turns.is_empty()
            || self
                .turns
                .last()
                .is_some_and(|turn| turn.footer.contains("streaming"))
    }

    fn fork_session_label(&self) -> String {
        self.active_fork
            .as_ref()
            .map(|fork| {
                format!(
                    "fork {}",
                    truncate_to_width(fork.summary.as_deref().unwrap_or(&fork.fork_id), 24)
                )
            })
            .unwrap_or_else(|| "root".to_string())
    }

    fn toggle_interaction_mode(&mut self) {
        self.interaction_mode = self.interaction_mode.next();
        self.yolo_mode = self.interaction_mode.yolo();
    }

    fn close_menus(&mut self) {
        if let Some(menu) = &mut self.ask_menu {
            if menu.is_editing_other() {
                self.input.clear();
                self.input_cursor = None;
            }
        }
        if self.ask_menu.is_some() {
            self.ask_menu = None;
            return;
        }
        self.command_palette = None;
        self.plan_menu = None;
    }

    fn input_cursor_index(&self) -> usize {
        self.input_cursor.unwrap_or_else(|| self.input.len())
    }

    fn set_input_cursor(&mut self, index: usize) {
        self.input_cursor = Some(index.min(self.input.len()));
    }

    fn insert_input_char(&mut self, ch: char) {
        let index = self.input_cursor_index().min(self.input.len());
        self.input.insert(index, ch);
        self.set_input_cursor(index + ch.len_utf8());
    }

    fn insert_input_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let index = self.input_cursor_index().min(self.input.len());
        self.input.insert_str(index, text);
        self.set_input_cursor(index + text.len());
    }

    fn backspace_input(&mut self) {
        let index = self.input_cursor_index().min(self.input.len());
        let Some(previous) = self.input[..index]
            .char_indices()
            .last()
            .map(|(index, _)| index)
        else {
            self.set_input_cursor(0);
            return;
        };
        self.input.drain(previous..index);
        self.set_input_cursor(previous);
    }

    fn move_input_cursor_left(&mut self) {
        let index = self.input_cursor_index().min(self.input.len());
        let previous = self.input[..index]
            .char_indices()
            .last()
            .map(|(index, _)| index)
            .unwrap_or(0);
        self.set_input_cursor(previous);
    }

    fn move_input_cursor_right(&mut self) {
        let index = self.input_cursor_index().min(self.input.len());
        if index >= self.input.len() {
            self.set_input_cursor(self.input.len());
            return;
        }
        let next = self.input[index..]
            .chars()
            .next()
            .map(|ch| index + ch.len_utf8())
            .unwrap_or(self.input.len());
        self.set_input_cursor(next);
    }

    fn move_input_cursor_vertical(&mut self, delta: isize) {
        let index = self.input_cursor_index().min(self.input.len());
        let (line_start, column) = input_line_start_and_column(&self.input, index);
        if delta < 0 {
            if line_start == 0 {
                self.set_input_cursor(index);
                return;
            }
            let previous_end = line_start.saturating_sub(1);
            let previous_start = self.input[..previous_end]
                .rfind('\n')
                .map(|position| position + 1)
                .unwrap_or(0);
            self.set_input_cursor(input_index_for_column(
                &self.input,
                previous_start,
                previous_end,
                column,
            ));
        } else {
            let line_end = self.input[index..]
                .find('\n')
                .map(|offset| index + offset)
                .unwrap_or(self.input.len());
            if line_end >= self.input.len() {
                self.set_input_cursor(index);
                return;
            }
            let next_start = line_end + 1;
            let next_end = self.input[next_start..]
                .find('\n')
                .map(|offset| next_start + offset)
                .unwrap_or(self.input.len());
            self.set_input_cursor(input_index_for_column(
                &self.input,
                next_start,
                next_end,
                column,
            ));
        }
    }

    fn refresh_command_palette(&mut self) {
        if self
            .ask_menu
            .as_ref()
            .is_some_and(|menu| menu.is_editing_other())
        {
            self.command_palette = None;
            return;
        }
        if !self.input.starts_with('/') {
            self.command_palette = None;
            return;
        }
        let query = self.input.trim_start_matches('/').to_ascii_lowercase();
        let items = slash_commands_for_fork(self.active_fork.is_some())
            .into_iter()
            .filter(|command| command.name.starts_with(query.as_str()))
            .collect::<Vec<_>>();
        let selected = self
            .command_palette
            .as_ref()
            .map(|palette| palette.selected.min(items.len().saturating_sub(1)))
            .unwrap_or(0);
        self.command_palette = Some(CommandPalette {
            query,
            selected,
            items,
        });
    }

    fn move_active_menu(&mut self, delta: isize) -> bool {
        if let Some(menu) = &mut self.ask_menu {
            return menu.move_choice(delta);
        }
        if let Some(menu) = &mut self.plan_menu {
            menu.selected = move_index(menu.selected, menu.items.len(), delta);
            return true;
        }
        if let Some(menu) = &mut self.command_palette {
            menu.selected = move_index(menu.selected, menu.items.len(), delta);
            return true;
        }
        false
    }

    fn select_active_menu_number(&mut self, ch: char) -> bool {
        let Some(digit) = ch.to_digit(10) else {
            return false;
        };
        if digit == 0 {
            return false;
        }
        if let Some(menu) = &mut self.ask_menu {
            if !menu.select_current_choice(digit as usize - 1) {
                return false;
            }
            if menu.start_current_other_input() {
                self.input.clear();
                self.input_cursor = None;
                self.right_source.blackboard_status = "请输入 ASK Other 回答后发送".to_string();
            }
            return true;
        }
        false
    }

    fn handle_menu_confirm_or_next(&mut self, confirm: bool) -> bool {
        if self.ask_menu.is_some() {
            if self
                .ask_menu
                .as_ref()
                .is_some_and(|menu| menu.is_editing_other())
            {
                if confirm {
                    self.submit_input();
                }
                return true;
            }
            if confirm {
                self.confirm_ask_menu_selection();
            } else {
                self.advance_ask_menu_or_submit();
            }
            return true;
        }
        if self.plan_menu.is_some() {
            if confirm {
                self.confirm_plan_menu_selection();
            } else {
                self.move_active_menu(1);
            }
            return true;
        }
        if self.command_palette.is_some() {
            if confirm {
                self.confirm_command_palette_selection();
            } else {
                self.complete_command_palette_selection();
            }
            return true;
        }
        false
    }

    fn active_menu_lines(&self, theme: &Theme) -> Option<Vec<Line<'static>>> {
        if let Some(menu) = &self.ask_menu {
            return Some(render_ask_menu_lines(menu, theme));
        }
        if let Some(menu) = &self.plan_menu {
            return Some(render_plan_menu_lines(menu, theme));
        }
        self.command_palette
            .as_ref()
            .map(|menu| render_command_palette_lines(menu, theme))
    }

    fn complete_command_palette_selection(&mut self) {
        let Some(command) = self
            .command_palette
            .as_ref()
            .and_then(|menu| menu.items.get(menu.selected))
            .copied()
        else {
            return;
        };
        self.input = format!("/{}", command.name);
        self.input_cursor = None;
        self.refresh_command_palette();
    }

    fn confirm_command_palette_selection(&mut self) {
        if self
            .ask_menu
            .as_ref()
            .is_some_and(|menu| menu.is_editing_other())
        {
            self.command_palette = None;
            self.right_source.blackboard_status =
                "请先完成当前 ASK Other 回答，Esc 返回选项".to_string();
            return;
        }
        let Some(command) = self
            .command_palette
            .as_ref()
            .and_then(|menu| menu.items.get(menu.selected))
            .copied()
        else {
            let command = self.input.clone();
            self.command_palette = None;
            self.right_source.blackboard_status = format!("未知命令：{command}");
            return;
        };
        self.command_palette = None;
        self.input.clear();
        self.input_cursor = None;
        self.execute_slash_command(command.kind);
    }

    fn execute_slash_command(&mut self, command: SlashCommandKind) {
        match command {
            SlashCommandKind::Exit => self.should_quit = true,
            SlashCommandKind::ExitFork => self.exit_fork_session(),
            SlashCommandKind::Help => {
                self.right_source.blackboard_status =
                    "命令：/help /yolo /model /status /history /fork /ask /blackboard /todo /memory · fork 中可用 /exit fork · /yolo 危险：外模式会绕过沙箱/高权限执行"
                        .to_string();
            }
            SlashCommandKind::Yolo => {
                self.interaction_mode = if self.interaction_mode == InteractionMode::Yolo {
                    InteractionMode::Act
                } else {
                    InteractionMode::Yolo
                };
                self.yolo_mode = self.interaction_mode.yolo();
                self.right_source.blackboard_status = if self.yolo_mode {
                    "YOLO 已开启：外模式/高权限，可能绕过沙箱执行".to_string()
                } else {
                    "YOLO 已关闭：恢复普通权限模式".to_string()
                };
            }
            SlashCommandKind::Model => {
                self.right_source.blackboard_status = format!(
                    "模型只读 · 上下文窗口={} · 最大输出={}",
                    self.model_context_window_tokens
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "未收到上下文窗口".to_string()),
                    self.max_output_tokens
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "暂无数据".to_string())
                );
            }
            SlashCommandKind::Status => {
                self.right_source.blackboard_status = "已请求刷新 status".to_string();
                if self.socket_tx.send(SocketCommand::StatusGet).is_err() {
                    self.right_source.blackboard_status =
                        "status refresh failed · socket worker is not running".to_string();
                }
            }
            SlashCommandKind::History => {
                self.right_source.blackboard_status = "已请求刷新历史".to_string();
                if self
                    .socket_tx
                    .send(SocketCommand::HistoryList {
                        context_fork_id: self.active_context_fork_id.clone(),
                    })
                    .is_err()
                {
                    self.right_source.blackboard_status =
                        "history refresh failed · socket worker is not running".to_string();
                }
            }
            SlashCommandKind::Fork => {
                if let Some(index) = self.latest_assistant_turn_index() {
                    if let Some(command) = fork_create_command_from_turn(
                        &self.turns[index],
                        &self.active_context_fork_id,
                    ) {
                        if self.socket_tx.send(command).is_err() {
                            self.right_source.blackboard_status =
                                "fork create failed · socket worker is not running".to_string();
                        } else {
                            self.mark_fork_create_pending();
                        }
                    }
                } else {
                    self.right_source.blackboard_status = "暂无可创建 fork 的回答".to_string();
                }
            }
            SlashCommandKind::Ask => {
                if !self.open_latest_ask_menu() {
                    self.right_source.blackboard_status = "暂无待回答 ASK".to_string();
                }
            }
            SlashCommandKind::Blackboard => {
                self.right_source.blackboard_status = latest_context_summary(
                    &self.turns,
                    ContextRowKind::Blackboard,
                    "暂无 blackboard 摘要",
                );
            }
            SlashCommandKind::Todo => {
                if self.plan_state() == PlanState::AwaitingConfirmation {
                    self.open_plan_menu();
                    self.right_source.blackboard_status = "请选择计划操作".to_string();
                } else {
                    self.right_source.blackboard_status = "已请求刷新 TODO".to_string();
                    if self.socket_tx.send(SocketCommand::TaskList).is_err() {
                        self.right_source.blackboard_status =
                            "TODO refresh failed · socket worker is not running".to_string();
                    }
                }
            }
            SlashCommandKind::Memory => {
                self.right_source.blackboard_status =
                    latest_context_summary(&self.turns, ContextRowKind::Recall, "暂无回忆摘要");
            }
        }
    }

    fn exit_fork_session(&mut self) {
        let Some(fork) = self.active_fork.take() else {
            self.right_source.blackboard_status = "当前不在 fork 对话".to_string();
            return;
        };
        self.active_context_fork_id = fork.parent_fork_id;
        if let Some((parent_fork, parent_turns)) = self.fork_stack.pop() {
            self.active_context_fork_id = Some(parent_fork.fork_id.clone());
            self.active_fork = Some(parent_fork);
            self.turns = parent_turns;
            self.right_source.blackboard_status = "已退出当前 fork，回到父级 fork 对话".to_string();
        } else if !self.root_turns.is_empty() {
            self.turns = std::mem::take(&mut self.root_turns);
            self.right_source.blackboard_status = "已退出 fork，回到父级/root 对话".to_string();
        } else {
            self.right_source.blackboard_status = "已退出 fork，回到父级/root 对话".to_string();
        }
        self.pending_turns.clear();
        self.chat_render_key = None;
        self.left.initial_scroll_applied = false;
        self.left.stick_to_bottom = true;
    }

    fn latest_assistant_turn_index(&self) -> Option<usize> {
        self.turns
            .iter()
            .enumerate()
            .rev()
            .find(|(_, turn)| !turn.answer.trim().is_empty())
            .map(|(index, _)| index)
    }

    fn open_latest_ask_menu(&mut self) -> bool {
        let Some((turn_index, menu)) = self
            .turns
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, turn)| ask_menu_from_turn(index, turn))
        else {
            return false;
        };
        self.ask_menu = Some(menu);
        self.right_source.blackboard_status = format!("ASK 选项来自 turn {turn_index}");
        true
    }

    fn mark_fork_create_pending(&mut self) {
        self.pending_fork_create = true;
        self.right_source.blackboard_status = "fork 创建中...".to_string();
    }

    fn confirm_ask_menu_selection(&mut self) {
        let Some(menu) = self.ask_menu.as_mut() else {
            return;
        };
        if menu.start_current_other_input() {
            self.input.clear();
            self.input_cursor = None;
            self.right_source.blackboard_status = "请输入 ASK Other 回答后发送".to_string();
            return;
        }
        let Some(menu) = self.ask_menu.take() else {
            return;
        };
        self.submit_or_advance_ask_menu(menu);
    }

    fn advance_ask_menu_or_submit(&mut self) {
        let Some(menu) = self.ask_menu.take() else {
            return;
        };
        self.submit_or_advance_ask_menu(menu);
    }

    fn submit_or_advance_ask_menu(&mut self, mut menu: AskMenu) {
        if menu.advance_question() {
            self.ask_menu = Some(menu);
            return;
        }
        let answers = menu
            .answers()
            .into_iter()
            .map(AskAnswer::from)
            .collect::<Vec<_>>();
        self.send_ask_answers(menu.turn_index, answers, menu.continuation);
    }

    fn open_plan_menu(&mut self) {
        self.plan_menu = Some(default_plan_menu());
    }

    fn confirm_plan_menu_selection(&mut self) {
        let Some(menu) = self.plan_menu.take() else {
            return;
        };
        let Some(item) = menu.items.get(menu.selected).cloned() else {
            return;
        };
        match item.action {
            PlanAction::Confirm | PlanAction::Abandon => self.send_plan_command(item.action, None),
            PlanAction::Revise => {
                self.pending_plan_action = Some(PlanPendingAction::Revise);
                self.input.clear();
                self.input_cursor = None;
                self.right_source.blackboard_status = "请输入计划补充后发送".to_string();
            }
        }
    }

    fn send_plan_command(&mut self, action: PlanAction, revision: Option<String>) {
        let Some(plan_id) = self.active_plan_id() else {
            self.right_source.blackboard_status = "暂无待确认计划 id".to_string();
            return;
        };
        if self
            .socket_tx
            .send(SocketCommand::TaskPlanDecide {
                plan_id: plan_id.clone(),
                action,
                revision,
            })
            .is_err()
        {
            self.right_source.blackboard_status =
                "task.plan.decide failed · socket worker is not running".to_string();
            return;
        }
        self.right_source.blackboard_status =
            format!("已发送计划决策：{} · {plan_id}", action.as_str());
    }

    fn send_ask_answers(
        &mut self,
        turn_index: usize,
        ask_answers: Vec<AskAnswer>,
        continuation: Value,
    ) {
        if ask_answers.is_empty() {
            return;
        }
        let message_id = format!("flyflor-cli-message-{}", now_millis());
        let new_turn_index = self.turns.len();
        let socket_connected = self.socket_connected;
        let metadata = tui::ask::command::ask_message_metadata_many(continuation, &ask_answers);
        let user_text = ask_answers
            .iter()
            .map(|answer| answer.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        self.turns.push(Turn {
            message_id: Some(message_id.clone()),
            event_id: None,
            user: user_text.clone(),
            thought: None,
            answer: String::new(),
            metadata: None,
            context_rows: context_rows_from_metadata(&None),
            pending_continuation: None,
            footer: if socket_connected {
                format!("flyflor · ask answer · source turn {turn_index}")
            } else {
                "flyflor · send error".to_string()
            },
        });
        if !socket_connected {
            if let Some(turn) = self.turns.get_mut(new_turn_index) {
                turn.answer = format!("请求失败：socket unavailable · {}", ws_url());
            }
            self.right_source.blackboard_status = format!("socket unavailable · {}", ws_url());
            self.left.stick_to_bottom = true;
            return;
        }
        self.pending_turns
            .insert(message_id.clone(), new_turn_index);
        if self
            .socket_tx
            .send(SocketCommand::SendMessage {
                message_id,
                text: user_text,
                context_fork_id: self.active_context_fork_id.clone(),
                metadata: Some(metadata),
                mode: self.interaction_mode,
                yolo: self.yolo_mode,
            })
            .is_err()
        {
            if let Some(turn) = self.turns.get_mut(new_turn_index) {
                turn.answer = "请求失败：socket worker is not running".to_string();
                turn.footer = "flyflor · send error".to_string();
            }
        }
        self.left.stick_to_bottom = true;
    }

    fn update_left_viewport(&mut self, area: Rect, theme: &Theme) {
        let width = area.width.max(1) as usize;
        let phase = working_light_phase(now_millis());
        let key = chat_render_key(&self.turns, width, phase);
        if self.chat_render_key != Some(key) {
            let render = render_turns(&self.turns, width, theme, phase);
            self.chat_lines = render.lines;
            self.chat_context_regions = render.context_row_regions;
            self.chat_render_key = Some(key);
        }
        update_scroll_state_from_rendered(&self.chat_lines, &mut self.left, area);
        self.context_row_hitboxes =
            visible_context_hitboxes(&self.chat_context_regions, self.left.scroll, area);
    }

    fn update_right_viewport(&mut self, todo_area: Rect) {
        let data = self.current_right_panel_data();
        let todos = self.visible_todos();
        self.right_sections =
            render_right_panel_sections(&data, &todos, todo_area.width.max(1) as usize);
        self.right_lines =
            flatten_right_panel_sections(scrollable_right_sections(&self.right_sections));
        if let Some(todo) = self.right_sections.first() {
            update_scroll_state(&todo.lines, &mut self.right, todo_area);
        } else {
            self.right = ScrollState::default();
        }
    }

    fn visible_todos(&self) -> Vec<TodoItem> {
        if let Some(task_todos) = &self.task_todos {
            if task_todos.is_empty() {
                vec![TodoItem::empty_plan()]
            } else {
                task_todos.clone()
            }
        } else if !todos_from_turns(&self.turns).is_empty() {
            todos_from_turns(&self.turns)
        } else if !self.todos.is_empty() {
            self.todos.clone()
        } else {
            vec![TodoItem::empty_plan()]
        }
    }

    fn active_plan_id(&self) -> Option<String> {
        self.active_confirmation_plan_id()
            .or_else(|| {
                self.task_todos
                    .as_ref()
                    .and_then(|todos| todos.iter().find_map(|todo| todo.plan_id.clone()))
            })
            .or_else(|| self.todos.iter().find_map(|todo| todo.plan_id.clone()))
            .or_else(|| {
                self.turns
                    .iter()
                    .filter_map(|turn| turn.metadata.as_ref())
                    .find_map(plan_id_from_metadata)
            })
    }

    fn active_confirmation_plan_id(&self) -> Option<String> {
        self.task_todos
            .as_ref()
            .and_then(|todos| plan_id_waiting_for_confirmation(todos))
            .or_else(|| plan_id_waiting_for_confirmation(&self.todos))
            .or_else(|| plan_id_waiting_for_confirmation(&todos_from_turns(&self.turns)))
    }

    fn plan_state(&self) -> PlanState {
        let todos = self.visible_todos();
        plan_state_from_todos(&todos)
    }

    fn current_right_panel_data(&self) -> RightPanelData {
        let mut data = self.right_source.clone();
        data.fork_memory = self.fork_memory.clone();
        data.thinking_label = if self.interaction_mode == InteractionMode::Yolo {
            "YOLO".to_string()
        } else if self.pending_fork_create {
            "fork 创建中".to_string()
        } else if self.is_working() {
            "接收中".to_string()
        } else {
            self.interaction_mode.label().to_string()
        };
        data.model_stats = vec![
            StatItem {
                label: "mode".to_string(),
                value: self.interaction_mode.label().to_string(),
            },
            StatItem {
                label: "权限".to_string(),
                value: if self.yolo_mode {
                    "YOLO 外模式/高权限".to_string()
                } else {
                    "普通模式".to_string()
                },
            },
            StatItem {
                label: "model".to_string(),
                value: self
                    .model_name
                    .clone()
                    .unwrap_or_else(|| "未知模型".to_string()),
            },
            StatItem {
                label: "provider".to_string(),
                value: self
                    .model_provider
                    .clone()
                    .unwrap_or_else(|| "暂无数据".to_string()),
            },
        ];
        let context = estimate_context_window(
            &self.turns,
            &self.active_context_fork_id,
            &StatusSnapshot {
                context_window_tokens: self.model_context_window_tokens,
                max_output_tokens: self.max_output_tokens,
                hot_context_tokens: self.hot_context_tokens,
                context_window_percent: self.context_window_percent,
                context_status: self.context_status.clone(),
                remaining_context_tokens: self.remaining_context_tokens,
                cache_read_tokens: self.cache_read_tokens,
                cache_write_tokens: self.cache_write_tokens,
                model_name: self.model_name.clone(),
                model_provider: self.model_provider.clone(),
            },
        );
        data.context_total = context.total;
        data.context_percent = context.percent;
        data.context_bar = context.bar;
        data.context_usage = context.usage;
        data.context_ratio = context.ratio;
        data.context_used_tokens = context.used_tokens;
        data.context_max_tokens = context.max_tokens;
        data.context_used = compact_token_count(context.used_tokens);
        data.context_max = context
            .max_tokens
            .map(compact_token_count)
            .unwrap_or_else(|| "未知".to_string());
        data.run_timeline = self.run_timeline.clone();
        data.footer = "Shift+Tab 切换模式".to_string();
        data
    }

    fn should_copy_with_y(&self) -> bool {
        !self.input_context_active() && self.selection_has_content()
    }

    fn scroll_left_by(&mut self, delta: isize) {
        apply_scroll_delta(&mut self.left, delta);
    }

    fn scroll_right_todo_by(&mut self, delta: isize) {
        apply_scroll_delta(&mut self.right, delta);
    }

    fn drag_scroll_left(&mut self, anchor_scroll: usize, delta_rows: isize) {
        drag_scroll(&mut self.left, anchor_scroll, delta_rows);
    }

    fn drag_scroll_right_todo(&mut self, anchor_scroll: usize, delta_rows: isize) {
        drag_scroll(&mut self.right, anchor_scroll, delta_rows);
    }

    fn is_right_todo_scroll_hit(&self, x: u16, y: u16) -> bool {
        self.right.scrollbar.contains(x, y)
            || self
                .right_todo_area()
                .is_some_and(|area| in_rect(x, y, area))
    }

    fn right_todo_area(&self) -> Option<Rect> {
        let inner = self.layout.right_panel.inner(Margin {
            vertical: 1,
            horizontal: 2,
        });
        if inner.width < 24 || inner.height < 14 {
            return None;
        }
        let data = self.current_right_panel_data();
        let text_width = inner.width.saturating_sub(2).max(1);
        let bottom_height = right_bottom_height(&data, text_width as usize, inner.height);
        Some(right_todo_body_area(
            right_panel_layout(inner, bottom_height).todo,
        ))
    }

    fn toggle_context_row_at(&mut self, x: u16, y: u16) -> bool {
        let Some(hit) = self
            .context_row_hitboxes
            .iter()
            .find(|hitbox| in_rect(x, y, hitbox.rect))
            .copied()
        else {
            return false;
        };
        let Some(row) = self
            .turns
            .get_mut(hit.turn_index)
            .and_then(|turn| turn.context_rows.get_mut(hit.row_index))
        else {
            return false;
        };
        match row.kind {
            ContextRowKind::Recall
            | ContextRowKind::Thought
            | ContextRowKind::Fork
            | ContextRowKind::Execution
            | ContextRowKind::Blackboard => {
                row.expanded = !row.expanded;
                self.left.stick_to_bottom = false;
            }
            ContextRowKind::AskResume => {
                if let Some(turn) = self.turns.get(hit.turn_index) {
                    if let Some((_, menu)) = ask_menu_from_turn(hit.turn_index, turn) {
                        self.ask_menu = Some(menu);
                    } else {
                        self.resend_turn_with_continuation(hit.turn_index);
                    }
                }
            }
            ContextRowKind::CreateFork => {
                let Some(turn) = self.turns.get(hit.turn_index) else {
                    return false;
                };
                if let Some(command) =
                    fork_create_command_from_turn(turn, &self.active_context_fork_id)
                {
                    if self.socket_tx.send(command).is_err() {
                        self.right_source.blackboard_status =
                            "fork create failed · socket worker is not running".to_string();
                    } else {
                        self.mark_fork_create_pending();
                    }
                }
            }
        }
        true
    }

    fn resend_turn_with_continuation(&mut self, turn_index: usize) {
        let Some(turn) = self.turns.get(turn_index) else {
            return;
        };
        let Some(continuation) = continuation_from_turn(turn) else {
            self.right_source.blackboard_status = "no ASK continuation on this turn".to_string();
            return;
        };
        let text = turn.user.trim().to_string();
        if text.is_empty() {
            return;
        }

        let message_id = format!("flyflor-cli-message-{}", now_millis());
        let new_turn_index = self.turns.len();
        let socket_connected = self.socket_connected;
        self.turns.push(Turn {
            message_id: Some(message_id.clone()),
            event_id: None,
            user: text.clone(),
            thought: None,
            answer: String::new(),
            metadata: None,
            context_rows: context_rows_from_metadata(&None),
            pending_continuation: None,
            footer: if socket_connected {
                "flyflor · resending".to_string()
            } else {
                "flyflor · send error".to_string()
            },
        });
        if !socket_connected {
            if let Some(turn) = self.turns.get_mut(new_turn_index) {
                turn.answer = format!("请求失败：socket unavailable · {}", ws_url());
            }
            self.right_source.blackboard_status = format!("socket unavailable · {}", ws_url());
            self.left.stick_to_bottom = true;
            return;
        }
        self.pending_turns
            .insert(message_id.clone(), new_turn_index);
        if self
            .socket_tx
            .send(SocketCommand::SendMessage {
                message_id,
                text,
                context_fork_id: self.active_context_fork_id.clone(),
                metadata: Some(json!({ "continuation": continuation })),
                mode: self.interaction_mode,
                yolo: self.yolo_mode,
            })
            .is_err()
        {
            if let Some(turn) = self.turns.get_mut(new_turn_index) {
                turn.answer = "请求失败：socket worker is not running".to_string();
                turn.footer = "flyflor · send error".to_string();
            }
        }
        self.left.stick_to_bottom = true;
    }

    fn paste_from_clipboard(&mut self) {
        let tx = self.clipboard_tx.clone();
        thread::spawn(move || {
            let result = read_clipboard_text();
            let _ = tx.send(result);
        });
    }

    fn handle_ctrl_c(&mut self) {
        if self.selection_has_content() {
            self.copy_selection_to_clipboard();
            return;
        }
        if !self.input.is_empty() {
            self.input.clear();
            self.input_cursor = None;
        }
        self.composer_notice = Some(ComposerNotice::ExitHint);
    }

    fn begin_selection_at(&mut self, x: u16, y: u16) -> bool {
        let Some(point) = self.selection_point_from_position(x, y) else {
            self.selection.clear();
            return false;
        };
        self.selection.anchor = Some(point);
        self.selection.head = Some(point);
        self.selection.dragging = true;
        true
    }

    fn update_selection_at(&mut self, x: u16, y: u16) {
        if !self.selection.dragging {
            return;
        }
        if let Some(point) = self.selection_point_from_position(x, y) {
            self.selection.head = Some(point);
        }
    }

    fn input_context_active(&self) -> bool {
        !self.input.is_empty()
            || self.command_palette.is_some()
            || self.ask_menu.is_some()
            || self.plan_menu.is_some()
    }

    fn finish_selection_at(&mut self, x: u16, y: u16) {
        if !self.selection.dragging {
            return;
        }
        self.update_selection_at(x, y);
        self.selection.dragging = false;
        if self.selection_has_content() {
            self.copy_selection_to_clipboard();
        }
    }

    fn selection_point_from_position(&self, x: u16, y: u16) -> Option<SelectionPoint> {
        self.left_selection_point_from_position(x, y)
            .or_else(|| self.right_selection_point_from_position(x, y))
    }

    fn left_selection_point_from_position(&self, x: u16, y: u16) -> Option<SelectionPoint> {
        let area = self.layout.left_panel.inner(Margin {
            vertical: 1,
            horizontal: 1,
        });
        let text_area = Rect::new(area.x, area.y, area.width.saturating_sub(2), area.height);
        if !in_rect(x, y, text_area) {
            return None;
        }
        let visible_row = y.saturating_sub(text_area.y) as usize;
        Some(SelectionPoint {
            target: SelectionTarget::Left,
            line_index: self.left.scroll + visible_row,
            column: x.saturating_sub(text_area.x) as usize,
        })
    }

    fn right_selection_point_from_position(&self, x: u16, y: u16) -> Option<SelectionPoint> {
        let inner = self.layout.right_panel.inner(Margin {
            vertical: 1,
            horizontal: 2,
        });
        if inner.width < 24 || inner.height < 14 {
            return None;
        }
        let data = self.current_right_panel_data();
        let text_width = inner.width.saturating_sub(2).max(1);
        let bottom_height = right_bottom_height(&data, text_width as usize, inner.height);
        let layout = right_panel_layout(inner, bottom_height);
        let text_area = layout.bottom_text;
        if !in_rect(x, y, text_area) {
            return None;
        }
        let visible_row = y.saturating_sub(text_area.y) as usize;
        Some(SelectionPoint {
            target: SelectionTarget::Right,
            line_index: visible_row,
            column: x.saturating_sub(text_area.x) as usize,
        })
    }

    fn selection_has_content(&self) -> bool {
        let Some((start, end)) = self.selection.ordered_endpoints() else {
            return false;
        };
        (start.line_index, start.column) != (end.line_index, end.column)
    }

    fn copy_selection_to_clipboard(&mut self) {
        let Some(text) = self.selection_to_text() else {
            return;
        };
        match write_text_to_clipboard(&text) {
            Ok(()) => {
                self.right_source.blackboard_status = "selection copied".to_string();
                log_event(format!("selection copied chars={}", text.chars().count()));
            }
            Err(error) => {
                self.right_source.blackboard_status = format!("copy failed · {error}");
                log_event(format!("selection copy failed {error}"));
            }
        }
    }

    fn selection_to_text(&self) -> Option<String> {
        let (start, end) = self.selection.ordered_endpoints()?;
        if !self.selection_has_content() {
            return None;
        }
        let mut rows = Vec::new();
        let lines = match start.target {
            SelectionTarget::Left => &self.chat_lines,
            SelectionTarget::Right => &self.right_lines,
        };
        for line_index in start.line_index..=end.line_index {
            let line = lines.get(line_index)?;
            let plain = line_plain_text(line);
            let start_col = if line_index == start.line_index {
                start.column
            } else {
                0
            };
            let end_col = if line_index == end.line_index {
                end.column
            } else {
                usize::MAX
            };
            rows.push(
                slice_display_columns(&plain, start_col, end_col)
                    .trim_end()
                    .to_string(),
            );
        }
        let text = rows.join("\n");
        Some(match start.target {
            SelectionTarget::Left => strip_transcript_rails(&text),
            SelectionTarget::Right => text,
        })
    }

    fn selected_chat_lines(&self, theme: &Theme) -> Vec<Line<'static>> {
        let mut lines = visible_line_slice(
            &self.chat_lines,
            self.left.scroll,
            self.left.viewport_height,
        );
        apply_selection_to_lines(
            &mut lines,
            self.left.scroll,
            self.selection,
            SelectionTarget::Left,
            theme,
        );
        lines
    }

    fn visible_chat_lines(&self, theme: &Theme) -> Vec<Line<'static>> {
        if self.selection_has_content() {
            return self.selected_chat_lines(theme);
        }
        visible_line_slice(
            &self.chat_lines,
            self.left.scroll,
            self.left.viewport_height,
        )
    }

    fn selected_right_lines(&self, theme: &Theme) -> Vec<Line<'static>> {
        let mut lines = visible_line_slice(&self.right_lines, 0, self.right.viewport_height);
        apply_selection_to_lines(&mut lines, 0, self.selection, SelectionTarget::Right, theme);
        lines
    }

    fn visible_right_lines(&self, theme: &Theme, height: usize) -> Vec<Line<'static>> {
        if self.selection_has_content() {
            return self.selected_right_lines(theme);
        }
        visible_line_slice(&self.right_lines, 0, height)
    }

    fn submit_input(&mut self) {
        let text = self.input.trim().to_string();
        if let Some(mut menu) = self.ask_menu.take_if(|menu| menu.is_editing_other()) {
            if text.is_empty() {
                self.ask_menu = Some(menu);
                return;
            }
            menu.set_current_freeform(text.clone());
            menu.editing_other = false;
            self.input.clear();
            self.input_cursor = None;
            self.submit_or_advance_ask_menu(menu);
            return;
        }
        if text.is_empty() {
            return;
        }
        if matches!(self.pending_plan_action, Some(PlanPendingAction::Revise)) {
            self.pending_plan_action = None;
            self.send_plan_command(PlanAction::Revise, Some(text));
            self.input.clear();
            self.input_cursor = None;
            return;
        }
        if text == "/exit" {
            self.should_quit = true;
            return;
        }
        if text == "/exit fork" {
            if self.active_fork.is_some() {
                self.exit_fork_session();
            } else {
                self.right_source.blackboard_status = "当前不在 fork 对话".to_string();
            }
            self.input.clear();
            self.input_cursor = None;
            return;
        }
        if text.starts_with('/') {
            self.refresh_command_palette();
            if self.command_palette.is_some() {
                self.confirm_command_palette_selection();
            } else {
                self.right_source.blackboard_status = format!("未知命令：{text}");
                self.input.clear();
                self.input_cursor = None;
            }
            return;
        }
        self.composer_notice = None;

        let message_id = format!("flyflor-cli-message-{}", now_millis());
        if !self.socket_connected {
            log_event(format!("send blocked: socket unavailable url={}", ws_url()));
            self.history_status = HistoryStatus::Error;
            self.right_source.blackboard_status = format!("socket unavailable · {}", ws_url());
            return;
        }
        let turn_index = self.turns.len();
        self.turns.push(Turn {
            message_id: Some(message_id.clone()),
            event_id: None,
            user: text.clone(),
            thought: None,
            answer: String::new(),
            metadata: None,
            context_rows: context_rows_from_metadata(&None),
            pending_continuation: None,
            footer: "flyflor · sending".to_string(),
        });
        self.pending_turns.insert(message_id.clone(), turn_index);
        if self
            .socket_tx
            .send(SocketCommand::SendMessage {
                message_id,
                text,
                context_fork_id: self.active_context_fork_id.clone(),
                metadata: None,
                mode: self.interaction_mode,
                yolo: self.yolo_mode,
            })
            .is_err()
        {
            log_event("send failed: socket worker channel closed");
            if let Some(turn) = self.turns.get_mut(turn_index) {
                turn.answer = "请求失败：socket worker is not running".to_string();
                turn.footer = "flyflor · send error".to_string();
            }
        }
        self.input.clear();
        self.input_cursor = None;
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

    state.scrollbar = compute_scrollbar_inset(area, state.scroll, state.max_scroll, 0, 2);
}

fn visible_context_hitboxes(
    regions: &[ContextRowRegion],
    scroll: usize,
    area: Rect,
) -> Vec<ContextRowHitbox> {
    let hit_width = area.width.min(8).max(1);
    regions
        .iter()
        .filter_map(|region| {
            let visible_index = region.line_index.checked_sub(scroll)?;
            if visible_index >= area.height as usize {
                return None;
            }
            Some(ContextRowHitbox {
                turn_index: region.turn_index,
                row_index: region.row_index,
                rect: Rect::new(area.x, area.y + visible_index as u16, hit_width, 1),
            })
        })
        .collect()
}

fn visible_line_slice(lines: &[Line<'static>], scroll: usize, height: usize) -> Vec<Line<'static>> {
    if height == 0 {
        return Vec::new();
    }
    lines.iter().skip(scroll).take(height).cloned().collect()
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
        state.scroll = 0;
        state.initial_scroll_applied = true;
    } else if state.stick_to_bottom {
        state.scroll = state.max_scroll;
    } else {
        state.scroll = state.max_scroll.saturating_sub(offset_from_bottom);
    }

    state.scrollbar = compute_scrollbar_inset(area, state.scroll, state.max_scroll, 0, 1);
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

fn compute_scrollbar_inset(
    area: Rect,
    scroll: usize,
    max_scroll: usize,
    right_inset: u16,
    hit_width: u16,
) -> ScrollbarGeometry {
    let track_height = area.height;
    let thumb_height = 1;
    let travel = track_height.saturating_sub(thumb_height);
    let thumb_top = if max_scroll == 0 || travel == 0 {
        area.y
    } else {
        area.y + ((scroll as f32 / max_scroll as f32) * travel as f32).round() as u16
    };
    let x = area
        .x
        .saturating_add(area.width.saturating_sub(1).saturating_sub(right_inset));
    ScrollbarGeometry {
        x,
        track_top: area.y,
        track_height,
        thumb_top,
        thumb_height,
        hit_area: Rect::new(
            x.saturating_add(1).saturating_sub(hit_width),
            area.y,
            hit_width,
            area.height,
        ),
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

fn render_turns(turns: &[Turn], width: usize, theme: &Theme, phase: usize) -> LeftRender {
    let mut lines = Vec::new();
    let mut context_row_regions = Vec::new();
    let mut thought_regions = Vec::new();
    let mut block_regions = Vec::new();

    for (turn_index, turn) in turns.iter().enumerate() {
        if turn_index > 0 {
            lines.push(empty_content_line(width, theme));
        }

        let user_start = lines.len();
        lines.extend(render_user_block(&turn.user, width, theme));
        let user_end = lines.len().saturating_sub(1);
        block_regions.push(BlockRegion {
            kind: BlockKind::User,
            turn_index,
            start_line: user_start,
            end_line: user_end,
        });
        lines.push(thread_line(Line::raw(""), width, theme, ThreadTone::Rail));

        render_context_rows_for_turn(
            turn,
            turn_index,
            width,
            theme,
            |row| row.kind != ContextRowKind::Execution,
            &mut lines,
            &mut context_row_regions,
            &mut block_regions,
        );

        if let Some(thought) = &turn.thought {
            let row = ContextRow {
                kind: ContextRowKind::Recall,
                summary: thought_summary(thought),
                detail: thought.content.clone(),
                expanded: thought.expanded,
            };
            let line_index = lines.len();
            let block_start = line_index;
            lines.push(render_context_row_header_with_phase(
                &row, width, theme, phase,
            ));
            thought_regions.push(ContextRowRegion {
                turn_index,
                row_index: 0,
                line_index,
            });
            if thought.expanded {
                lines.push(thread_line(Line::raw(""), width, theme, ThreadTone::Rail));
                for line in render_markdown_block(
                    &thought.content,
                    thread_body_width(width, theme, ThreadTone::Thought),
                    theme,
                    MarkdownTone::Thought,
                ) {
                    lines.push(thread_line(line, width, theme, ThreadTone::Thought));
                }
            }
            block_regions.push(BlockRegion {
                kind: BlockKind::Thought,
                turn_index,
                start_line: block_start,
                end_line: lines.len().saturating_sub(1),
            });
        }

        let answer_start = lines.len();
        for line in render_markdown_block(
            &turn.answer,
            thread_body_width(width, theme, ThreadTone::Rail),
            theme,
            MarkdownTone::Answer,
        ) {
            lines.push(thread_line(line, width, theme, ThreadTone::Rail));
        }
        if lines.len() > answer_start {
            block_regions.push(BlockRegion {
                kind: BlockKind::Answer,
                turn_index,
                start_line: answer_start,
                end_line: lines.len().saturating_sub(1),
            });
        }
        render_context_rows_for_turn(
            turn,
            turn_index,
            width,
            theme,
            |row| row.kind == ContextRowKind::Execution,
            &mut lines,
            &mut context_row_regions,
            &mut block_regions,
        );
        if !turn.footer.trim().is_empty() {
            let footer_start = lines.len();
            lines.push(render_footer_line(&turn.footer, width, theme));
            block_regions.push(BlockRegion {
                kind: BlockKind::Footer,
                turn_index,
                start_line: footer_start,
                end_line: lines.len().saturating_sub(1),
            });
        }
    }

    LeftRender {
        lines,
        context_row_regions,
        thought_regions,
        block_regions,
    }
}

fn render_context_rows_for_turn(
    turn: &Turn,
    turn_index: usize,
    width: usize,
    theme: &Theme,
    include: impl Fn(&ContextRow) -> bool,
    lines: &mut Vec<Line<'static>>,
    context_row_regions: &mut Vec<ContextRowRegion>,
    block_regions: &mut Vec<BlockRegion>,
) {
    for (row_index, row) in turn.context_rows.iter().enumerate() {
        if !include(row) {
            continue;
        }
        let line_index = lines.len();
        let block_start = line_index;
        lines.push(render_context_row_header(row, width, theme));
        context_row_regions.push(ContextRowRegion {
            turn_index,
            row_index,
            line_index,
        });
        if row.expanded {
            lines.push(thread_line(Line::raw(""), width, theme, ThreadTone::Rail));
            for line in render_markdown_block(
                &row.detail,
                thread_body_width(width, theme, ThreadTone::Thought),
                theme,
                MarkdownTone::Thought,
            ) {
                lines.push(thread_line(line, width, theme, ThreadTone::Thought));
            }
        }
        block_regions.push(BlockRegion {
            kind: BlockKind::Thought,
            turn_index,
            start_line: block_start,
            end_line: lines.len().saturating_sub(1),
        });
    }
}

fn chat_render_key(turns: &[Turn], width: usize, phase: usize) -> ChatRenderKey {
    let last_turn_hash = turns.iter().fold(0u64, |acc, turn| {
        acc.wrapping_mul(16777619)
            .wrapping_add(hash_turn_render_inputs(turn))
    });
    let expanded_hash = turns.iter().fold(0u64, |acc, turn| {
        let row_hash = turn.context_rows.iter().fold(0u64, |row_acc, row| {
            let mut row_hash = row_acc
                .wrapping_mul(31)
                .wrapping_add(u64::from(row.expanded));
            for byte in row.summary.as_bytes().iter().chain(row.detail.as_bytes()) {
                row_hash ^= u64::from(*byte);
                row_hash = row_hash.wrapping_mul(1099511628211);
            }
            row_hash
        });
        acc.wrapping_mul(131)
            .wrapping_add(row_hash)
            .wrapping_add(u64::from(
                turn.thought
                    .as_ref()
                    .is_some_and(|thought| thought.expanded),
            ))
    });
    ChatRenderKey {
        width,
        animation_phase: if turns_have_running_execution_rows(turns) {
            phase % 4
        } else {
            0
        },
        turns_len: turns.len(),
        last_turn_hash,
        expanded_hash,
    }
}

fn turns_have_running_execution_rows(turns: &[Turn]) -> bool {
    turns.iter().any(|turn| {
        turn.context_rows
            .iter()
            .any(|row| row.kind == ContextRowKind::Execution && row_is_running(row))
    })
}

fn hash_turn_render_inputs(turn: &Turn) -> u64 {
    let mut hash = 1469598103934665603u64;
    for text in [&turn.user, &turn.answer, &turn.footer] {
        for byte in text.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(1099511628211);
        }
    }
    for row in &turn.context_rows {
        for byte in row.summary.as_bytes().iter().chain(row.detail.as_bytes()) {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(1099511628211);
        }
    }
    hash
}

fn render_user_block(text: &str, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let content_width = width.saturating_sub(theme.thread_gutter + theme.user_right_gap);
    let mut lines = Vec::new();
    let leading = theme
        .user_leading_bar
        .to_string()
        .repeat(theme.thread_gutter);
    lines.push(Line::from(vec![
        Span::styled(
            leading.clone(),
            Style::default()
                .fg(theme.thread_accent)
                .bg(theme.thread_accent),
        ),
        Span::styled(
            " ".repeat(content_width),
            Style::default().bg(theme.user_bg),
        ),
        Span::raw(" ".repeat(theme.user_right_gap)),
    ]));
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
                leading.clone(),
                Style::default()
                    .fg(theme.thread_accent)
                    .bg(theme.thread_accent),
            ),
            Span::styled(padded, Style::default().bg(theme.user_bg).fg(theme.text)),
            Span::raw(" ".repeat(theme.user_right_gap)),
        ]));
    }
    lines.push(Line::from(vec![
        Span::styled(
            leading,
            Style::default()
                .fg(theme.thread_accent)
                .bg(theme.thread_accent),
        ),
        Span::styled(
            " ".repeat(content_width),
            Style::default().bg(theme.user_bg),
        ),
        Span::raw(" ".repeat(theme.user_right_gap)),
    ]));
    lines
}

fn render_context_row_header(row: &ContextRow, width: usize, theme: &Theme) -> Line<'static> {
    render_context_row_header_with_phase(row, width, theme, 0)
}

fn render_context_row_header_with_phase(
    row: &ContextRow,
    width: usize,
    theme: &Theme,
    phase: usize,
) -> Line<'static> {
    let marker = context_row_marker(row, phase);
    let body_width = thread_body_width(width, theme, ThreadTone::Thought);
    let label = truncate_to_width(
        &format!("{marker} {} {}", context_row_label(row.kind), row.summary),
        body_width,
    );
    let style = if row_is_running(row) {
        working_shimmer_style(0, phase, theme).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme.thought_text)
            .add_modifier(Modifier::BOLD)
    };
    thread_line(
        Line::from(vec![Span::styled(pad_to_width(&label, body_width), style)]),
        width,
        theme,
        ThreadTone::Thought,
    )
}

fn row_is_running(row: &ContextRow) -> bool {
    row.detail.contains("状态：running")
        || row.detail.contains("状态：streaming")
        || row.detail.contains("状态：thinking")
        || row.summary.contains("运行中")
        || row.summary.contains("思考中")
}

fn thought_summary(thought: &ThoughtData) -> String {
    if !thought.summary.trim().is_empty() {
        return thought.summary.clone();
    }
    match thought.duration_ms {
        Some(duration) => format!("Thought for {duration}ms"),
        None => "Thought".to_string(),
    }
}

fn context_row_marker(row: &ContextRow, phase: usize) -> &'static str {
    match row.kind {
        ContextRowKind::AskResume => "◎",
        ContextRowKind::CreateFork => "⊕",
        ContextRowKind::Execution if row.expanded => "◼",
        ContextRowKind::Execution if row_is_running(row) => {
            const FRAMES: [&str; 4] = ["◰", "◳", "◲", "◱"];
            FRAMES[phase % FRAMES.len()]
        }
        ContextRowKind::Execution => "◱",
        ContextRowKind::Blackboard | ContextRowKind::Thought => {
            if row.expanded {
                "▼"
            } else {
                "▶"
            }
        }
        _ if row.expanded => "▼",
        _ => "▶",
    }
}

fn context_row_label(kind: ContextRowKind) -> &'static str {
    match kind {
        ContextRowKind::Recall => "☁️ 回忆中",
        ContextRowKind::Thought => "😌 思考中",
        ContextRowKind::Fork => "🍀 fork",
        ContextRowKind::Blackboard => "🤔 黑板讨论",
        ContextRowKind::Execution => "Exo",
        ContextRowKind::AskResume => "重新回答",
        ContextRowKind::CreateFork => "新建 fork",
    }
}

fn thread_line(
    line: Line<'static>,
    width: usize,
    theme: &Theme,
    tone: ThreadTone,
) -> Line<'static> {
    let content_pad = thread_left_pad(theme, tone);
    let right_pad = thread_right_pad(theme, tone);
    let body_width = thread_body_width(width, theme, tone);
    let mut spans = vec![Span::styled(
        centered_bar(
            match tone {
                ThreadTone::Rail => theme.rail_char,
                ThreadTone::Thought => theme.thought_bar_char,
            },
            theme.thread_gutter,
        ),
        Style::default().fg(match tone {
            ThreadTone::Rail => theme.rail,
            ThreadTone::Thought => theme.thought_bar,
        }),
    )];
    if content_pad > 0 {
        spans.push(Span::raw(" ".repeat(content_pad)));
    }
    if line.spans.is_empty() {
        spans.push(Span::raw(" ".repeat(body_width)));
        if right_pad > 0 {
            spans.push(Span::raw(" ".repeat(right_pad)));
        }
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
    if right_pad > 0 {
        spans.push(Span::raw(" ".repeat(right_pad)));
    }
    Line::from(spans)
}

fn empty_content_line(width: usize, theme: &Theme) -> Line<'static> {
    thread_line(Line::raw(""), width, theme, ThreadTone::Rail)
}

fn render_footer_line(footer: &str, width: usize, theme: &Theme) -> Line<'static> {
    let body_width = thread_body_width(width, theme, ThreadTone::Rail);
    let label = truncate_to_width(footer, body_width.saturating_sub(4));
    let mut spans = vec![
        Span::styled(
            centered_bar(theme.rail_char, theme.thread_gutter),
            Style::default().fg(theme.rail),
        ),
        Span::raw(" ".repeat(theme.answer_left_pad)),
        Span::styled(
            theme.footer_icon.to_string(),
            Style::default().fg(theme.footer_icon_color),
        ),
        Span::raw(" "),
    ];
    let parts: Vec<&str> = label.split(" · ").collect();
    for (index, part) in parts.iter().enumerate() {
        spans.push(Span::styled(
            part.to_string(),
            Style::default().fg(if index == 0 {
                theme.footer_primary
            } else {
                theme.footer_muted
            }),
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
    if theme.answer_right_pad > 0 {
        spans.push(Span::raw(" ".repeat(theme.answer_right_pad)));
    }
    Line::from(spans)
}

fn thread_left_pad(theme: &Theme, tone: ThreadTone) -> usize {
    match tone {
        ThreadTone::Rail | ThreadTone::Thought => theme.answer_left_pad,
    }
}

fn thread_right_pad(theme: &Theme, tone: ThreadTone) -> usize {
    match tone {
        ThreadTone::Rail | ThreadTone::Thought => theme.answer_right_pad,
    }
}

fn thread_body_width(width: usize, theme: &Theme, tone: ThreadTone) -> usize {
    width
        .saturating_sub(theme.thread_gutter)
        .saturating_sub(thread_left_pad(theme, tone))
        .saturating_sub(thread_right_pad(theme, tone))
        .max(1)
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
            lines.extend(render_code_block(
                &lang,
                &code_lines,
                content_width,
                theme,
                tone,
            ));
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
    lines.push(Line::styled(truncate_to_width(&label, width), header_style));
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
                style: base_style.fg(theme.blue).add_modifier(Modifier::UNDERLINED),
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

fn centered_bar(bar: char, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let center = width / 2;
    let mut output = String::with_capacity(width);
    for index in 0..width {
        output.push(if index == center { bar } else { ' ' });
    }
    output
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
        let first = format!(
            "{prefix}─ {}",
            truncate_to_width(&from, width.saturating_sub(3))
        );
        lines.push(first);
        lines.push(format!(
            "│  {}",
            truncate_to_width("│", width.saturating_sub(3))
        ));
        lines.push(format!(
            "│  └─▶ {}",
            truncate_to_width(&to, width.saturating_sub(7))
        ));
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

fn render_right_panel_sections(
    data: &RightPanelData,
    todos: &[TodoItem],
    width: usize,
) -> Vec<RightPanelSection> {
    let mut sections = vec![
        right_section("ACT 计划", todo_section_rows(todos)),
        right_section("Run", right_run_summary_rows(&data.run_timeline)),
        right_section(
            "Model / Status",
            data.model_stats
                .iter()
                .chain(data.token_stats.iter())
                .map(|stat| format!("{}: {}", stat.label, stat.value))
                .collect(),
        ),
        right_section("Context Window", vec![context_usage_for_width(data, width)]),
        right_section(
            "Fork / Memory",
            fork_memory_rows_for_width(&data.fork_memory, width),
        ),
    ];

    for section in sections.iter_mut() {
        if section.title == "ACT 计划" {
            continue;
        }
        section.lines = render_right_section_lines(section, width, false);
    }
    sections
}

fn right_run_summary_rows(timeline: &RunTimeline) -> Vec<String> {
    if timeline.items.is_empty() && timeline.subagents.batches.is_empty() {
        return vec!["waiting for run events".to_string()];
    }
    let mut rows = vec![execution_context_summary(timeline)];
    rows.extend(
        run_panel_lines(timeline)
            .into_iter()
            .skip(2)
            .map(|line| line_plain_text(&line))
            .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with("Subagents"))
            .take(5),
    );
    rows
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RightPanelStickyLayout {
    todo: Rect,
    separator: Rect,
    bottom: Rect,
    bottom_text: Rect,
}

fn right_panel_layout(inner: Rect, bottom_height: u16) -> RightPanelStickyLayout {
    let bottom_height = bottom_height.min(inner.height);
    let separator_height = u16::from(inner.height.saturating_sub(bottom_height) > 1);
    let todo_height = inner
        .height
        .saturating_sub(separator_height)
        .saturating_sub(bottom_height);
    let todo = Rect::new(inner.x, inner.y, inner.width.saturating_sub(2), todo_height);
    let separator = Rect::new(
        inner.x,
        inner.y + todo_height,
        inner.width.saturating_sub(2),
        separator_height,
    );
    let bottom = Rect::new(
        inner.x,
        inner.y + todo_height + separator_height,
        inner.width,
        bottom_height,
    );
    let bottom_text = Rect::new(
        bottom.x,
        bottom.y,
        bottom.width.saturating_sub(2),
        bottom.height,
    );
    RightPanelStickyLayout {
        todo,
        separator,
        bottom,
        bottom_text,
    }
}

fn right_todo_body_area(todo: Rect) -> Rect {
    let title_height = u16::from(todo.height > 0);
    Rect::new(
        todo.x,
        todo.y + title_height,
        todo.width,
        todo.height.saturating_sub(title_height),
    )
}

fn right_bottom_height(data: &RightPanelData, width: usize, max_height: u16) -> u16 {
    let sections = render_right_panel_sections(data, &[TodoItem::empty_plan()], width);
    let bottom_lines = flatten_right_panel_sections(scrollable_right_sections(&sections));
    let desired = count_visual_lines(&bottom_lines, width.max(1)) as u16;
    desired
        .min(max_height.saturating_sub(3))
        .max(1)
        .min(max_height)
}

#[cfg(test)]
fn fork_memory_rows(snapshot: &ForkMemorySnapshot) -> Vec<String> {
    fork_memory_rows_for_width(snapshot, usize::MAX)
}

fn fork_memory_rows_for_width(snapshot: &ForkMemorySnapshot, width: usize) -> Vec<String> {
    let mut rows = vec!["fork 最近 5 条".to_string()];
    if snapshot.forks.is_empty() {
        rows.push("fork: 暂无数据".to_string());
    } else {
        rows.extend(
            snapshot
                .forks
                .iter()
                .take(5)
                .enumerate()
                .map(|(index, fork)| {
                    let time = fork.time.as_deref().unwrap_or("时间未知");
                    truncate_to_width(
                        &format!("{}. {} · {}", index + 1, fork.summary, time),
                        width.saturating_sub(3).max(1),
                    )
                }),
        );
    }
    rows.push(format!("brain.db: {}", brain_db_label(snapshot)));
    rows
}

fn brain_db_label(snapshot: &ForkMemorySnapshot) -> String {
    match snapshot.brain_db_status.as_deref() {
        Some("unavailable") => "不可用".to_string(),
        Some("unknown") => "未收到".to_string(),
        _ => snapshot
            .brain_db_human
            .clone()
            .unwrap_or_else(|| "未收到".to_string()),
    }
}

fn fork_memory_from_data(data: &Value) -> ForkMemorySnapshot {
    let forks_value = data
        .get("forks")
        .or_else(|| data.get("recentForks"))
        .or_else(|| data.get("items"));
    let forks = forks_value
        .and_then(Value::as_array)
        .map(|forks| {
            forks
                .iter()
                .filter_map(fork_memory_item_from_value)
                .take(5)
                .collect()
        })
        .unwrap_or_default();
    let brain_db = data.get("brainDb").or_else(|| data.get("brainDB"));
    ForkMemorySnapshot {
        forks,
        brain_db_human: brain_db
            .and_then(|value| value_string(value, "human"))
            .or_else(|| value_string(data, "brainDbHuman"))
            .or_else(|| {
                brain_db
                    .and_then(|value| value.get("bytes").and_then(Value::as_u64))
                    .map(format_bytes)
            })
            .or_else(|| {
                data.get("brainDbBytes")
                    .and_then(Value::as_u64)
                    .map(format_bytes)
            }),
        brain_db_status: brain_db
            .and_then(|value| value_string(value, "status"))
            .or_else(|| value_string(data, "brainDbStatus")),
    }
}

fn fork_memory_item_from_value(value: &Value) -> Option<ForkMemoryItem> {
    let summary = value_string(value, "summary")
        .or_else(|| value_string(value, "title"))
        .or_else(|| value_string(value, "id"))?;
    let time = value_string(value, "updatedAt")
        .or_else(|| value_string(value, "createdAt"))
        .or_else(|| value_string(value, "at"));
    Some(ForkMemoryItem { summary, time })
}

fn format_bytes(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / MB)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn todo_section_rows(todos: &[TodoItem]) -> Vec<String> {
    let state = plan_state_from_todos(todos);
    let status = (state != PlanState::Empty).then(|| format!("状态：{}", state.label()));
    status
        .into_iter()
        .chain(todos.iter().map(|item| {
            format!(
                "{} {} [{}]{}",
                item.marker,
                item.label,
                item.status,
                if item.active { " 当前" } else { "" }
            )
        }))
        .chain(if state == PlanState::AwaitingConfirmation {
            vec![
                "操作：确认计划".to_string(),
                "操作：补充计划".to_string(),
                "操作：放弃计划".to_string(),
            ]
        } else {
            Vec::new()
        })
        .collect()
}

fn right_section(title: &str, rows: Vec<String>) -> RightPanelSection {
    RightPanelSection {
        title: title.to_string(),
        lines: rows.into_iter().map(Line::raw).collect(),
    }
}

fn render_right_section_title(
    section: &RightPanelSection,
    width: usize,
    selected: bool,
) -> Line<'static> {
    let title_color = if selected {
        Color::Rgb(255, 120, 170)
    } else {
        Color::Rgb(126, 160, 255)
    };
    Line::from(vec![
        Span::styled(
            if selected { "› " } else { "  " },
            Style::default().fg(title_color),
        ),
        Span::styled(
            truncate_to_width(&section.title, width.saturating_sub(2)),
            Style::default()
                .fg(title_color)
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

fn context_usage_for_width(data: &RightPanelData, width: usize) -> String {
    let text_width = width.saturating_sub(3).max(1);
    let compact_tokens = context_tokens_label(
        &compact_token_count(data.context_used_tokens),
        &data
            .context_max_tokens
            .map(compact_token_count)
            .unwrap_or_else(|| "未知".to_string()),
        &data.context_percent,
    );
    if let Some(line) = context_usage_line_that_fits(
        data.context_ratio,
        &compact_tokens,
        text_width,
        DEFAULT_CONTEXT_BAR_WIDTH,
    ) {
        return line;
    }
    if UnicodeWidthStr::width(compact_tokens.as_str()) <= text_width {
        compact_tokens
    } else {
        context_tokens_label(&data.context_used, &data.context_max, &data.context_percent)
    }
}

fn context_usage_line_that_fits(
    ratio: f64,
    tokens: &str,
    width: usize,
    preferred_bar_width: usize,
) -> Option<String> {
    let token_width = UnicodeWidthStr::width(tokens);
    if width <= token_width + 1 {
        return None;
    }
    let max_bar_width = width - token_width - 1;
    let bar_width = preferred_bar_width.min(max_bar_width).max(1);
    let line = format!("{} {tokens}", context_bar(ratio, bar_width));
    if UnicodeWidthStr::width(line.as_str()) <= width {
        Some(line)
    } else {
        None
    }
}

fn render_right_section_lines(
    section: &RightPanelSection,
    width: usize,
    selected: bool,
) -> Vec<Line<'static>> {
    if section.title == "Context Window" {
        return section
            .lines
            .iter()
            .map(|line| {
                Line::styled(
                    format!("  {}", line_plain_text(line)),
                    Style::default().fg(Color::Rgb(232, 235, 245)),
                )
            })
            .chain(std::iter::once(Line::raw("")))
            .collect();
    }
    let mut lines = vec![render_right_section_title(section, width, selected)];
    let content_width = width.saturating_sub(3).max(1);
    for line in &section.lines {
        let text = line_plain_text(line);
        for row in wrap_plain_text(&text, content_width) {
            lines.push(Line::styled(
                format!("  {row}"),
                Style::default().fg(Color::Rgb(232, 235, 245)),
            ));
        }
    }
    lines.push(Line::raw(""));
    lines
}

fn flatten_right_panel_sections(sections: &[RightPanelSection]) -> Vec<Line<'static>> {
    sections
        .iter()
        .flat_map(|section| section.lines.clone())
        .collect()
}

fn scrollable_right_sections(sections: &[RightPanelSection]) -> &[RightPanelSection] {
    if sections.is_empty() {
        sections
    } else {
        &sections[1..]
    }
}

struct ContextWindowEstimate {
    total: String,
    percent: String,
    bar: String,
    usage: String,
    ratio: f64,
    used_tokens: usize,
    max_tokens: Option<usize>,
}

fn estimate_context_window(
    turns: &[Turn],
    active_context_fork_id: &Option<String>,
    status: &StatusSnapshot,
) -> ContextWindowEstimate {
    let local_estimated_tokens = estimate_hot_context_tokens(turns, active_context_fork_id);
    let hot_tokens = status.hot_context_tokens.unwrap_or(local_estimated_tokens);
    let max_tokens = status
        .context_window_tokens
        .or_else(|| {
            status
                .model_name
                .as_deref()
                .and_then(model_context_window_tokens)
        })
        .or_else(model_context_window_tokens_from_env);
    let (percent, bar, usage, ratio) = if let Some(max_tokens) = max_tokens {
        let ratio = status
            .context_window_percent
            .map(|percent| {
                if percent > 1.0 {
                    percent / 100.0
                } else {
                    percent
                }
            })
            .unwrap_or_else(|| {
                if max_tokens == 0 {
                    0.0
                } else {
                    hot_tokens as f64 / max_tokens as f64
                }
            })
            .clamp(0.0, 1.0);
        let display_percent = status.context_window_percent.unwrap_or(ratio * 100.0);
        (
            format!("{display_percent:.2}%"),
            context_bar(ratio, DEFAULT_CONTEXT_BAR_WIDTH),
            context_usage_line(
                &context_bar(ratio, DEFAULT_CONTEXT_BAR_WIDTH),
                &compact_token_count(hot_tokens),
                &compact_token_count(max_tokens),
                &format!("{display_percent:.2}%"),
            ),
            ratio,
        )
    } else {
        (
            "未知".to_string(),
            context_bar(0.0, DEFAULT_CONTEXT_BAR_WIDTH),
            context_usage_line(
                &context_bar(0.0, DEFAULT_CONTEXT_BAR_WIDTH),
                &compact_token_count(hot_tokens),
                "未知",
                "未知",
            ),
            0.0,
        )
    };

    ContextWindowEstimate {
        total: max_tokens
            .map(|tokens| format!("最大 {} tokens", compact_token_count(tokens)))
            .unwrap_or_else(|| "未收到上下文窗口".to_string()),
        percent,
        bar,
        usage,
        ratio,
        used_tokens: hot_tokens,
        max_tokens,
    }
}

fn model_context_window_tokens(model_name: &str) -> Option<usize> {
    let name = model_name.to_ascii_lowercase();
    if name.contains("deepseek") {
        Some(1_000_000)
    } else {
        None
    }
}

fn estimate_hot_context_tokens(turns: &[Turn], active_context_fork_id: &Option<String>) -> usize {
    let transcript_chars: usize = turns
        .iter()
        .rev()
        .take(12)
        .map(|turn| turn.user.chars().count() + turn.answer.chars().count())
        .sum();
    let fork_context = active_context_fork_id
        .as_ref()
        .map(|fork_id| fork_id.chars().count())
        .unwrap_or(0);
    (transcript_chars + fork_context).div_ceil(4).max(1)
}

fn model_context_window_tokens_from_env() -> Option<usize> {
    env::var("FLYFLOR_CONTEXT_WINDOW")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
}

fn context_bar(ratio: f64, width: usize) -> String {
    let filled = ((ratio.clamp(0.0, 1.0) * width as f64).round() as usize).min(width);
    format!("{}{}", "■".repeat(filled), "□".repeat(width - filled))
}

fn context_usage_line(bar: &str, used: &str, max: &str, percent: &str) -> String {
    let tokens = context_tokens_label(used, max, percent);
    format!("{bar} {tokens}")
}

fn context_tokens_label(used: &str, max: &str, percent: &str) -> String {
    format!("{used}/{max} {percent}")
}

fn compact_token_count(tokens: usize) -> String {
    const UNITS: &[(usize, &str)] = &[(1_000_000_000, "B"), (1_000_000, "M"), (1_000, "k")];
    for (unit, suffix) in UNITS {
        if tokens >= *unit {
            let value = tokens as f64 / *unit as f64;
            if value >= 100.0 || (value.fract() - 0.0).abs() < f64::EPSILON {
                return format!("{}{suffix}", value.round() as usize);
            }
            return format!("{value:.1}{suffix}");
        }
    }
    tokens.to_string()
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

fn render_input_lines(input: &str, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let content_width = width.saturating_sub(2).max(1);
    if input.is_empty() {
        return vec![Line::from(vec![
            Span::styled(
                pad_to_width("ask anything...", content_width),
                Style::default().fg(theme.muted),
            ),
            Span::styled(
                " >",
                Style::default()
                    .fg(theme.purple)
                    .add_modifier(Modifier::BOLD),
            ),
        ])];
    }

    let mut lines = Vec::new();
    for source_line in input.split('\n') {
        let wrapped = wrap_plain_text(source_line, content_width);
        for row in wrapped {
            lines.push(Line::from(vec![Span::styled(
                pad_to_width(&row, content_width),
                Style::default().fg(theme.text),
            )]));
        }
    }
    if input.ends_with('\n') {
        lines.push(Line::raw(""));
    }
    if let Some(last) = lines.last_mut() {
        last.spans.push(Span::styled(
            " >",
            Style::default()
                .fg(theme.purple)
                .add_modifier(Modifier::BOLD),
        ));
    }
    lines
}

fn footer_mode_text(app: &App) -> String {
    app.interaction_mode.label().to_string()
}

fn composer_footer_line(app: &App, theme: &Theme) -> Line<'static> {
    if matches!(app.composer_notice, Some(ComposerNotice::ExitHint)) {
        return Line::from(vec![Span::styled(
            "输入 /exit 退出",
            Style::default().fg(theme.pink).add_modifier(Modifier::BOLD),
        )]);
    }

    Line::from(vec![
        Span::styled(
            format!("{}{}", footer_mode_text(app), fork_footer_suffix(app)),
            Style::default()
                .fg(app.interaction_mode.color(theme))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" · Enter 发送   ", Style::default().fg(theme.muted)),
        Span::styled("Shift + Tab", Style::default().fg(theme.text)),
        Span::styled(" 切换模式  ", Style::default().fg(theme.muted)),
        Span::styled("←/→", Style::default().fg(theme.text)),
        Span::styled(" 移动光标", Style::default().fg(theme.muted)),
        Span::styled("y", Style::default().fg(theme.text)),
        Span::styled(" 复制选择", Style::default().fg(theme.muted)),
    ])
}

fn input_cursor_position(
    input: &str,
    cursor_index: usize,
    area: Rect,
    scroll: usize,
) -> Option<Position> {
    let content_width = area.width.saturating_sub(2).max(1) as usize;
    let (visual_line, visual_col) =
        input_visual_cursor(input, cursor_index.min(input.len()), content_width);

    let visible_line = visual_line.saturating_sub(scroll);
    if visible_line >= area.height as usize {
        return None;
    }
    Some(Position::new(
        area.x + visual_col.min(area.width.saturating_sub(1) as usize) as u16,
        area.y + visible_line as u16,
    ))
}

fn input_visual_cursor(input: &str, cursor_index: usize, content_width: usize) -> (usize, usize) {
    let mut visual_line = 0usize;
    let mut current_line_start = 0usize;
    for line in input.split_inclusive('\n') {
        let has_newline = line.ends_with('\n');
        let line_end = current_line_start + line.len() - usize::from(has_newline);
        if cursor_index <= line_end {
            let prefix = &input[current_line_start..cursor_index];
            let width = UnicodeWidthStr::width(prefix);
            return (visual_line + width / content_width, width % content_width);
        }
        let width = UnicodeWidthStr::width(&input[current_line_start..line_end]);
        visual_line += width.div_ceil(content_width).max(1);
        if has_newline {
            current_line_start += line.len();
        }
    }
    if current_line_start <= input.len() {
        let prefix = &input[current_line_start..cursor_index.min(input.len())];
        let width = UnicodeWidthStr::width(prefix);
        return (visual_line + width / content_width, width % content_width);
    }
    (visual_line, 0)
}

fn input_line_start_and_column(input: &str, index: usize) -> (usize, usize) {
    let index = index.min(input.len());
    let line_start = input[..index]
        .rfind('\n')
        .map(|position| position + 1)
        .unwrap_or(0);
    let column = UnicodeWidthStr::width(&input[line_start..index]);
    (line_start, column)
}

fn input_index_for_column(input: &str, start: usize, end: usize, target_column: usize) -> usize {
    let mut width = 0usize;
    for (offset, ch) in input[start..end].char_indices() {
        let next = width + string_width_char(ch);
        if next > target_column {
            return start + offset;
        }
        width = next;
    }
    end
}

fn normalize_paste_text(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn apply_selection_to_lines(
    lines: &mut [Line<'static>],
    top: usize,
    selection: TranscriptSelection,
    target: SelectionTarget,
    theme: &Theme,
) {
    let Some((start, end)) = selection.ordered_endpoints() else {
        return;
    };
    if start.target != target {
        return;
    }
    let selection_style = Style::default().bg(theme.status_active_bg).fg(theme.text);
    for (line_index, line) in lines.iter_mut().enumerate() {
        let document_line_index = top + line_index;
        if document_line_index < start.line_index || document_line_index > end.line_index {
            continue;
        }
        let (col_start, col_end) = if start.line_index == end.line_index {
            (start.column, end.column)
        } else if document_line_index == start.line_index {
            (start.column, usize::MAX)
        } else if document_line_index == end.line_index {
            (0, end.column)
        } else {
            (0, usize::MAX)
        };
        if col_start == 0 && col_end == usize::MAX {
            for span in &mut line.spans {
                span.style = span.style.patch(selection_style);
            }
            continue;
        }
        line.spans = apply_selection_to_line(line, col_start, col_end, selection_style);
    }
}

fn apply_selection_to_line(
    line: &Line<'static>,
    col_start: usize,
    col_end: usize,
    selection_style: Style,
) -> Vec<Span<'static>> {
    let mut result = Vec::with_capacity(line.spans.len().saturating_add(2));
    let mut current_col = 0usize;

    for span in &line.spans {
        let text: &str = span.content.as_ref();
        let span_width = UnicodeWidthStr::width(text);
        let span_end = current_col.saturating_add(span_width);
        if span_end <= col_start || current_col >= col_end {
            result.push(span.clone());
        } else if current_col >= col_start && span_end <= col_end {
            result.push(Span::styled(
                span.content.clone(),
                span.style.patch(selection_style),
            ));
        } else {
            let mut before = String::new();
            let mut selected = String::new();
            let mut after = String::new();
            let mut ch_col = current_col;

            for ch in text.chars() {
                let width = string_width_char(ch);
                let ch_start = ch_col;
                let ch_end = ch_col.saturating_add(width);
                if ch_end <= col_start {
                    before.push(ch);
                } else if ch_start >= col_end {
                    after.push(ch);
                } else {
                    selected.push(ch);
                }
                ch_col = ch_end;
            }

            if !before.is_empty() {
                result.push(Span::styled(before, span.style));
            }
            if !selected.is_empty() {
                result.push(Span::styled(selected, span.style.patch(selection_style)));
            }
            if !after.is_empty() {
                result.push(Span::styled(after, span.style));
            }
        }
        current_col = span_end;
    }
    result
}

fn line_plain_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
}

fn slice_display_columns(text: &str, start: usize, end: usize) -> String {
    let mut output = String::new();
    let mut col = 0usize;
    for ch in text.chars() {
        let width = string_width_char(ch);
        let ch_start = col;
        let ch_end = col.saturating_add(width);
        if ch_end > start && ch_start < end {
            output.push(ch);
        }
        col = ch_end;
        if col >= end {
            break;
        }
    }
    output
}

fn strip_transcript_rails(text: &str) -> String {
    text.lines()
        .map(|line| {
            line.trim_start_matches(' ')
                .strip_prefix('│')
                .map(|rest| rest.trim_start_matches(' '))
                .unwrap_or(line)
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn clipboard_with_timeout() -> Option<Clipboard> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(Clipboard::new().ok());
    });
    rx.recv_timeout(CLIPBOARD_INIT_TIMEOUT).ok().flatten()
}

fn read_clipboard_text() -> Result<String, String> {
    let Some(mut clipboard) = clipboard_with_timeout() else {
        return Err("system clipboard unavailable or timed out".to_string());
    };
    clipboard.get_text().map_err(|error| error.to_string())
}

fn write_text_to_clipboard(text: &str) -> Result<(), String> {
    if let Some(mut clipboard) = clipboard_with_timeout()
        && clipboard.set_text(text.to_string()).is_ok()
    {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    if write_text_with_command("pbcopy", &[], text).is_ok() {
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    if write_text_with_command(
        "powershell.exe",
        &["-NoProfile", "-Command", "Set-Clipboard -Value $input"],
        text,
    )
    .is_ok()
    {
        return Ok(());
    }

    write_text_with_osc52(text)
}

fn write_text_with_command(command: &str, args: &[&str], text: &str) -> Result<(), String> {
    let mut child = Command::new(command)
        .args(args)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to run {command}: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|error| format!("failed to write to {command}: {error}"))?;
    }
    let status = child
        .wait()
        .map_err(|error| format!("failed to wait for {command}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{command} failed"))
    }
}

fn write_text_with_osc52(text: &str) -> Result<(), String> {
    if !io::stdout().is_terminal() {
        return Err("OSC52 clipboard fallback requires a terminal".to_string());
    }
    let sequence = osc52_sequence(text, env::var_os("TMUX").is_some())?;
    io::stdout()
        .write_all(sequence.as_bytes())
        .map_err(|error| format!("write OSC52 failed: {error}"))?;
    io::stdout()
        .flush()
        .map_err(|error| format!("flush OSC52 failed: {error}"))
}

fn osc52_sequence(text: &str, in_tmux: bool) -> Result<String, String> {
    if text.len() > OSC52_MAX_BYTES {
        return Err("selection is too large for OSC 52 clipboard fallback".to_string());
    }
    let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
    let sequence = format!("\x1b]52;c;{encoded}\x07");
    if in_tmux {
        return Ok(format!("\x1bPtmux;\x1b{sequence}\x1b\\"));
    }
    Ok(sequence)
}

enum SocketCommand {
    SendMessage {
        message_id: String,
        text: String,
        context_fork_id: Option<String>,
        metadata: Option<Value>,
        mode: InteractionMode,
        yolo: bool,
    },
    ForkCreate {
        request_id: String,
        payload: Value,
    },
    TaskList,
    TaskPlanDecide {
        plan_id: String,
        action: PlanAction,
        revision: Option<String>,
    },
    ExecutionJobDetailGet {
        job_id: String,
    },
    ForkMemoryGet,
    StatusGet,
    HistoryList {
        context_fork_id: Option<String>,
    },
}

enum SocketEvent {
    Connected,
    HistoryLoaded(Vec<Turn>),
    TurnDelta {
        message_id: String,
        delta: String,
    },
    TurnFinal {
        message_id: String,
        text: String,
        metadata: Option<Value>,
    },
    TurnError {
        message_id: String,
        message: String,
    },
    ForkCreated {
        fork_id: String,
        parent_id: Option<String>,
        root_id: Option<String>,
        summary: Option<String>,
    },
    ForkMemoryLoaded(ForkMemorySnapshot),
    TaskListLoaded(Vec<TodoItem>),
    RuntimeEvent(Value),
    ExecutionJobSnapshot(Value),
    StatusLoaded(StatusSnapshot),
    ContextSnapshotLoaded(Value),
    Disconnected(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
struct StatusSnapshot {
    context_window_tokens: Option<usize>,
    max_output_tokens: Option<usize>,
    hot_context_tokens: Option<usize>,
    context_window_percent: Option<f64>,
    context_status: Option<String>,
    remaining_context_tokens: Option<usize>,
    cache_read_tokens: Option<usize>,
    cache_write_tokens: Option<usize>,
    model_name: Option<String>,
    model_provider: Option<String>,
}

fn spawn_socket_worker() -> (Sender<SocketCommand>, Receiver<SocketEvent>) {
    let (command_tx, command_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();
    if !history_enabled() {
        log_event("socket worker disabled by FLYFLOR_HISTORY");
        return (command_tx, event_rx);
    }
    thread::spawn(move || {
        log_event("socket worker start");
        if let Err(message) = run_socket_worker(command_rx, event_tx.clone()) {
            log_event(format!("socket worker fatal {message}"));
            let _ = event_tx.send(SocketEvent::Disconnected(message));
        }
    });
    (command_tx, event_rx)
}

fn history_enabled() -> bool {
    if tui::demo_enabled() {
        return false;
    }
    !env::var("FLYFLOR_HISTORY")
        .map(|value| matches!(value.as_str(), "0" | "false" | "FALSE" | "off" | "OFF"))
        .unwrap_or(false)
}

fn run_socket_worker(
    command_rx: Receiver<SocketCommand>,
    event_tx: Sender<SocketEvent>,
) -> Result<(), String> {
    loop {
        if let Err(message) = run_socket_session(&command_rx, &event_tx) {
            let _ = event_tx.send(SocketEvent::Disconnected(message));
            thread::sleep(Duration::from_millis(500));
        }
    }
}

fn run_socket_session(
    command_rx: &Receiver<SocketCommand>,
    event_tx: &Sender<SocketEvent>,
) -> Result<(), String> {
    let url = ws_url();
    log_event(format!("socket connect {url}"));
    let (mut socket, _) = connect(url.as_str()).map_err(|error| error.to_string())?;
    configure_socket_timeout(&mut socket)?;
    log_event("socket connected");

    let now = now_millis();
    let gateway = gateway_command_builder();
    let bootstrap = gateway_client_bootstrap();
    for (index, envelope) in bootstrap
        .build(now, env!("CARGO_PKG_VERSION"))
        .into_iter()
        .enumerate()
    {
        socket
            .send(Message::text(envelope.into_value().to_string()))
            .map_err(|error| error.to_string())?;
        if index == 0 {
            event_tx
                .send(SocketEvent::Connected)
                .map_err(|error| error.to_string())?;
        }
    }

    loop {
        while let Ok(command) = command_rx.try_recv() {
            match command {
                SocketCommand::SendMessage {
                    message_id,
                    text,
                    context_fork_id,
                    metadata,
                    mode,
                    yolo,
                } => {
                    log_event(format!(
                        "send gateway.message.send message_id={message_id} chars={}",
                        text.chars().count()
                    ));
                    if let Err(error) = socket.send(Message::text(
                        gateway
                            .gateway_message_send(
                                now_millis(),
                                gateway_message_payload(
                                    &message_id,
                                    &text,
                                    context_fork_id.as_deref(),
                                    metadata.as_ref(),
                                    mode,
                                    yolo,
                                ),
                            )
                            .into_value()
                            .to_string(),
                    )) {
                        log_event(format!("send failed message_id={message_id} error={error}"));
                        let _ = event_tx.send(SocketEvent::TurnError {
                            message_id,
                            message: error.to_string(),
                        });
                        return Err(error.to_string());
                    }
                }
                SocketCommand::ForkCreate {
                    request_id,
                    payload,
                } => {
                    log_event(format!("send fork.create request_id={request_id}"));
                    if let Err(error) = socket.send(Message::text(
                        gateway
                            .fork_create(&request_id, now_millis(), payload)
                            .into_value()
                            .to_string(),
                    )) {
                        log_event(format!(
                            "fork.create failed request_id={request_id} error={error}"
                        ));
                        let _ = event_tx.send(SocketEvent::Disconnected(error.to_string()));
                        return Err(error.to_string());
                    }
                }
                SocketCommand::TaskList => {
                    let now = now_millis();
                    log_event("send task.list");
                    if let Err(error) = socket.send(Message::text(
                        gateway.task_list(now).into_value().to_string(),
                    )) {
                        log_event(format!("task.list failed error={error}"));
                        let _ = event_tx.send(SocketEvent::Disconnected(error.to_string()));
                        return Err(error.to_string());
                    }
                }
                SocketCommand::TaskPlanDecide {
                    plan_id,
                    action,
                    revision,
                } => {
                    let now = now_millis();
                    log_event(format!("send task.plan.decide action={}", action.as_str()));
                    if let Err(error) = socket.send(Message::text(
                        gateway
                            .task_plan_decide(now, &plan_id, action, revision.as_deref())
                            .into_value()
                            .to_string(),
                    )) {
                        log_event(format!("task.plan.decide failed error={error}"));
                        let _ = event_tx.send(SocketEvent::Disconnected(error.to_string()));
                        return Err(error.to_string());
                    }
                }
                SocketCommand::ExecutionJobDetailGet { job_id } => {
                    let now = now_millis();
                    log_event(format!("send execution.job.detail.get job_id={job_id}"));
                    if let Err(error) = socket.send(Message::text(
                        gateway
                            .execution_job_detail_get(now, &job_id)
                            .into_value()
                            .to_string(),
                    )) {
                        log_event(format!("execution.job.detail.get failed error={error}"));
                        let _ = event_tx.send(SocketEvent::Disconnected(error.to_string()));
                        return Err(error.to_string());
                    }
                }
                SocketCommand::ForkMemoryGet => {
                    let now = now_millis();
                    log_event("send fork.memory.get");
                    if let Err(error) = socket.send(Message::text(
                        gateway.fork_memory_get(now, 5).into_value().to_string(),
                    )) {
                        log_event(format!("fork.memory.get failed error={error}"));
                        let _ = event_tx.send(SocketEvent::Disconnected(error.to_string()));
                        return Err(error.to_string());
                    }
                }
                SocketCommand::StatusGet => {
                    let now = now_millis();
                    log_event("send gateway.status.get");
                    if let Err(error) = socket.send(Message::text(
                        gateway.gateway_status_get(now).into_value().to_string(),
                    )) {
                        log_event(format!("gateway.status.get failed error={error}"));
                        let _ = event_tx.send(SocketEvent::Disconnected(error.to_string()));
                        return Err(error.to_string());
                    }
                }
                SocketCommand::HistoryList { context_fork_id } => {
                    let now = now_millis();
                    log_event("send history.list");
                    if let Err(error) = socket.send(Message::text(
                        gateway
                            .history_list_with_before(
                                now,
                                history_limit_from_env(),
                                context_fork_id.as_deref(),
                                history_before_ts_from_env(),
                            )
                            .into_value()
                            .to_string(),
                    )) {
                        log_event(format!("history.list failed error={error}"));
                        let _ = event_tx.send(SocketEvent::Disconnected(error.to_string()));
                        return Err(error.to_string());
                    }
                }
            }
        }

        match socket.read() {
            Ok(Message::Text(text)) => {
                handle_socket_text(text.as_ref(), &event_tx)?;
            }
            Ok(Message::Close(_)) => {
                log_event("socket close frame");
                return Err("socket closed".to_string());
            }
            Ok(_) => {}
            Err(WsError::Io(error))
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(error) => {
                log_event(format!("socket read error {error}"));
                return Err(error.to_string());
            }
        }
    }
}

fn configure_socket_timeout(
    socket: &mut tungstenite::WebSocket<MaybeTlsStream<TcpStream>>,
) -> Result<(), String> {
    match socket.get_mut() {
        MaybeTlsStream::Plain(stream) => stream
            .set_read_timeout(Some(Duration::from_millis(50)))
            .map_err(|error| error.to_string()),
        _ => Ok(()),
    }
}

fn gateway_command_builder() -> GatewayCommandBuilder {
    GatewayCommandBuilder::new(EnvelopeFactory::new("flyflor-cli"))
}

fn gateway_client_bootstrap() -> GatewayClientBootstrap {
    GatewayClientBootstrap::new(EnvelopeFactory::new("flyflor-cli"))
        .with_limits(history_limit_from_env(), 5)
        .with_history_before_ts(history_before_ts_from_env())
}

fn history_limit_from_env() -> u64 {
    env::var("FLYFLOR_HISTORY_LIMIT")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(20)
}

fn history_before_ts_from_env() -> Option<u64> {
    env::var("FLYFLOR_HISTORY_BEFORE_TS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
}

fn gateway_message_payload(
    message_id: &str,
    text: &str,
    context_fork_id: Option<&str>,
    metadata: Option<&Value>,
    mode: InteractionMode,
    yolo: bool,
) -> GatewayMessagePayload {
    let mut payload = GatewayMessagePayload::new(message_id, text)
        .mode(mode.as_str(), yolo)
        .identity(
            env::var("FLYFLOR_CONVERSATION_KEY").unwrap_or_else(|_| "flyflor-cli".to_string()),
            env::var("FLYFLOR_THREAD_ID").unwrap_or_else(|_| "flyflor-cli".to_string()),
            env::var("FLYFLOR_USER_ID").unwrap_or_else(|_| "flyflor-cli-user".to_string()),
            env::var("FLYFLOR_USER_NAME").unwrap_or_else(|_| "Flyflor CLI User".to_string()),
        );
    if let Some(context_fork_id) = context_fork_id {
        payload = payload.context_fork_id(context_fork_id);
    }
    if let Some(metadata) = metadata {
        payload = payload.metadata(metadata.clone());
    }
    payload
}

fn handle_socket_text(raw: &str, event_tx: &Sender<SocketEvent>) -> Result<(), String> {
    if let Some(turns) = parse_history_snapshot(raw)? {
        log_event(format!("history snapshot turns={}", turns.len()));
        event_tx
            .send(SocketEvent::HistoryLoaded(turns))
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    if let Some(event) = parse_fork_snapshot(raw)? {
        event_tx.send(event).map_err(|error| error.to_string())?;
        return Ok(());
    }
    if let Some(event) = parse_task_list_snapshot(raw)? {
        event_tx.send(event).map_err(|error| error.to_string())?;
        return Ok(());
    }
    if let Some(event) = parse_status_snapshot(raw)? {
        event_tx.send(event).map_err(|error| error.to_string())?;
        return Ok(());
    }
    if let Some(event) = parse_fork_memory_snapshot(raw)? {
        event_tx.send(event).map_err(|error| error.to_string())?;
        return Ok(());
    }
    if let Some(event) = parse_execution_job_snapshot(raw)? {
        event_tx.send(event).map_err(|error| error.to_string())?;
        return Ok(());
    }
    if let Some(event) = parse_context_snapshot(raw)? {
        event_tx.send(event).map_err(|error| error.to_string())?;
        return Ok(());
    }
    if let Some(event) = parse_subscription_event(raw)? {
        event_tx.send(event).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let Some(event) = parse_turn_event(raw)? else {
        return Ok(());
    };
    match event {
        SocketEvent::Disconnected(message) => {
            log_event(format!("socket envelope error {message}"));
            let _ = event_tx.send(SocketEvent::Disconnected(message));
        }
        event => {
            event_tx.send(event).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn parse_history_snapshot(raw: &str) -> Result<Option<Vec<Turn>>, String> {
    let envelope: SocketEnvelope = serde_json::from_str(raw).map_err(|error| error.to_string())?;
    if envelope.message_type == "error" {
        return Ok(None);
    }
    if envelope.message_type != "history.snapshot" {
        return Ok(None);
    }

    let Some(payload) = envelope.payload else {
        return Err("history.snapshot missing payload".to_string());
    };
    let snapshot: HistorySnapshotPayload =
        serde_json::from_value(payload).map_err(|error| error.to_string())?;
    let mut history = snapshot.history;
    history.sort_by_key(|turn| turn.ts);
    Ok(Some(
        history
            .into_iter()
            .map(history_turn_snapshot_to_turn)
            .collect(),
    ))
}

fn parse_turn_event(raw: &str) -> Result<Option<SocketEvent>, String> {
    let envelope: SocketEnvelope = serde_json::from_str(raw).map_err(|error| error.to_string())?;
    match envelope.message_type.as_str() {
        "turn.delta" => {
            let Some(payload) = envelope.payload else {
                return Err("turn.delta missing payload".to_string());
            };
            let payload: TurnDeltaPayload =
                serde_json::from_value(payload).map_err(|error| error.to_string())?;
            Ok(Some(SocketEvent::TurnDelta {
                message_id: payload.message_id,
                delta: payload.delta,
            }))
        }
        "turn.final" => {
            let Some(payload) = envelope.payload else {
                return Err("turn.final missing payload".to_string());
            };
            let payload: TurnFinalPayload =
                serde_json::from_value(payload).map_err(|error| error.to_string())?;
            Ok(Some(SocketEvent::TurnFinal {
                message_id: payload.reply.message_id,
                text: payload.reply.text,
                metadata: payload.reply.metadata,
            }))
        }
        "turn.error" => {
            let Some(payload) = envelope.payload else {
                return Err("turn.error missing payload".to_string());
            };
            let payload: TurnErrorPayload =
                serde_json::from_value(payload).map_err(|error| error.to_string())?;
            Ok(Some(SocketEvent::TurnError {
                message_id: payload.message_id,
                message: payload.message,
            }))
        }
        "error" => Ok(Some(SocketEvent::Disconnected(error_message_from_payload(
            &envelope.payload,
        )))),
        _ => Ok(None),
    }
}

fn parse_fork_snapshot(raw: &str) -> Result<Option<SocketEvent>, String> {
    let envelope: SocketEnvelope = serde_json::from_str(raw).map_err(|error| error.to_string())?;
    if envelope.message_type != "fork.snapshot" {
        return Ok(None);
    }
    let Some(payload) = envelope.payload else {
        return Ok(None);
    };
    let data = payload.get("data").unwrap_or(&payload);
    let fork = data.get("fork").or_else(|| {
        data.get("forks")
            .and_then(Value::as_array)
            .and_then(|forks| forks.first())
    });
    let Some(fork) = fork else {
        return Ok(None);
    };
    let Some(fork_id) = value_string(fork, "id") else {
        return Ok(None);
    };
    let session = data.get("session");
    Ok(Some(SocketEvent::ForkCreated {
        fork_id: session
            .and_then(|session| value_string(session, "activeForkId"))
            .unwrap_or(fork_id),
        parent_id: session.and_then(|session| value_string(session, "parentId")),
        root_id: session.and_then(|session| value_string(session, "rootId")),
        summary: value_string(fork, "summary")
            .or_else(|| value_string(fork, "continuitySummary"))
            .or_else(|| value_string(fork, "title")),
    }))
}

fn parse_task_list_snapshot(raw: &str) -> Result<Option<SocketEvent>, String> {
    let envelope: SocketEnvelope = serde_json::from_str(raw).map_err(|error| error.to_string())?;
    if !matches!(
        envelope.message_type.as_str(),
        "task.snapshot" | "task.list.result" | "task.list.snapshot" | "task.list"
    ) {
        return Ok(None);
    }
    let Some(payload) = envelope.payload else {
        return Ok(Some(SocketEvent::TaskListLoaded(Vec::new())));
    };
    let data = payload.get("data").unwrap_or(&payload);
    Ok(Some(SocketEvent::TaskListLoaded(todos_from_task_data(
        data,
    ))))
}

fn parse_status_snapshot(raw: &str) -> Result<Option<SocketEvent>, String> {
    let envelope: SocketEnvelope = serde_json::from_str(raw).map_err(|error| error.to_string())?;
    if !matches!(
        envelope.message_type.as_str(),
        "gateway.status.snapshot" | "gateway.status" | "status.snapshot"
    ) {
        return Ok(None);
    }
    let Some(payload) = envelope.payload else {
        return Ok(None);
    };
    let data = payload.get("data").unwrap_or(&payload);
    Ok(Some(SocketEvent::StatusLoaded(status_from_data(data))))
}

fn parse_fork_memory_snapshot(raw: &str) -> Result<Option<SocketEvent>, String> {
    let envelope: SocketEnvelope = serde_json::from_str(raw).map_err(|error| error.to_string())?;
    if !matches!(
        envelope.message_type.as_str(),
        "fork.memory.snapshot"
            | "memory.fork.snapshot"
            | "fork.memory"
            | "fork.memory.result"
            | "fork.list.snapshot"
    ) {
        return Ok(None);
    }
    let Some(payload) = envelope.payload else {
        return Ok(Some(SocketEvent::ForkMemoryLoaded(
            ForkMemorySnapshot::default(),
        )));
    };
    let data = payload.get("data").unwrap_or(&payload);
    Ok(Some(SocketEvent::ForkMemoryLoaded(fork_memory_from_data(
        data,
    ))))
}

fn parse_execution_job_snapshot(raw: &str) -> Result<Option<SocketEvent>, String> {
    let envelope: SocketEnvelope = serde_json::from_str(raw).map_err(|error| error.to_string())?;
    if envelope.message_type != "execution.job.snapshot" {
        return Ok(None);
    }
    let Some(payload) = envelope.payload else {
        return Ok(None);
    };
    Ok(Some(SocketEvent::ExecutionJobSnapshot(json!({
        "type": envelope.message_type,
        "payload": payload
    }))))
}

fn parse_subscription_event(raw: &str) -> Result<Option<SocketEvent>, String> {
    let envelope: SocketEnvelope = serde_json::from_str(raw).map_err(|error| error.to_string())?;
    if !matches!(
        envelope.message_type.as_str(),
        "event.publish" | "event.snapshot" | "event"
    ) {
        return Ok(None);
    }
    let Some(payload) = envelope.payload else {
        return Ok(None);
    };
    let event = payload
        .get("event")
        .cloned()
        .unwrap_or_else(|| payload.clone());
    Ok(Some(SocketEvent::RuntimeEvent(event)))
}

fn runtime_event_type(event: &Value) -> Option<&str> {
    event
        .get("type")
        .or_else(|| event.get("eventType"))
        .or_else(|| event.get("name"))
        .and_then(Value::as_str)
        .or_else(|| {
            event
                .get("event")
                .and_then(|event| event.get("type"))
                .and_then(Value::as_str)
        })
}

fn job_id_from_event_payload(event: &Value) -> Option<String> {
    for source in [
        Some(event),
        event.get("data"),
        event.get("payload"),
        event.get("payload").and_then(|payload| payload.get("data")),
        event.get("payload").and_then(|payload| payload.get("job")),
        event
            .get("payload")
            .and_then(|payload| payload.get("event")),
        event
            .get("payload")
            .and_then(|payload| payload.get("payload")),
        event.get("event"),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(job_id) = value_string(source, "jobId") {
            return Some(job_id);
        }
    }
    None
}

fn event_text_from_payload(payload: &Value) -> Option<String> {
    for source in [
        Some(payload),
        payload.get("data"),
        payload.get("event"),
        payload.get("event").and_then(|event| event.get("data")),
        payload.get("payload"),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(text) = value_string(source, "text")
            .or_else(|| value_string(source, "message"))
            .or_else(|| value_string(source, "content"))
            .or_else(|| value_string(source, "summary"))
        {
            return Some(text);
        }
    }
    None
}

fn parse_context_snapshot(raw: &str) -> Result<Option<SocketEvent>, String> {
    let envelope: SocketEnvelope = serde_json::from_str(raw).map_err(|error| error.to_string())?;
    if !matches!(
        envelope.message_type.as_str(),
        "thought.snapshot"
            | "recall.snapshot"
            | "memory.snapshot"
            | "blackboard.snapshot"
            | "ask.snapshot"
    ) {
        return Ok(None);
    }
    let Some(payload) = envelope.payload else {
        return Ok(None);
    };
    Ok(Some(SocketEvent::ContextSnapshotLoaded(json!({
        "kind": envelope.message_type,
        "payload": payload
    }))))
}

fn history_turn_snapshot_to_turn(snapshot: HistoryTurnSnapshot) -> Turn {
    let metadata = history_snapshot_metadata(&snapshot);
    let answer = turn_final_visible_text(&snapshot.assistant_text, metadata.as_ref());
    Turn {
        message_id: metadata
            .as_ref()
            .and_then(|metadata| value_string(metadata, "messageId")),
        event_id: Some(snapshot.event_id.clone()),
        user: snapshot.user_text,
        thought: None,
        answer,
        context_rows: context_rows_from_metadata(&metadata),
        metadata,
        pending_continuation: None,
        footer: format!(
            "flyflor history · {} · {}",
            snapshot.event_id,
            iso8601_from_millis(snapshot.ts)
        ),
    }
}

fn turn_final_visible_text(text: &str, metadata: Option<&Value>) -> String {
    ask_prompt_from_metadata(metadata)
        .map(|prompt| prompt.to_string())
        .unwrap_or_else(|| text.to_string())
}

fn ask_prompt_from_metadata(metadata: Option<&Value>) -> Option<&str> {
    let metadata = metadata?;
    if metadata.get("kind").and_then(Value::as_str) != Some("ask") && metadata.get("ask").is_none()
    {
        return None;
    }
    metadata
        .get("ask")
        .and_then(|ask| ask.get("prompt"))
        .or_else(|| metadata.get("prompt"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
}

fn turn_from_context_snapshot(snapshot: &Value) -> Option<Turn> {
    let kind = snapshot.get("kind").and_then(Value::as_str)?;
    let payload = snapshot.get("payload")?;
    let data = payload.get("data").unwrap_or(payload);
    let metadata = match kind {
        "thought.snapshot" => {
            let thought = data.get("thought").unwrap_or(data);
            json!({
                "thought": thought
            })
        }
        "recall.snapshot" | "memory.snapshot" => {
            let recall = data
                .get("recall")
                .or_else(|| data.get("memory"))
                .unwrap_or(data);
            json!({ "recall": recall })
        }
        "blackboard.snapshot" => {
            let blackboard = data.get("blackboard").unwrap_or(data);
            json!({
                "planning": {
                    "replays": [{
                        "id": value_string(blackboard, "id").unwrap_or_else(|| "blackboard".to_string()),
                        "kind": "blackboard",
                        "title": value_string(blackboard, "title")
                            .or_else(|| value_string(blackboard, "summary"))
                            .unwrap_or_else(|| "blackboard 摘要".to_string()),
                        "summary": value_string(blackboard, "summary")
                            .or_else(|| value_string(blackboard, "content"))
                            .unwrap_or_else(|| "blackboard snapshot".to_string())
                    }]
                }
            })
        }
        "ask.snapshot" => {
            let ask = data.get("ask").unwrap_or(data).clone();
            json!({ "ask": ask })
        }
        _ => return None,
    };
    Some(Turn {
        message_id: value_string(data, "messageId"),
        event_id: value_string(data, "eventId").or_else(|| value_string(data, "id")),
        user: format!("socket {kind}"),
        thought: None,
        answer: match kind {
            "thought.snapshot" => "收到思考摘要。".to_string(),
            "recall.snapshot" | "memory.snapshot" => "收到回忆摘要。".to_string(),
            "blackboard.snapshot" => "收到 blackboard 摘要。".to_string(),
            "ask.snapshot" => "需要用户选择 ASK 回答。".to_string(),
            _ => kind.to_string(),
        },
        context_rows: context_rows_from_metadata(&Some(metadata.clone())),
        metadata: Some(metadata),
        pending_continuation: None,
        footer: format!("flyflor · {kind}"),
    })
}

fn history_snapshot_metadata(snapshot: &HistoryTurnSnapshot) -> Option<Value> {
    let mut merged = snapshot
        .metadata
        .as_ref()
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    if let Some(value) = &snapshot.context_forks {
        let planning = merged
            .entry("planning".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(planning) = planning.as_object_mut() {
            planning.insert("contextForks".to_string(), value.clone());
        }
    }
    if let Some(value) = &snapshot.replays {
        let planning = merged
            .entry("planning".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(planning) = planning.as_object_mut() {
            planning.insert("replays".to_string(), value.clone());
        }
    }
    if let Some(value) = &snapshot.task_plans {
        let planning = merged
            .entry("planning".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(planning) = planning.as_object_mut() {
            planning.insert("taskPlans".to_string(), value.clone());
        }
    }
    if let Some(value) = &snapshot.executive_tool_executions {
        merged.insert("executiveToolExecutions".to_string(), value.clone());
    }

    if merged.is_empty() {
        None
    } else {
        Some(Value::Object(merged))
    }
}

fn todos_from_turns(turns: &[Turn]) -> Vec<TodoItem> {
    turns
        .iter()
        .filter_map(|turn| turn.metadata.as_ref())
        .filter_map(|metadata| {
            metadata
                .get("planning")
                .and_then(|planning| planning.get("taskPlans"))
        })
        .flat_map(todos_from_task_plans)
        .collect()
}

fn todos_from_task_data(data: &Value) -> Vec<TodoItem> {
    if let Some(task_plans) = data.get("taskPlans").or_else(|| data.get("plans")) {
        return todos_from_task_plans(task_plans);
    }
    if let Some(tasks) = data.get("tasks").or_else(|| data.get("items")) {
        return todos_from_task_plans(tasks);
    }
    todos_from_task_plans(data)
}

fn status_from_data(data: &Value) -> StatusSnapshot {
    let status = data.get("status").unwrap_or(data);
    let model = status
        .get("model")
        .or_else(|| data.get("model"))
        .unwrap_or(status);
    let context = status
        .get("context")
        .or_else(|| data.get("context"))
        .or_else(|| data.get("telemetry"))
        .or_else(|| data.get("cache"));
    StatusSnapshot {
        context_window_tokens: value_usize(model, "contextWindowTokens")
            .or_else(|| context.and_then(|value| value_usize(value, "contextWindowTokens")))
            .or_else(|| value_usize(status, "contextWindowTokens"))
            .or_else(|| value_usize(data, "contextWindowTokens")),
        max_output_tokens: value_usize(model, "maxOutputTokens")
            .or_else(|| value_usize(status, "maxOutputTokens"))
            .or_else(|| value_usize(data, "maxOutputTokens")),
        hot_context_tokens: value_usize(model, "contextUsedTokens")
            .or_else(|| value_usize(model, "currentTokens"))
            .or_else(|| context.and_then(|value| value_usize(value, "contextUsedTokens")))
            .or_else(|| context.and_then(|value| value_usize(value, "currentTokens")))
            .or_else(|| value_usize(status, "contextUsedTokens"))
            .or_else(|| value_usize(status, "currentTokens"))
            .or_else(|| context_telemetry_usize(data, "hotContextTokens"))
            .or_else(|| context_telemetry_usize(data, "usedContextTokens"))
            .or_else(|| context_telemetry_usize(data, "contextTokens")),
        context_window_percent: value_f64(model, "contextWindowPercent")
            .or_else(|| context.and_then(|value| value_f64(value, "contextWindowPercent")))
            .or_else(|| value_f64(status, "contextWindowPercent"))
            .or_else(|| value_f64(data, "contextWindowPercent")),
        context_status: value_string(model, "contextStatus")
            .or_else(|| context.and_then(|value| value_string(value, "contextStatus")))
            .or_else(|| value_string(status, "contextStatus"))
            .or_else(|| value_string(data, "contextStatus")),
        remaining_context_tokens: context
            .and_then(|value| value_usize(value, "remainingContextTokens"))
            .or_else(|| context_telemetry_usize(data, "remainingContextTokens")),
        cache_read_tokens: context
            .and_then(|value| value_usize(value, "cacheReadTokens"))
            .or_else(|| context_telemetry_usize(data, "cacheReadTokens"))
            .or_else(|| context_telemetry_usize(data, "cachedInputTokens")),
        cache_write_tokens: context
            .and_then(|value| value_usize(value, "cacheWriteTokens"))
            .or_else(|| context_telemetry_usize(data, "cacheWriteTokens")),
        model_name: value_string(model, "model")
            .or_else(|| value_string(model, "name"))
            .or_else(|| value_string(model, "id"))
            .or_else(|| value_string(status, "modelName"))
            .or_else(|| value_string(data, "modelName"))
            .or_else(|| value_string(data, "model")),
        model_provider: value_string(model, "provider")
            .or_else(|| value_string(model, "providerId"))
            .or_else(|| value_string(status, "provider"))
            .or_else(|| value_string(status, "providerId"))
            .or_else(|| value_string(data, "provider"))
            .or_else(|| value_string(data, "providerId")),
    }
}

fn context_telemetry_usize(data: &Value, key: &str) -> Option<usize> {
    data.get("context")
        .and_then(|value| value_usize(value, key))
        .or_else(|| {
            data.get("telemetry")
                .and_then(|value| value_usize(value, key))
        })
        .or_else(|| data.get("cache").and_then(|value| value_usize(value, key)))
        .or_else(|| value_usize(data, key))
}

fn value_usize(value: &Value, key: &str) -> Option<usize> {
    value
        .get(key)?
        .as_u64()
        .and_then(|number| usize::try_from(number).ok())
}

fn value_f64(value: &Value, key: &str) -> Option<f64> {
    value.get(key)?.as_f64()
}

fn todos_from_task_plans(task_plans: &Value) -> Vec<TodoItem> {
    match task_plans {
        Value::Array(plans) => plans.iter().flat_map(todo_items_from_task_plan).collect(),
        Value::Object(_) => todo_items_from_task_plan(task_plans),
        _ => Vec::new(),
    }
}

fn todo_items_from_task_plan(plan: &Value) -> Vec<TodoItem> {
    let steps = plan
        .get("steps")
        .or_else(|| plan.get("todos"))
        .or_else(|| plan.get("items"))
        .and_then(Value::as_array);
    let Some(steps) = steps else {
        return Vec::new();
    };

    let active_index = active_step_index(plan, steps);
    steps
        .iter()
        .enumerate()
        .map(|(index, step)| {
            let status = value_string(step, "status")
                .or_else(|| value_string(step, "state"))
                .unwrap_or_else(|| "todo".to_string());
            let active = active_index == Some(index);
            TodoItem {
                marker: todo_marker(&status, active).to_string(),
                label: value_string(step, "title")
                    .or_else(|| value_string(step, "label"))
                    .or_else(|| value_string(step, "text"))
                    .unwrap_or_else(|| format!("步骤 {}", index + 1)),
                status: todo_status_label(&status, active),
                active,
                plan_id: plan_id_from_task_plan(plan),
            }
        })
        .collect()
}

fn plan_id_from_task_plan(plan: &Value) -> Option<String> {
    value_string(plan, "planId")
        .or_else(|| value_string(plan, "id"))
        .or_else(|| value_string(plan, "taskPlanId"))
}

fn plan_id_from_metadata(metadata: &Value) -> Option<String> {
    metadata
        .get("planning")
        .and_then(|planning| planning.get("taskPlans"))
        .and_then(|task_plans| match task_plans {
            Value::Array(plans) => plans.iter().find_map(plan_id_from_task_plan),
            Value::Object(_) => plan_id_from_task_plan(task_plans),
            _ => None,
        })
}

fn active_step_index(plan: &Value, steps: &[Value]) -> Option<usize> {
    if let Some(index) = steps.iter().position(step_is_current) {
        return Some(index);
    }
    if let Some(current) = plan.get("currentStep").and_then(Value::as_u64) {
        let index = current as usize;
        if index < steps.len() {
            return Some(index);
        }
        if current > 0 && (current as usize) <= steps.len() {
            return Some(current as usize - 1);
        }
    }
    steps.iter().position(|step| {
        let status = value_string(step, "status")
            .or_else(|| value_string(step, "state"))
            .unwrap_or_default();
        !status_matches_done(&status)
    })
}

fn step_is_current(step: &Value) -> bool {
    step.get("active").and_then(Value::as_bool).unwrap_or(false)
        || value_string(step, "status")
            .or_else(|| value_string(step, "state"))
            .is_some_and(|status| status_matches_current(&status))
}

fn status_matches_current(status: &str) -> bool {
    matches!(
        normalized_status(status).as_str(),
        "active" | "current" | "running" | "in_progress" | "in-progress" | "doing"
    ) || matches!(status, "进行中" | "当前")
}

fn status_matches_done(status: &str) -> bool {
    matches!(
        normalized_status(status).as_str(),
        "done" | "completed" | "complete"
    ) || status == "完成"
}

fn status_matches_awaiting_confirmation(status: &str) -> bool {
    matches!(
        normalized_status(status).as_str(),
        "awaiting confirmation" | "needs confirmation" | "waiting confirmation" | "needs_user"
    ) || status.contains("等待确认")
        || status.contains("待确认")
}

fn normalized_status(status: &str) -> String {
    status.trim().to_ascii_lowercase()
}

fn todo_marker(status: &str, active: bool) -> &'static str {
    if status_matches_done(status) {
        "✓"
    } else if active {
        "›"
    } else {
        "○"
    }
}

fn todo_status_label(status: &str, active: bool) -> String {
    if status_matches_awaiting_confirmation(status) {
        "等待确认".to_string()
    } else if active {
        "进行中".to_string()
    } else if status_matches_done(status) {
        "完成".to_string()
    } else if status.trim().is_empty() {
        "待办".to_string()
    } else {
        truncate_to_width(status, 6)
    }
}

fn plan_state_from_todos(todos: &[TodoItem]) -> PlanState {
    if todos.is_empty() || todos.iter().all(|todo| todo.label == "暂无计划") {
        return PlanState::Empty;
    }
    let joined = todos
        .iter()
        .map(|todo| todo.status.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    if status_matches_awaiting_confirmation(&joined) {
        PlanState::AwaitingConfirmation
    } else if joined.contains("生成中") {
        PlanState::Generating
    } else if joined.contains("已放弃") || joined.contains("放弃") {
        PlanState::Abandoned
    } else if todos.iter().any(|todo| todo.active) {
        PlanState::Running
    } else {
        PlanState::Empty
    }
}

fn plan_id_waiting_for_confirmation(todos: &[TodoItem]) -> Option<String> {
    todos
        .iter()
        .find(|todo| status_matches_awaiting_confirmation(&todo.status))
        .and_then(|todo| todo.plan_id.clone())
}

fn error_message_from_payload(payload: &Option<Value>) -> String {
    let Some(payload) = payload else {
        return "socket error".to_string();
    };
    payload
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| payload.get("error").and_then(Value::as_str))
        .unwrap_or("socket error")
        .to_string()
}

fn context_rows_from_metadata(metadata: &Option<Value>) -> Vec<ContextRow> {
    let mut rows = Vec::new();
    let Some(metadata) = metadata else {
        rows.push(ContextRow {
            kind: ContextRowKind::CreateFork,
            summary: "从本轮创建 context fork".to_string(),
            detail: String::new(),
            expanded: false,
        });
        return rows;
    };

    if let Some(ask) = metadata.get("ask") {
        let summary = value_string(ask, "prompt").unwrap_or_else(|| "等待用户确认".to_string());
        if continuation_from_value(ask).is_some() {
            rows.push(ContextRow {
                kind: ContextRowKind::AskResume,
                summary,
                detail: format_ask_detail(ask),
                expanded: false,
            });
        }
    }

    if let Some(continuation) = metadata.get("continuation")
        && !rows.iter().any(|row| row.kind == ContextRowKind::AskResume)
        && continuation_from_value(continuation).is_some()
    {
        rows.push(ContextRow {
            kind: ContextRowKind::AskResume,
            summary: value_string(continuation, "summary")
                .or_else(|| value_string(continuation, "title"))
                .unwrap_or_else(|| "继续未完成回答".to_string()),
            detail: format_ask_detail(continuation),
            expanded: false,
        });
    }

    if let Some(blackboard) = metadata.get("blackboard") {
        rows.push(ContextRow {
            kind: ContextRowKind::Blackboard,
            summary: blackboard_summary(blackboard),
            detail: format_blackboard_discussion(blackboard),
            expanded: true,
        });
    }

    if let Some(thought) = metadata
        .get("thought")
        .or_else(|| metadata.get("thinking"))
        .or_else(|| metadata.get("directWithWatch"))
        .or_else(|| metadata.get("direct-with-watch"))
    {
        rows.push(ContextRow {
            kind: ContextRowKind::Thought,
            summary: context_value_summary(thought, "思考详情"),
            detail: format_thought_detail(thought),
            expanded: false,
        });
    }

    if let Some(recall) = metadata
        .get("recall")
        .or_else(|| metadata.get("memory"))
        .or_else(|| metadata.get("scopeMemory"))
    {
        rows.push(ContextRow {
            kind: ContextRowKind::Recall,
            summary: context_value_summary(recall, "召回记忆"),
            detail: format_recall_detail(recall),
            expanded: false,
        });
    }

    if let Some(planning) = metadata.get("planning") {
        if let Some(forks) = planning.get("contextForks").and_then(Value::as_array) {
            if let Some(row) = aggregate_context_row(ContextRowKind::Fork, forks, "context fork") {
                rows.push(row);
            }
        }
        if let Some(replays) = planning.get("replays").and_then(Value::as_array) {
            let mut blackboard = Vec::new();
            let mut recall = Vec::new();
            for replay in replays {
                let kind = value_string(replay, "kind").unwrap_or_default();
                if kind == "blackboard" {
                    blackboard.push(replay.clone());
                } else {
                    recall.push(replay.clone());
                }
            }
            if let Some(row) = aggregate_context_row(ContextRowKind::Recall, &recall, "replay") {
                rows.push(row);
            }
            if !rows
                .iter()
                .any(|row| row.kind == ContextRowKind::Blackboard)
                && let Some(row) =
                    aggregate_context_row(ContextRowKind::Blackboard, &blackboard, "blackboard")
            {
                rows.push(row);
            }
        }
    }

    if let Some(row) = execution_context_row_from_metadata(metadata) {
        rows.push(row);
    }

    rows.push(ContextRow {
        kind: ContextRowKind::CreateFork,
        summary: "从本轮创建 context fork".to_string(),
        detail: String::new(),
        expanded: false,
    });

    rows
}

fn execution_context_row_from_metadata(metadata: &Value) -> Option<ContextRow> {
    let ask_loop = metadata
        .get("ask")
        .and_then(|ask| ask.get("executiveToolLoop"))
        .or_else(|| metadata.get("executiveToolLoop"));
    let tool_executions = metadata
        .get("executiveToolExecutions")
        .and_then(Value::as_array);
    if ask_loop.is_none() && tool_executions.is_none() {
        return None;
    }

    let mut detail = Vec::new();
    if let Some(loop_snapshot) = ask_loop {
        detail.push("执行层 ASK / 外骨骼暂停".to_string());
        push_detail_field(
            &mut detail,
            "askId",
            value_string(loop_snapshot, "askId").or_else(|| value_string(loop_snapshot, "id")),
        );
        push_detail_field(
            &mut detail,
            "原因",
            value_string(loop_snapshot, "message")
                .or_else(|| value_string(loop_snapshot, "reason"))
                .or_else(|| value_string(loop_snapshot, "stop")),
        );
        push_detail_field(
            &mut detail,
            "工具步数",
            loop_snapshot
                .get("stepCount")
                .and_then(Value::as_u64)
                .map(|value| value.to_string()),
        );
    }
    if let Some(tool_executions) = tool_executions {
        detail.push(format!("工具调用：{}", tool_executions.len()));
        for execution in tool_executions.iter().take(8) {
            detail.push(format!("- {}", compact_tool_execution_label(execution, 96)));
        }
    }

    Some(ContextRow {
        kind: ContextRowKind::Execution,
        summary: execution_metadata_summary(ask_loop, tool_executions),
        detail: detail.join("\n"),
        expanded: false,
    })
}

fn execution_metadata_summary(
    ask_loop: Option<&Value>,
    tool_executions: Option<&Vec<Value>>,
) -> String {
    let tool_count = tool_executions.map(Vec::len).unwrap_or(0);
    let stop = ask_loop
        .and_then(|value| value_string(value, "stop"))
        .unwrap_or_else(|| "visible".to_string());
    if tool_count > 0 {
        format!("工具/子进程 {tool_count} · Plan {stop}")
    } else {
        format!("工具阻断 · Plan {stop}")
    }
}

fn compact_tool_execution_label(value: &Value, width: usize) -> String {
    let label = value_string(value, "tool")
        .or_else(|| value_string(value, "name"))
        .or_else(|| value_string(value, "command"))
        .or_else(|| value_string(value, "id"))
        .unwrap_or_else(|| "tool".to_string());
    let status = value_string(value, "status")
        .or_else(|| value_string(value, "state"))
        .unwrap_or_else(|| "unknown".to_string());
    truncate_to_width(&format!("{label} · {status}"), width)
}

fn execution_context_rows(timeline: &RunTimeline) -> Vec<ContextRow> {
    let mut children = Vec::new();
    for batch in &timeline.subagents.batches {
        for child in &batch.children {
            children.push((Some(batch.name.as_str()), child));
        }
    }
    for child in &timeline.subagents.loose_children {
        children.push((None, child));
    }

    let total = children.len();
    if total == 0 {
        if timeline.items.is_empty() {
            return Vec::new();
        }
        return vec![ContextRow {
            kind: ContextRowKind::Execution,
            summary: execution_context_summary(timeline),
            detail: execution_context_detail(timeline),
            expanded: false,
        }];
    }

    children
        .into_iter()
        .enumerate()
        .map(|(index, (batch_name, child))| ContextRow {
            kind: ContextRowKind::Execution,
            summary: subagent_execution_summary(index + 1, total, child),
            detail: subagent_execution_detail(batch_name, child),
            expanded: false,
        })
        .collect()
}

fn subagent_execution_summary(
    index: usize,
    total: usize,
    child: &tui::subagent::state::SubagentChild,
) -> String {
    let task = child
        .task
        .as_deref()
        .filter(|task| !task.trim().is_empty())
        .unwrap_or(child.name.as_str());
    let tool_count = child.tool_calls.len();
    let process_count = child.processes.len()
        + child
            .tool_calls
            .iter()
            .map(|tool| tool.processes.len())
            .sum::<usize>();
    let limited = if child.limited { " · partial" } else { "" };
    format!(
        "({index}/{total}) {} | {} · {}{limited} · 工具 {tool_count} · 子进程 {process_count}",
        truncate_to_width(&child.name, 24),
        truncate_to_width(task, 42),
        child.status.as_str()
    )
}

fn subagent_execution_detail(
    batch_name: Option<&str>,
    child: &tui::subagent::state::SubagentChild,
) -> String {
    let mut lines = vec![format!("子代理ID: {}", child.id)];
    if let Some(batch_name) = batch_name {
        lines.push(format!("batch: {batch_name}"));
    }
    lines.push(format!("状态: {}", child.status.as_str()));
    if child.limited || child.suppressed_ask_required {
        lines.push(format!(
            "限制: {}{}",
            child.limit_reason.as_deref().unwrap_or("partial-result"),
            if child.suppressed_ask_required {
                " · ASK suppressed"
            } else {
                ""
            }
        ));
    }
    if let Some(job_id) = &child.job_id {
        lines.push(format!("jobId: {job_id}"));
    }
    if let Some(task) = &child.task {
        lines.push(format!("任务: {task}"));
    }
    if let Some(model) = &child.model {
        lines.push(format!("模型: {}", model_allocation_label(model)));
        if let Some(reason) = &model.reason {
            lines.push(format!("模型分配: {reason}"));
        }
    }
    if !child.allowed_tools.is_empty() {
        lines.push(format!("allowed tools: {}", child.allowed_tools.join(", ")));
    }
    if child.tool_calls.is_empty() && child.processes.is_empty() {
        lines.push("执行过程: 暂无工具/子进程事件".to_string());
    }
    for tool in &child.tool_calls {
        lines.push(format!(
            "- tool {} · {}",
            subagent_tool_label(tool),
            tool.status.as_str()
        ));
        if let Some(args) = &tool.args_preview {
            lines.push(format!("  args: {}", truncate_to_width(args, 120)));
        }
        if let Some(output_path) = &tool.output_path {
            lines.push(format!("  output: {output_path}"));
        }
        if let Some(error) = &tool.error {
            lines.push(format!("  error: {}", truncate_to_width(error, 120)));
        }
        for process in &tool.processes {
            lines.push(format!(
                "  - process {} · {}",
                subagent_process_label(process),
                process.status.as_str()
            ));
        }
    }
    for process in &child.processes {
        lines.push(format!(
            "- process {} · {}",
            subagent_process_label(process),
            process.status.as_str()
        ));
    }
    if let Some(ask) = &child.ask {
        lines.push(format!("ASK: {} · {}", ask.id, ask.status.as_str()));
        if let Some(reason) = &ask.reason {
            lines.push(format!("ASK reason: {reason}"));
        }
        if let Some(crystal) = &ask.crystal_candidate {
            lines.push(format!("crystal candidate: {crystal}"));
        }
    }
    if let Some(crystal) = &child.crystal {
        lines.push(format!("crystal: {crystal}"));
    }
    lines.join("\n")
}

fn model_allocation_label(model: &tui::subagent::state::ModelAllocation) -> String {
    match (&model.provider_id, &model.model_id) {
        (Some(provider), Some(model_id)) => format!("{provider}/{model_id}"),
        (None, Some(model_id)) => model_id.clone(),
        (Some(provider), None) => provider.clone(),
        (None, None) => model.id.clone(),
    }
}

fn subagent_tool_label(tool: &tui::subagent::state::SubagentToolCall) -> String {
    let name = tool
        .tool
        .clone()
        .or_else(|| tool.command.clone())
        .unwrap_or_else(|| tool.name.clone());
    tool.server
        .as_ref()
        .map(|server| format!("{server}/{name}"))
        .unwrap_or(name)
}

fn subagent_process_label(process: &tui::subagent::state::SubagentProcess) -> String {
    process
        .command
        .clone()
        .or_else(|| process.output_path.clone())
        .unwrap_or_else(|| process.id.clone())
}

fn execution_row_identity(row: &ContextRow) -> Option<String> {
    row.detail
        .lines()
        .find_map(|line| line.strip_prefix("子代理ID: ").map(str::to_string))
        .or_else(|| Some(row.summary.clone()))
}

fn execution_context_summary(timeline: &RunTimeline) -> String {
    let model_count = timeline.subagents.models.len();
    let batch_count = timeline.subagents.batches.len();
    let child_count = timeline
        .subagents
        .batches
        .iter()
        .map(|batch| batch.children.len())
        .sum::<usize>()
        + timeline.subagents.loose_children.len();
    let tool_count = timeline.subagents.loose_tool_calls.len()
        + timeline
            .subagents
            .batches
            .iter()
            .map(|batch| {
                batch
                    .children
                    .iter()
                    .map(|child| child.tool_calls.len())
                    .sum::<usize>()
            })
            .sum::<usize>()
        + timeline
            .subagents
            .loose_children
            .iter()
            .map(|child| child.tool_calls.len())
            .sum::<usize>();
    let process_count = timeline.subagents.loose_processes.len()
        + timeline
            .subagents
            .batches
            .iter()
            .map(|batch| {
                batch
                    .children
                    .iter()
                    .map(|child| child.processes.len())
                    .sum::<usize>()
            })
            .sum::<usize>()
        + timeline
            .subagents
            .loose_children
            .iter()
            .map(|child| child.processes.len())
            .sum::<usize>();
    let ask_count = timeline.subagents.loose_asks.len()
        + timeline
            .subagents
            .batches
            .iter()
            .map(|batch| {
                batch
                    .children
                    .iter()
                    .filter(|child| child.ask.is_some())
                    .count()
            })
            .sum::<usize>()
        + timeline
            .subagents
            .loose_children
            .iter()
            .filter(|child| child.ask.is_some())
            .count();
    let status = if ask_count > 0 {
        "等待 ASK"
    } else if timeline.items.iter().any(|item| {
        matches!(
            item.status,
            tui::run_timeline::state::RunTimelineItemStatus::Running
        )
    }) {
        "进行中"
    } else {
        "已记录"
    };
    format!(
        "{status} · 模型 {model_count} · 子代理 {batch_count}/{child_count} · 工具 {tool_count} · 子进程 {process_count}"
    )
}

fn execution_context_detail(timeline: &RunTimeline) -> String {
    run_panel_lines(timeline)
        .into_iter()
        .skip(1)
        .take(18)
        .map(|line| line_plain_text(&line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn aggregate_context_row(
    kind: ContextRowKind,
    values: &[Value],
    fallback_label: &str,
) -> Option<ContextRow> {
    let deduped = dedupe_context_values(values);
    let latest = deduped.last()?;
    let latest_summary = context_value_summary(latest, fallback_label);
    let summary = if deduped.len() == 1 {
        latest_summary
    } else {
        format!("{} 条摘要 · 最近：{}", deduped.len(), latest_summary)
    };
    let merged_count = values.len().saturating_sub(deduped.len());
    Some(ContextRow {
        kind,
        summary,
        detail: format_context_detail(kind, &deduped, fallback_label, merged_count),
        expanded: false,
    })
}

fn dedupe_context_values(values: &[Value]) -> Vec<Value> {
    let mut deduped = Vec::new();
    for value in values {
        let key = context_value_key(value);
        if let Some(existing) = deduped
            .iter()
            .position(|existing| context_value_key(existing) == key)
        {
            deduped[existing] = value.clone();
            continue;
        }
        deduped.push(value.clone());
    }
    deduped
}

fn context_value_key(value: &Value) -> String {
    let source_key = value_string(value, "source")
        .or_else(|| value_string(value, "sourceEventId"))
        .or_else(|| value_string(value, "sourceKey"));
    if source_key.is_none()
        && let Some(id) = value_string(value, "id").or_else(|| value_string(value, "eventId"))
    {
        return id;
    }
    let key = [
        source_key,
        value_string(value, "title").or_else(|| value_string(value, "inputSummary")),
        value_string(value, "summary"),
        value_string(value, "continuitySummary"),
    ]
    .into_iter()
    .flatten()
    .map(|part| normalize_context_key_part(&part))
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join("|");
    if key.is_empty() {
        value_string(value, "id")
            .or_else(|| value_string(value, "eventId"))
            .unwrap_or_else(|| serde_json::to_string(value).unwrap_or_else(|_| value.to_string()))
    } else {
        key
    }
}

fn normalize_context_key_part(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn context_value_summary(value: &Value, fallback_label: &str) -> String {
    value_string(value, "title")
        .or_else(|| value_string(value, "summary"))
        .or_else(|| value_string(value, "continuitySummary"))
        .unwrap_or_else(|| {
            kind_or_default(
                &value_string(value, "kind").unwrap_or_default(),
                fallback_label,
            )
        })
}

fn format_context_detail(
    kind: ContextRowKind,
    values: &[Value],
    fallback_label: &str,
    merged_count: usize,
) -> String {
    let mut items = values
        .iter()
        .enumerate()
        .map(|(index, value)| format_context_detail_item(kind, index + 1, value, fallback_label))
        .collect::<Vec<_>>();
    if kind == ContextRowKind::Fork && merged_count > 0 {
        items.push(format!("已合并 {merged_count} 条重复 fork"));
    }
    items.join("\n\n")
}

fn format_context_detail_item(
    kind: ContextRowKind,
    index: usize,
    value: &Value,
    fallback_label: &str,
) -> String {
    if kind == ContextRowKind::Fork {
        return format_fork_context_detail_item(index, value, fallback_label);
    }
    if kind == ContextRowKind::Blackboard {
        return format_blackboard_discussion(value);
    }
    if kind == ContextRowKind::Recall {
        return format_recall_detail(value);
    }
    if kind == ContextRowKind::Thought {
        return format_thought_detail(value);
    }
    let title = context_value_summary(value, fallback_label);
    let mut lines = vec![format!("{index}. {title}")];
    push_detail_field(&mut lines, "id", value_string(value, "id"));
    push_detail_field(&mut lines, "摘要", value_string(value, "summary"));
    push_detail_field(
        &mut lines,
        "延续摘要",
        value_string(value, "continuitySummary"),
    );
    push_detail_field(
        &mut lines,
        "来源",
        value_string(value, "sourceEventId")
            .or_else(|| value_string(value, "eventId"))
            .or_else(|| value_string(value, "sourceAskId"))
            .or_else(|| value_string(value, "sourceBlackboardTurnId")),
    );
    lines.join("\n")
}

fn format_fork_context_detail_item(index: usize, value: &Value, fallback_label: &str) -> String {
    let title = context_value_summary(value, fallback_label);
    let mut lines = vec![format!(
        "{index}. {}",
        truncate_to_width(&title.replace('\n', " "), 72)
    )];
    let summary = value_string(value, "summary");
    let continuity = value_string(value, "continuitySummary");
    if summary.as_deref().map(str::trim) != Some(title.trim()) {
        push_detail_field(
            &mut lines,
            "摘要",
            summary
                .as_ref()
                .map(|value| truncate_to_width(&value.replace('\n', " "), 96)),
        );
    }
    if continuity.as_deref().map(str::trim) != summary.as_deref().map(str::trim) {
        push_detail_field(
            &mut lines,
            "延续",
            continuity.map(|value| truncate_to_width(&value.replace('\n', " "), 96)),
        );
    }
    lines.join("\n")
}

fn format_recall_detail(value: &Value) -> String {
    let mut lines = vec![format!(
        "### scope 记忆装配：{}",
        context_value_summary(value, "召回记忆")
    )];
    push_detail_field(&mut lines, "scope", value_string(value, "scopeId"));
    push_detail_field(
        &mut lines,
        "摘要",
        value_string(value, "summary").or_else(|| value_string(value, "content")),
    );
    if let Some(items) = value
        .get("items")
        .or_else(|| value.get("memories"))
        .or_else(|| value.get("recalls"))
        .and_then(Value::as_array)
    {
        for (index, item) in items.iter().enumerate() {
            lines.push(format!(
                "{}. {}",
                index + 1,
                context_value_summary(item, "memory")
            ));
            push_detail_field(
                &mut lines,
                "   内容",
                value_string(item, "content").or_else(|| value_string(item, "summary")),
            );
        }
    }
    lines.join("\n\n")
}

fn format_thought_detail(value: &Value) -> String {
    let mut lines = vec![format!(
        "### LLM 思考详情：{}",
        context_value_summary(value, "思考详情")
    )];
    push_detail_field(&mut lines, "状态", value_string(value, "status"));
    push_detail_field(
        &mut lines,
        "摘要",
        value_string(value, "summary").or_else(|| value_string(value, "title")),
    );
    push_detail_field(
        &mut lines,
        "内容",
        value_string(value, "content")
            .or_else(|| value_string(value, "text"))
            .or_else(|| value_string(value, "detail")),
    );
    if let Some(steps) = value
        .get("steps")
        .or_else(|| value.get("messages"))
        .and_then(Value::as_array)
    {
        for (index, step) in steps.iter().enumerate() {
            lines.push(format!(
                "{}. {}",
                index + 1,
                value_string(step, "content")
                    .or_else(|| value_string(step, "text"))
                    .or_else(|| value_string(step, "summary"))
                    .unwrap_or_else(|| "思考步骤".to_string())
            ));
        }
    }
    lines.join("\n\n")
}

fn blackboard_summary(value: &Value) -> String {
    value_string(value, "summary")
        .or_else(|| value_string(value, "title"))
        .or_else(|| value_string(value, "reason"))
        .unwrap_or_else(|| "blackboard 摘要".to_string())
}

fn format_blackboard_discussion(value: &Value) -> String {
    let summary = blackboard_summary(value);
    let status = value_string(value, "status").unwrap_or_else(|| "unknown".to_string());
    let plan = value_string(value, "mode")
        .or_else(|| value_string(value, "plan"))
        .unwrap_or_else(|| "none".to_string());
    let mut lines = vec![format!(
        "Blackboard discussion: Status: {status}; reason: {summary}; plan: {plan}"
    )];
    let content = value.get("content").unwrap_or(value);
    append_blackboard_content(&mut lines, content, 1);
    lines.join("\n\n")
}

fn append_blackboard_content(lines: &mut Vec<String>, value: &Value, round: usize) {
    match value {
        Value::String(text) => {
            if !text.trim().is_empty() {
                lines.push(format!("Round {round}: {}", text.trim()));
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                append_blackboard_content(lines, item, index + 1);
            }
        }
        Value::Object(map) => {
            if let Some(items) = map
                .get("rounds")
                .or_else(|| map.get("messages"))
                .or_else(|| map.get("items"))
                .and_then(Value::as_array)
            {
                for (index, item) in items.iter().enumerate() {
                    append_blackboard_content(lines, item, index + 1);
                }
                return;
            }
            let role = value_string(value, "role")
                .or_else(|| value_string(value, "speaker"))
                .or_else(|| value_string(value, "agent"))
                .unwrap_or_else(|| format!("Round {round}"));
            if let Some(text) = value_string(value, "content")
                .or_else(|| value_string(value, "text"))
                .or_else(|| value_string(value, "message"))
            {
                if !text.trim().is_empty() {
                    lines.push(format!("Round {round} · {role}: {}", text.trim()));
                }
            }
        }
        _ => {}
    }
}

fn push_detail_field(lines: &mut Vec<String>, label: &str, value: Option<String>) {
    let Some(value) = value else {
        return;
    };
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    lines.push(format!("   {label}: {value}"));
}

fn format_ask_detail(value: &Value) -> String {
    let mut lines = vec!["ASK 续答上下文".to_string()];
    push_detail_field(&mut lines, "问题", value_string(value, "prompt"));
    push_detail_field(&mut lines, "摘要", value_string(value, "summary"));
    push_detail_field(&mut lines, "snapshotId", value_string(value, "snapshotId"));
    push_detail_field(
        &mut lines,
        "continuationId",
        value_string(value, "continuationId"),
    );
    let menu = tui::ask::parser::ask_menu_from_metadata(0, &json!({ "ask": value }));
    if let Some(menu) = menu {
        for question in menu.questions {
            lines.push(format!("   Q: {}", question.prompt));
            for option in question.choices.iter().filter(|option| !option.is_other) {
                lines.push(format!("   - {}", option.label));
            }
            if question.choices.iter().any(|option| option.is_other) {
                lines.push("   - Other 自由输入".to_string());
            }
        }
    }
    lines.join("\n")
}

fn latest_context_fork_id(metadata: &Option<Value>) -> Option<String> {
    let metadata = metadata.as_ref()?;
    metadata
        .get("planning")
        .and_then(|planning| planning.get("contextForks"))
        .and_then(Value::as_array)
        .and_then(|forks| forks.last())
        .and_then(|fork| value_string(fork, "id"))
}

fn continuation_from_turn(turn: &Turn) -> Option<Value> {
    turn.pending_continuation
        .clone()
        .or_else(|| turn.metadata.as_ref().and_then(continuation_from_metadata))
}

fn ask_menu_from_turn(turn_index: usize, turn: &Turn) -> Option<(usize, AskMenu)> {
    let metadata = turn.metadata.as_ref()?;
    ask_menu_from_turn_metadata(turn_index, metadata)
}

fn latest_context_summary(turns: &[Turn], kind: ContextRowKind, fallback: &str) -> String {
    turns
        .iter()
        .rev()
        .flat_map(|turn| turn.context_rows.iter())
        .find(|row| row.kind == kind)
        .map(|row| format!("{}：{}", context_row_label(kind), row.summary))
        .unwrap_or_else(|| fallback.to_string())
}

fn fork_create_command_from_turn(
    turn: &Turn,
    active_context_fork_id: &Option<String>,
) -> Option<SocketCommand> {
    let request_id = format!("flyflor-cli-fork-{}", now_millis());
    let source = ForkCreateSource {
        user: turn.user.clone(),
        answer: turn.answer.clone(),
        source_event_id: turn.event_id.clone(),
        source_message_id: turn.message_id.clone(),
        source_ask_id: turn
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("ask"))
            .and_then(|ask| value_string(ask, "askId").or_else(|| value_string(ask, "snapshotId"))),
    };
    let payload = fork_create_payload(&source, active_context_fork_id.as_deref())?;
    Some(SocketCommand::ForkCreate {
        request_id,
        payload,
    })
}

fn value_string(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_string)
}

fn kind_or_default(kind: &str, default: &str) -> String {
    if kind.trim().is_empty() {
        default.to_string()
    } else {
        kind.to_string()
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn iso8601_from_millis(millis: u64) -> String {
    let seconds = (millis / 1000).min(i64::MAX as u64) as i64;
    let nanos = ((millis % 1000) * 1_000_000) as u32;
    let Ok(datetime) = OffsetDateTime::from_unix_timestamp(seconds)
        .and_then(|datetime| datetime.replace_nanosecond(nanos))
    else {
        return millis.to_string();
    };
    datetime
        .format(&Rfc3339)
        .unwrap_or_else(|_| millis.to_string())
}

struct Theme {
    bg: Color,
    text: Color,
    muted: Color,
    dim: Color,
    blue: Color,
    purple: Color,
    pink: Color,
    danger: Color,
    green: Color,
    overlay: Color,
    scroll_thumb: Color,
    scroll_track: Color,
    status_active_bg: Color,
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
    user_leading_bar: char,
    footer_icon: char,
    thread_gutter: usize,
    user_pad: usize,
    user_right_gap: usize,
    answer_left_pad: usize,
    answer_right_pad: usize,
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

#[derive(Clone)]
struct ContextRow {
    kind: ContextRowKind,
    summary: String,
    detail: String,
    expanded: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ContextRowKind {
    Recall,
    Thought,
    Fork,
    Blackboard,
    Execution,
    AskResume,
    CreateFork,
}

#[derive(Clone, Deserialize)]
struct Turn {
    #[serde(default)]
    message_id: Option<String>,
    #[serde(default)]
    event_id: Option<String>,
    user: String,
    #[serde(default)]
    thought: Option<ThoughtData>,
    answer: String,
    #[serde(default)]
    metadata: Option<Value>,
    #[serde(skip)]
    context_rows: Vec<ContextRow>,
    #[serde(skip)]
    pending_continuation: Option<Value>,
    #[serde(default)]
    footer: String,
}

#[derive(Clone, Deserialize)]
struct TodoItem {
    marker: String,
    label: String,
    status: String,
    active: bool,
    plan_id: Option<String>,
}

impl TodoItem {
    fn empty_plan() -> Self {
        Self {
            marker: "○".to_string(),
            label: "暂无计划".to_string(),
            status: "-".to_string(),
            active: false,
            plan_id: None,
        }
    }
}

#[derive(Clone, Deserialize)]
struct StatItem {
    label: String,
    value: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
struct ForkMemorySnapshot {
    #[serde(default)]
    forks: Vec<ForkMemoryItem>,
    #[serde(default, rename = "brainDbHuman")]
    brain_db_human: Option<String>,
    #[serde(default, rename = "brainDbStatus")]
    brain_db_status: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct ForkMemoryItem {
    summary: String,
    #[serde(default)]
    time: Option<String>,
}

#[derive(Clone, Deserialize)]
struct RightPanelData {
    thinking_label: String,
    blackboard_status: String,
    blackboard_stream: Vec<String>,
    model_stats: Vec<StatItem>,
    token_stats: Vec<StatItem>,
    context_total: String,
    context_percent: String,
    context_bar: String,
    context_usage: String,
    context_ratio: f64,
    context_used_tokens: usize,
    context_max_tokens: Option<usize>,
    context_used: String,
    context_max: String,
    #[serde(default)]
    fork_memory: ForkMemorySnapshot,
    #[serde(skip)]
    run_timeline: RunTimeline,
    footer: String,
}

impl RightPanelData {
    fn default_live() -> Self {
        Self {
            thinking_label: "Socket".to_string(),
            blackboard_status: "waiting for flyflor socket".to_string(),
            blackboard_stream: Vec::new(),
            model_stats: Vec::new(),
            token_stats: Vec::new(),
            context_total: "未收到上下文窗口".to_string(),
            context_percent: "未知".to_string(),
            context_bar: String::new(),
            context_usage: "暂无数据".to_string(),
            context_ratio: 0.0,
            context_used_tokens: 0,
            context_max_tokens: None,
            context_used: "0".to_string(),
            context_max: "未知".to_string(),
            fork_memory: ForkMemorySnapshot::default(),
            run_timeline: RunTimeline::new(),
            footer: "flyflor-cli".to_string(),
        }
    }
}

#[derive(Deserialize)]
struct SocketEnvelope {
    #[serde(rename = "type")]
    message_type: String,
    #[serde(default)]
    payload: Option<Value>,
}

#[derive(Deserialize)]
struct HistorySnapshotPayload {
    history: Vec<HistoryTurnSnapshot>,
}

#[derive(Deserialize)]
struct HistoryTurnSnapshot {
    #[serde(rename = "assistantText")]
    assistant_text: String,
    #[serde(rename = "eventId")]
    event_id: String,
    #[serde(default, rename = "contextForks")]
    context_forks: Option<Value>,
    #[serde(default, rename = "executiveToolExecutions")]
    executive_tool_executions: Option<Value>,
    #[serde(default)]
    metadata: Option<Value>,
    #[serde(default)]
    replays: Option<Value>,
    #[serde(default, rename = "taskPlans")]
    task_plans: Option<Value>,
    ts: u64,
    #[serde(rename = "userText")]
    user_text: String,
}

#[derive(Deserialize)]
struct TurnDeltaPayload {
    #[serde(rename = "messageId")]
    message_id: String,
    delta: String,
}

#[derive(Deserialize)]
struct TurnFinalPayload {
    reply: GatewayReplyLike,
}

#[derive(Deserialize)]
struct GatewayReplyLike {
    #[serde(rename = "messageId")]
    message_id: String,
    text: String,
    #[serde(default)]
    metadata: Option<Value>,
}

#[derive(Deserialize)]
struct TurnErrorPayload {
    #[serde(rename = "messageId")]
    message_id: String,
    message: String,
}

#[derive(Deserialize)]
struct MockData {
    turns: Vec<Turn>,
    todos: Vec<TodoItem>,
    right_panel: RightPanelData,
    #[serde(default)]
    fork_memory: ForkMemorySnapshot,
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
            danger: Color::Rgb(255, 72, 72),
            green: Color::Rgb(91, 228, 155),
            overlay: Color::Rgb(10, 14, 28),
            scroll_thumb: Color::Rgb(218, 220, 228),
            scroll_track: Color::Rgb(107, 116, 144),
            status_active_bg: Color::Rgb(42, 38, 84),
            user_bg: Color::Rgb(32, 33, 36),
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
            rail_char: '┃',
            thought_bar_char: '┃',
            user_leading_bar: '┃',
            footer_icon: '◻',
            thread_gutter: 1,
            user_pad: 1,
            user_right_gap: 3,
            answer_left_pad: 2,
            answer_right_pad: 4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        layout::shell::{app_layout, content_root},
        tui::ask::state::{AskChoice, AskQuestion},
    };

    fn separator_text(width: u16) -> String {
        "─".repeat(width as usize)
    }

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

    #[test]
    fn parses_history_snapshot_into_turns() {
        let raw = r#"{
            "protocol": "flyflor.ws.v1",
            "id": "env-history-snapshot-1",
            "type": "history.snapshot",
            "at": "2026-05-22T00:00:05.050Z",
            "payload": {
                "history": [
                    {
                        "eventId": "event-2",
                        "ts": 1770000001000,
                        "userText": "第二条",
                        "assistantText": "第二个回答"
                    },
                    {
                        "eventId": "event-1",
                        "ts": 1770000000000,
                        "userText": "第一条",
                        "assistantText": "第一个回答",
                        "metadata": {
                            "messageId": "message-history-1",
                            "planning": {
                                "contextForks": [{
                                    "id": "fork-history-1",
                                    "title": "历史 fork",
                                    "continuitySummary": "保留历史上下文",
                                    "maxContextTokens": 12000
                                }]
                            }
                        }
                    }
                ]
            }
        }"#;

        let turns = parse_history_snapshot(raw)
            .expect("snapshot should parse")
            .expect("history snapshot should produce turns");

        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].user, "第一条");
        assert_eq!(turns[0].answer, "第一个回答");
        assert_eq!(turns[1].user, "第二条");
        assert!(turns[0].footer.contains("event-1"));
        assert_eq!(turns[0].message_id.as_deref(), Some("message-history-1"));
        assert_eq!(
            turns[0]
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("planning"))
                .and_then(|planning| planning.get("contextForks"))
                .and_then(Value::as_array)
                .and_then(|forks| forks.first())
                .and_then(|fork| fork.get("id"))
                .and_then(Value::as_str),
            Some("fork-history-1")
        );
        assert!(
            turns[0]
                .context_rows
                .iter()
                .any(|row| row.kind == ContextRowKind::Fork && row.summary == "历史 fork")
        );
    }

    #[test]
    fn parses_history_top_level_context_fields_into_metadata() {
        let raw = r#"{
            "protocol": "flyflor.ws.v1",
            "id": "env-history-snapshot-1",
            "type": "history.snapshot",
            "at": "2026-05-22T00:00:05.050Z",
            "payload": {
                "history": [
                    {
                        "eventId": "event-1",
                        "ts": 1770000000000,
                        "userText": "第一条",
                        "assistantText": "第一个回答",
                        "contextForks": [{
                            "id": "fork-top-1",
                            "title": "顶层 fork",
                            "continuitySummary": "来自 history 顶层字段",
                            "maxContextTokens": 12000
                        }],
                        "replays": [{
                            "id": "replay-top-1",
                            "kind": "blackboard",
                            "title": "顶层 blackboard"
                        }]
                    }
                ]
            }
        }"#;

        let turns = parse_history_snapshot(raw)
            .expect("snapshot should parse")
            .expect("history snapshot should produce turns");

        let metadata = turns[0].metadata.as_ref().expect("metadata should merge");
        assert_eq!(
            metadata
                .get("planning")
                .and_then(|planning| planning.get("contextForks"))
                .and_then(Value::as_array)
                .and_then(|forks| forks.first())
                .and_then(|fork| fork.get("id"))
                .and_then(Value::as_str),
            Some("fork-top-1")
        );
        assert!(
            turns[0]
                .context_rows
                .iter()
                .any(|row| row.kind == ContextRowKind::Fork && row.summary == "顶层 fork")
        );
        assert!(
            turns[0]
                .context_rows
                .iter()
                .any(|row| row.kind == ContextRowKind::Blackboard
                    && row.summary == "顶层 blackboard")
        );
    }

    #[test]
    fn aggregates_repeated_context_rows_by_kind() {
        let metadata = Some(json!({
            "planning": {
                "contextForks": [
                    {
                        "id": "fork-1",
                        "title": "阅读 coding-worktree",
                        "summary": "读取项目代码"
                    },
                    {
                        "id": "fork-1",
                        "title": "阅读 coding-worktree 更新版",
                        "summary": "重复记录应该保留最新版本"
                    },
                    {
                        "id": "fork-2",
                        "title": "实现 TUI",
                        "summary": "继续 TUI 调试"
                    }
                ],
                "replays": [
                    {
                        "id": "replay-1",
                        "kind": "blackboard",
                        "title": "黑板记录"
                    },
                    {
                        "id": "replay-2",
                        "kind": "blackboard",
                        "title": "黑板记录 2"
                    },
                    {
                        "id": "replay-3",
                        "kind": "recall",
                        "title": "热记忆"
                    },
                    {
                        "id": "replay-4",
                        "kind": "recall",
                        "title": "热记忆 2"
                    }
                ]
            }
        }));

        let rows = context_rows_from_metadata(&metadata);
        assert_eq!(
            rows.iter()
                .filter(|row| row.kind == ContextRowKind::Fork)
                .count(),
            1
        );
        assert_eq!(
            rows.iter()
                .filter(|row| row.kind == ContextRowKind::Blackboard)
                .count(),
            1
        );
        assert_eq!(
            rows.iter()
                .filter(|row| row.kind == ContextRowKind::Recall)
                .count(),
            1
        );

        let fork = rows
            .iter()
            .find(|row| row.kind == ContextRowKind::Fork)
            .expect("fork row");
        assert_eq!(fork.summary, "2 条摘要 · 最近：实现 TUI");
        assert!(!fork.detail.trim_start().starts_with('['));
        assert!(fork.detail.contains("1. 阅读 coding-worktree 更新版"));
        assert!(fork.detail.contains("摘要: 重复记录应该保留最新版本"));
        assert!(fork.detail.contains("2. 实现 TUI"));
        assert!(!fork.detail.contains("id:"));
        assert!(!fork.detail.contains("来源:"));
        assert!(!fork.detail.contains("上下文上限:"));

        assert_eq!(
            rows.iter()
                .filter(|row| row.kind == ContextRowKind::CreateFork)
                .count(),
            1
        );
    }

    #[test]
    fn fork_context_detail_dedupes_repeated_summaries_and_hides_internal_fields() {
        let fork_value = |id: &str, max_context_tokens: usize| {
            json!({
                "sourceEventId": "event-read-worktree",
                "sourceKey": "read-worktree",
                "title": "阅读 coding-worktree",
                "inputSummary": "阅读 /Users/yihuaqing/Desktop/yihuaqing/flyflors/coding-worktree",
                "summary": "阅读 /Users/yihuaqing/Desktop/yihuaqing/flyflors/coding-worktree 的所有代码。",
                "continuitySummary": "阅读 /Users/yihuaqing/Desktop/yihuaqing/flyflors/coding-worktree 的所有代码。",
                "id": id,
                "maxContextTokens": max_context_tokens
            })
        };
        let metadata = Some(json!({
            "planning": {
                "contextForks": [
                    fork_value("fork-a", 12000),
                    fork_value("fork-b", 16000),
                    fork_value("fork-c", 18000),
                    fork_value("fork-d", 20000),
                    fork_value("fork-e", 24000),
                    fork_value("fork-f", 32000)
                ]
            }
        }));

        let rows = context_rows_from_metadata(&metadata);
        let fork = rows
            .iter()
            .find(|row| row.kind == ContextRowKind::Fork)
            .expect("fork row");

        assert_eq!(fork.summary, "阅读 coding-worktree");
        assert_eq!(fork.detail.matches("\n\n").count(), 1);
        assert!(fork.detail.contains("1. 阅读 coding-worktree"));
        assert!(fork.detail.contains("已合并 5 条重复 fork"));
        assert!(!fork.detail.contains("fork-a"));
        assert!(!fork.detail.contains("fork-b"));
        assert!(!fork.detail.contains("source"));
        assert!(!fork.detail.contains("上下文上限"));
        assert!(!fork.detail.contains("maxContextTokens"));
        assert_eq!(fork.detail.matches("延续").count(), 0);
    }

    #[test]
    fn ignores_non_history_socket_messages() {
        let raw = r#"{
            "protocol": "flyflor.ws.v1",
            "id": "env-ack-1",
            "type": "ack",
            "at": "2026-05-22T00:00:01.050Z",
            "payload": { "received": "client.hello" }
        }"#;

        assert!(
            parse_history_snapshot(raw)
                .expect("ack should parse")
                .is_none()
        );
    }

    #[test]
    fn parses_turn_delta_event() {
        let raw = r#"{
            "protocol": "flyflor.ws.v1",
            "id": "env-turn-delta-1",
            "type": "turn.delta",
            "at": "2026-05-22T00:00:06.100Z",
            "payload": {
                "messageId": "message-1",
                "delta": "收到"
            }
        }"#;

        let event = parse_turn_event(raw)
            .expect("delta should parse")
            .expect("delta should emit event");

        match event {
            SocketEvent::TurnDelta { message_id, delta } => {
                assert_eq!(message_id, "message-1");
                assert_eq!(delta, "收到");
            }
            _ => panic!("expected turn delta event"),
        }
    }

    #[test]
    fn parses_turn_final_event() {
        let raw = r#"{
            "protocol": "flyflor.ws.v1",
            "id": "env-turn-final-1",
            "type": "turn.final",
            "at": "2026-05-22T00:00:06.500Z",
            "payload": {
                "reply": {
                    "messageId": "message-1",
                    "route": {
                        "channel": "ws",
                        "conversationKey": "routing-lane-1",
                        "chatType": "direct"
                    },
                    "text": "收到，我会继续推进。"
                }
            }
        }"#;

        let event = parse_turn_event(raw)
            .expect("final should parse")
            .expect("final should emit event");

        match event {
            SocketEvent::TurnFinal {
                message_id,
                text,
                metadata,
            } => {
                assert_eq!(message_id, "message-1");
                assert_eq!(text, "收到，我会继续推进。");
                assert!(metadata.is_none());
            }
            _ => panic!("expected turn final event"),
        }
    }

    #[test]
    fn parses_turn_final_metadata_into_context_rows() {
        let raw = r#"{
            "protocol": "flyflor.ws.v1",
            "id": "env-turn-final-1",
            "type": "turn.final",
            "at": "2026-05-22T00:00:06.500Z",
            "payload": {
                "reply": {
                    "messageId": "message-1",
                    "route": {
                        "channel": "ws",
                        "conversationKey": "routing-lane-1",
                        "chatType": "direct"
                    },
                    "text": "我需要确认一个边界。",
                    "metadata": {
                        "kind": "ask",
                        "messageId": "message-1",
                        "ask": {
                            "prompt": "这轮是否继续？",
                            "reason": "clarification",
                            "snapshotId": "ask-snapshot-1"
                        },
                        "planning": {
                            "contextForks": [{
                                "id": "fork-1",
                                "title": "Implementation fork",
                                "continuitySummary": "Keep socket docs and tests in view.",
                                "maxContextTokens": 12000
                            }],
                            "replays": [{
                                "id": "replay-1",
                                "kind": "blackboard",
                                "title": "Blackboard replay",
                                "summary": "Replay current blackboard."
                            }]
                        }
                    }
                }
            }
        }"#;

        let event = parse_turn_event(raw)
            .expect("final should parse")
            .expect("final should emit event");

        match event {
            SocketEvent::TurnFinal { metadata, .. } => {
                let rows = context_rows_from_metadata(&metadata);
                assert!(rows.iter().any(|row| {
                    row.kind == ContextRowKind::AskResume && row.summary == "这轮是否继续？"
                }));
                assert!(rows.iter().any(|row| {
                    row.kind == ContextRowKind::Fork && row.summary == "Implementation fork"
                }));
                assert!(rows.iter().any(|row| {
                    row.kind == ContextRowKind::Blackboard && row.summary == "Blackboard replay"
                }));
                assert_eq!(latest_context_fork_id(&metadata).as_deref(), Some("fork-1"));
            }
            _ => panic!("expected turn final event"),
        }
    }

    #[test]
    fn turn_final_ask_metadata_opens_menu_and_compacts_transcript_text() {
        let mut app = App::new();
        let message_id = "message-ask-1".to_string();
        app.pending_turns.insert(message_id.clone(), 0);
        app.turns.push(Turn {
            message_id: Some(message_id.clone()),
            event_id: None,
            user: "read project".to_string(),
            thought: None,
            answer: String::new(),
            metadata: None,
            context_rows: Vec::new(),
            pending_continuation: None,
            footer: String::new(),
        });

        app.apply_socket_event(SocketEvent::TurnFinal {
            message_id,
            text: "执行层连续遇到工具阻断。\n\n1. 继续执行\n2. 缩小范围".to_string(),
            metadata: Some(json!({
                "kind": "ask",
                "ask": {
                    "prompt": "执行层连续遇到工具阻断。请补充下一步执行策略或调整约束后再继续。",
                    "snapshotId": "ask-snapshot-1",
                    "questions": [{
                        "id": "strategy",
                        "prompt": "下一步执行策略是什么？",
                        "recommendedChoiceId": "continue",
                        "choices": [
                            { "id": "continue", "label": "继续执行", "value": "continue" },
                            { "id": "narrow", "label": "缩小范围", "value": "narrow" }
                        ]
                    }]
                }
            })),
        });

        let turn = app.turns.first().expect("turn");
        assert_eq!(
            turn.answer,
            "执行层连续遇到工具阻断。请补充下一步执行策略或调整约束后再继续。"
        );
        assert!(!turn.answer.contains("1. 继续执行"));
        let menu = app.ask_menu.as_ref().expect("ASK menu should open");
        assert_eq!(menu.questions[0].id, "strategy");
        assert_eq!(menu.questions[0].choices[0].id, "continue");
        assert!(menu.questions[0].choices[0].recommended);
    }

    #[test]
    fn turn_final_message_id_mismatch_still_opens_structured_ask_menu() {
        let mut app = App::new();
        app.socket_connected = true;
        app.pending_turns.insert("client-message".to_string(), 0);
        app.turns.push(Turn {
            message_id: Some("client-message".to_string()),
            event_id: None,
            user: "read project".to_string(),
            thought: None,
            answer: String::new(),
            metadata: None,
            context_rows: Vec::new(),
            pending_continuation: None,
            footer: String::new(),
        });

        app.apply_socket_event(SocketEvent::TurnFinal {
            message_id: "runtime-message".to_string(),
            text: "fallback".to_string(),
            metadata: Some(json!({
                "kind": "ask",
                "behaviorSnapshotId": "behavior-ask-1",
                "ask": {
                    "prompt": "需要选择策略",
                    "questions": [{
                        "id": "strategy",
                        "prompt": "下一步？",
                        "recommendedChoiceId": "continue",
                        "choices": [
                            { "id": "continue", "label": "继续执行", "value": "continue" },
                            { "id": "stop", "label": "停止", "value": "stop" }
                        ]
                    }]
                }
            })),
        });

        let menu = app.ask_menu.as_ref().expect("ASK menu should open");
        assert_eq!(
            menu.continuation.get("snapshotId").and_then(Value::as_str),
            Some("behavior-ask-1")
        );
        assert_eq!(menu.selected_by_question[0], 0);
        assert!(menu.questions[0].choices[0].recommended);
    }

    #[test]
    fn turn_final_blackboard_metadata_formats_summary_and_discussion() {
        let raw = r#"{
            "protocol": "flyflor.ws.v1",
            "id": "env-turn-final-blackboard",
            "type": "turn.final",
            "payload": {
                "reply": {
                    "messageId": "message-blackboard-1",
                    "text": "回答正文不包含黑板内容",
                    "metadata": {
                        "blackboard": {
                            "summary": "确认右侧布局修复方案",
                            "status": "closed",
                            "mode": "plan",
                            "turnId": "bb-turn-1",
                            "content": [
                                { "role": "planner", "content": "先固定 TODO 标题。" },
                                { "role": "executor", "content": "再处理 fork 单行展示。" }
                            ]
                        }
                    }
                }
            }
        }"#;

        let event = parse_turn_event(raw)
            .expect("final should parse")
            .expect("final should emit event");

        match event {
            SocketEvent::TurnFinal { text, metadata, .. } => {
                assert_eq!(text, "回答正文不包含黑板内容");
                let rows = context_rows_from_metadata(&metadata);
                let blackboard = rows
                    .iter()
                    .find(|row| row.kind == ContextRowKind::Blackboard)
                    .expect("blackboard row");

                assert_eq!(blackboard.summary, "确认右侧布局修复方案");
                let header = line_text(&render_context_row_header(
                    blackboard,
                    96,
                    &Theme::default(),
                ));
                assert!(header.contains("▼ 🤔 黑板讨论 确认右侧布局修复方案"));
                assert!(blackboard.detail.contains("Blackboard discussion"));
                assert!(blackboard.detail.contains("Status: closed"));
                assert!(blackboard.detail.contains("reason: 确认右侧布局修复方案"));
                assert!(blackboard.detail.contains("plan: plan"));
                assert!(
                    blackboard
                        .detail
                        .contains("Round 1 · planner: 先固定 TODO 标题。")
                );
                assert!(
                    blackboard
                        .detail
                        .contains("Round 2 · executor: 再处理 fork 单行展示。")
                );
                assert!(!blackboard.detail.contains("\"summary\""));
                assert!(!blackboard.detail.contains("\"turnId\""));
            }
            _ => panic!("expected turn final event"),
        }
    }

    #[test]
    fn blackboard_discussion_formats_string_object_and_array_content() {
        let string_discussion = format_blackboard_discussion(&json!({
            "summary": "同步状态",
            "status": "open",
            "mode": "review",
            "content": "单条黑板记录"
        }));
        assert!(
            string_discussion
                .contains("Blackboard discussion: Status: open; reason: 同步状态; plan: review")
        );
        assert!(string_discussion.contains("Round 1: 单条黑板记录"));

        let object_discussion = format_blackboard_discussion(&json!({
            "summary": "对象内容",
            "content": {
                "rounds": [
                    { "speaker": "agent", "text": "对象里的第一轮" },
                    { "speaker": "user", "message": "对象里的第二轮" }
                ]
            }
        }));
        assert!(object_discussion.contains("Round 1 · agent: 对象里的第一轮"));
        assert!(object_discussion.contains("Round 2 · user: 对象里的第二轮"));
        assert!(!object_discussion.contains("\"rounds\""));
    }

    #[test]
    fn context_row_labels_use_new_copy() {
        let theme = Theme::default();
        let rows = vec![
            ContextRow {
                kind: ContextRowKind::Thought,
                summary: "摘要".to_string(),
                detail: String::new(),
                expanded: true,
            },
            ContextRow {
                kind: ContextRowKind::Recall,
                summary: "摘要".to_string(),
                detail: String::new(),
                expanded: true,
            },
            ContextRow {
                kind: ContextRowKind::Fork,
                summary: "摘要".to_string(),
                detail: String::new(),
                expanded: true,
            },
            ContextRow {
                kind: ContextRowKind::Blackboard,
                summary: "摘要".to_string(),
                detail: String::new(),
                expanded: true,
            },
            ContextRow {
                kind: ContextRowKind::AskResume,
                summary: "摘要".to_string(),
                detail: String::new(),
                expanded: false,
            },
            ContextRow {
                kind: ContextRowKind::CreateFork,
                summary: "摘要".to_string(),
                detail: String::new(),
                expanded: false,
            },
        ];
        let text = rows
            .iter()
            .map(|row| line_text(&render_context_row_header(row, 96, &theme)))
            .collect::<Vec<_>>()
            .join("\n");

        assert!(text.contains("▼ 😌 思考中 摘要"));
        assert!(text.contains("▼ ☁️ 回忆中 摘要"));
        assert!(text.contains("▼ 🍀 fork 摘要"));
        assert!(text.contains("▼ 🤔 黑板讨论 摘要"));
        assert!(text.contains("◎ 重新回答 摘要"));
        assert!(text.contains("⊕ 新建 fork 摘要"));
    }

    #[test]
    fn recall_and_thought_expand_as_markdown_and_copy() {
        let theme = Theme::default();
        let mut turn = test_turn("u", "answer");
        turn.metadata = Some(json!({
            "recall": {
                "summary": "装配 scope 记忆",
                "scopeId": "scope-1",
                "items": [{ "summary": "项目记忆", "content": "recall-copy-detail" }]
            },
            "thought": {
                "summary": "直接观察",
                "status": "running",
                "content": "thought-copy-detail"
            }
        }));
        turn.context_rows = context_rows_from_metadata(&turn.metadata);
        for row in &mut turn.context_rows {
            if matches!(row.kind, ContextRowKind::Recall | ContextRowKind::Thought) {
                row.expanded = true;
            }
        }
        let mut app = App::new();
        app.turns = vec![turn];
        app.update_left_viewport(Rect::new(0, 0, 96, 30), &theme);
        let rendered = app
            .chat_lines
            .iter()
            .map(line_plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("scope 记忆装配"));
        assert!(rendered.contains("recall-copy-detail"));
        assert!(rendered.contains("LLM 思考详情"));
        assert!(rendered.contains("thought-copy-detail"));

        app.selection.anchor = Some(SelectionPoint {
            target: SelectionTarget::Left,
            line_index: 0,
            column: 0,
        });
        app.selection.head = Some(SelectionPoint {
            target: SelectionTarget::Left,
            line_index: app.chat_lines.len().saturating_sub(1),
            column: 200,
        });
        let copied = app.selection_to_text().expect("selection text");
        assert!(copied.contains("recall-copy-detail"));
        assert!(copied.contains("thought-copy-detail"));
    }

    #[test]
    fn thought_and_recall_snapshots_update_context_rows() {
        let thought_raw = r#"{
            "type": "thought.snapshot",
            "payload": {
                "data": {
                    "thought": {
                        "id": "thought-1",
                        "summary": "direct watch",
                        "status": "running",
                        "content": "思考详情"
                    }
                }
            }
        }"#;
        let recall_raw = r#"{
            "type": "recall.snapshot",
            "payload": {
                "data": {
                    "recall": {
                        "id": "recall-1",
                        "summary": "scope recall",
                        "items": [{ "summary": "记忆 A", "content": "召回详情" }]
                    }
                }
            }
        }"#;

        let thought = parse_context_snapshot(thought_raw)
            .expect("thought parses")
            .expect("thought event");
        let recall = parse_context_snapshot(recall_raw)
            .expect("recall parses")
            .expect("recall event");
        let mut app = App::new();
        app.apply_socket_event(thought);
        app.apply_socket_event(recall);

        assert!(
            app.turns
                .iter()
                .flat_map(|turn| turn.context_rows.iter())
                .any(|row| row.kind == ContextRowKind::Thought && row.summary == "direct watch")
        );
        assert!(
            app.turns
                .iter()
                .flat_map(|turn| turn.context_rows.iter())
                .any(|row| row.kind == ContextRowKind::Recall && row.summary == "scope recall")
        );
    }

    #[test]
    fn reanswer_row_sends_structured_continuation() {
        let mut app = App::new();
        let mut turn = test_turn("原问题", "需要重答");
        turn.metadata = Some(json!({
            "continuation": {
                "mode": "continue",
                "snapshotId": "ask-ghost-1"
            }
        }));
        turn.context_rows = context_rows_from_metadata(&turn.metadata);
        let row_index = turn
            .context_rows
            .iter()
            .position(|row| row.kind == ContextRowKind::AskResume)
            .expect("ask resume row");
        app.turns = vec![turn];
        app.context_row_hitboxes = vec![ContextRowHitbox {
            turn_index: 0,
            row_index,
            rect: Rect::new(0, 0, 20, 1),
        }];

        assert!(app.toggle_context_row_at(1, 0));
        let sent = app.turns.last().expect("sent reanswer turn");
        assert_eq!(sent.user, "原问题");
        assert_eq!(
            sent.metadata
                .as_ref()
                .and_then(|metadata| metadata.get("continuation"))
                .and_then(|continuation| continuation.get("snapshotId"))
                .and_then(Value::as_str),
            Some("ask-ghost-1")
        );
    }

    #[test]
    fn running_context_row_uses_shimmer_style() {
        let theme = Theme::default();
        let row = ContextRow {
            kind: ContextRowKind::Thought,
            summary: "运行中".to_string(),
            detail: "状态：running".to_string(),
            expanded: false,
        };
        let line_a = render_context_row_header_with_phase(&row, 96, &theme, 0);
        let line_b = render_context_row_header_with_phase(&row, 96, &theme, 1);
        let colors_a = line_a
            .spans
            .iter()
            .filter_map(|span| span.style.fg)
            .collect::<Vec<_>>();
        let colors_b = line_b
            .spans
            .iter()
            .filter_map(|span| span.style.fg)
            .collect::<Vec<_>>();
        assert_ne!(colors_a, colors_b);
    }

    #[test]
    fn execution_context_row_uses_rotating_square_marker() {
        let theme = Theme::default();
        let row = ContextRow {
            kind: ContextRowKind::Execution,
            summary: "子代理 1/2 · 运行中".to_string(),
            detail: "状态：running".to_string(),
            expanded: false,
        };
        let line_a = line_text(&render_context_row_header_with_phase(&row, 96, &theme, 0));
        let line_b = line_text(&render_context_row_header_with_phase(&row, 96, &theme, 1));

        assert!(line_a.contains("◰"));
        assert!(line_b.contains("◳"));
        assert!(!line_a.contains("▶"));
    }

    #[test]
    fn non_execution_context_rows_keep_triangle_marker() {
        let theme = Theme::default();
        let row = ContextRow {
            kind: ContextRowKind::Blackboard,
            summary: "讨论".to_string(),
            detail: String::new(),
            expanded: false,
        };
        let line = line_text(&render_context_row_header_with_phase(&row, 96, &theme, 2));

        assert!(line.contains("▶"));
    }

    #[test]
    fn expanded_blackboard_discussion_is_available_to_selection_copy() {
        let theme = Theme::default();
        let mut turn = Turn {
            message_id: Some("message-1".to_string()),
            event_id: Some("event-1".to_string()),
            user: "u".to_string(),
            thought: None,
            answer: "assistant answer".to_string(),
            metadata: Some(json!({
                "blackboard": {
                    "summary": "复制黑板讨论",
                    "status": "done",
                    "mode": "act",
                    "content": [{ "role": "worker", "content": "可复制的展开内容" }]
                }
            })),
            context_rows: Vec::new(),
            pending_continuation: None,
            footer: String::new(),
        };
        turn.context_rows = context_rows_from_metadata(&turn.metadata);
        turn.context_rows
            .iter_mut()
            .find(|row| row.kind == ContextRowKind::Blackboard)
            .expect("blackboard row")
            .expanded = true;
        let mut app = App::new();
        app.turns.push(turn);
        app.update_left_viewport(Rect::new(0, 0, 96, 20), &theme);
        let start = app
            .chat_lines
            .iter()
            .position(|line| line_plain_text(line).contains("Blackboard discussion"))
            .expect("discussion start");
        let end = app
            .chat_lines
            .iter()
            .position(|line| line_plain_text(line).contains("可复制的展开内容"))
            .expect("discussion detail");
        let rendered_discussion = app
            .chat_lines
            .iter()
            .map(line_plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered_discussion.contains("Round 1 · worker"));
        assert!(rendered_discussion.contains("可复制的展开内容"));
        app.selection.anchor = Some(SelectionPoint {
            target: SelectionTarget::Left,
            line_index: start,
            column: 0,
        });
        app.selection.head = Some(SelectionPoint {
            target: SelectionTarget::Left,
            line_index: end,
            column: 80,
        });

        let copied = app.selection_to_text().expect("selection text");
        assert!(copied.contains("Blackboard discussion"));
        assert!(copied.contains("Round 1 · worker"));
        assert!(copied.contains("可复制的展开内容"));
    }

    #[test]
    fn context_theme_rows_render_human_labels() {
        let theme = Theme::default();
        let turn = Turn {
            message_id: Some("message-1".to_string()),
            event_id: Some("event-1".to_string()),
            user: "u".to_string(),
            thought: None,
            answer: "a".to_string(),
            metadata: Some(json!({
                "ask": {
                    "prompt": "请选择",
                    "snapshotId": "ask-snapshot-1",
                    "options": ["继续"]
                },
                "planning": {
                    "contextForks": [{ "id": "fork-1", "title": "fork 摘要" }],
                    "replays": [
                        { "id": "recall-1", "kind": "recall", "title": "回忆中 摘要" },
                        { "id": "blackboard-1", "kind": "blackboard", "title": "blackboard 摘要" }
                    ]
                }
            })),
            context_rows: Vec::new(),
            pending_continuation: None,
            footer: String::new(),
        };
        let mut turn = turn;
        turn.context_rows = context_rows_from_metadata(&turn.metadata);
        let text = turn
            .context_rows
            .iter()
            .map(|row| line_text(&render_context_row_header(row, 96, &theme)))
            .collect::<Vec<_>>()
            .join("\n");

        assert!(text.contains("回忆中"));
        assert!(text.contains("fork 摘要"));
        assert!(text.contains("blackboard 摘要"));
        assert!(text.contains("重新回答"));
        assert!(text.contains("新建 fork"));
    }

    #[test]
    fn ask_resume_detail_is_structured_text_not_json() {
        let rows = context_rows_from_metadata(&Some(json!({
            "ask": {
                "prompt": "请选择下一步",
                "snapshotId": "ask-snapshot-1",
                "options": ["继续", { "label": "停止", "value": "stop" }]
            }
        })));
        let row = rows
            .iter()
            .find(|row| row.kind == ContextRowKind::AskResume)
            .expect("ask row");

        assert!(row.detail.contains("ASK 续答上下文"));
        assert!(row.detail.contains("问题: 请选择下一步"));
        assert!(row.detail.contains("- Other 自由输入"));
        assert!(!row.detail.trim_start().starts_with('{'));
    }

    #[test]
    fn slash_command_palette_filters_and_executes_help() {
        let mut app = App::new();
        app.input = "/he".to_string();
        app.refresh_command_palette();

        assert_eq!(
            app.command_palette
                .as_ref()
                .and_then(|menu| menu.items.first())
                .map(|command| command.name),
            Some("help")
        );

        app.confirm_command_palette_selection();
        assert!(app.input.is_empty());
        assert!(app.right_source.blackboard_status.contains("/help"));
        assert!(app.right_source.blackboard_status.contains("/yolo"));
        assert!(app.right_source.blackboard_status.contains("危险"));
    }

    #[test]
    fn slash_command_yolo_toggles_and_exits_high_privilege_mode() {
        let mut app = App::new();
        assert!(!app.yolo_mode);
        assert_eq!(app.interaction_mode, InteractionMode::Act);

        app.input = "/yolo".to_string();
        app.refresh_command_palette();
        app.confirm_command_palette_selection();

        assert!(app.yolo_mode);
        assert_eq!(app.interaction_mode, InteractionMode::Yolo);
        assert!(app.right_source.blackboard_status.contains("YOLO 已开启"));
        assert!(app.right_source.blackboard_status.contains("高权限"));

        app.input = "/yolo".to_string();
        app.refresh_command_palette();
        app.confirm_command_palette_selection();

        assert!(!app.yolo_mode);
        assert_eq!(app.interaction_mode, InteractionMode::Act);
        assert!(app.right_source.blackboard_status.contains("YOLO 已关闭"));
    }

    #[test]
    fn slash_y_filters_yolo_instead_of_copying() {
        let mut app = App::new();
        app.input = "/".to_string();
        app.refresh_command_palette();

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
        );

        assert_eq!(app.input, "/y");
        assert_eq!(
            app.command_palette
                .as_ref()
                .and_then(|menu| menu.items.first())
                .map(|command| command.name),
            Some("yolo")
        );
    }

    #[test]
    fn ask_menu_adds_other_and_selection_sends_structured_continuation() {
        let mut app = App::new();
        app.turns.push(Turn {
            message_id: Some("message-1".to_string()),
            event_id: Some("event-1".to_string()),
            user: "u".to_string(),
            thought: None,
            answer: "需要选择".to_string(),
            metadata: Some(json!({
                "ask": {
                    "prompt": "选择下一步",
                    "snapshotId": "ask-snapshot-1",
                    "options": [
                        { "label": "继续实现", "value": "继续实现" }
                    ]
                }
            })),
            context_rows: Vec::new(),
            pending_continuation: None,
            footer: String::new(),
        });

        assert!(app.open_latest_ask_menu());
        let menu = app.ask_menu.as_ref().expect("ask menu");
        assert_eq!(menu.questions[0].choices.len(), 2);
        assert!(menu.questions[0].choices.last().expect("other").is_other);

        app.confirm_ask_menu_selection();
        let sent = app.turns.last().expect("sent turn");
        assert_eq!(sent.user, "继续实现");
    }

    #[test]
    fn ask_menu_moves_with_arrows_and_sends_selected_choice() {
        let mut app = App::new();
        app.turns.push(Turn {
            message_id: Some("message-1".to_string()),
            event_id: Some("event-1".to_string()),
            user: "u".to_string(),
            thought: None,
            answer: "需要选择".to_string(),
            metadata: Some(json!({
                "ask": {
                    "prompt": "选择下一步",
                    "snapshotId": "ask-snapshot-1",
                    "choices": ["A", "B"]
                }
            })),
            context_rows: Vec::new(),
            pending_continuation: None,
            footer: String::new(),
        });

        assert!(app.open_latest_ask_menu());
        assert!(app.move_active_menu(1));
        assert_eq!(
            app.ask_menu
                .as_ref()
                .expect("ask menu")
                .selected_by_question[0],
            1
        );
        assert!(app.handle_menu_confirm_or_next(true));

        let sent = app.turns.last().expect("sent turn");
        assert_eq!(sent.user, "B");
    }

    #[test]
    fn ask_menu_multi_question_submits_answers_array() {
        let mut app = App::new();
        let (tx, rx) = mpsc::channel();
        app.socket_tx = tx;
        app.socket_connected = true;
        app.turns.push(Turn {
            message_id: Some("message-1".to_string()),
            event_id: Some("event-1".to_string()),
            user: "u".to_string(),
            thought: None,
            answer: "需要选择".to_string(),
            metadata: Some(json!({
                "ask": {
                    "prompt": "选择下一步",
                    "snapshotId": "ask-snapshot-1",
                    "questions": [
                        { "id": "q1", "prompt": "策略", "choices": [{ "id": "continue", "label": "继续", "value": "continue" }] },
                        { "id": "q2", "prompt": "预算", "choices": [{ "id": "raise", "label": "增加", "value": "raise" }] }
                    ]
                }
            })),
            context_rows: Vec::new(),
            pending_continuation: None,
            footer: String::new(),
        });

        assert!(app.open_latest_ask_menu());
        assert!(app.handle_menu_confirm_or_next(false));
        assert!(app.handle_menu_confirm_or_next(true));

        match rx.try_recv().expect("send message command") {
            SocketCommand::SendMessage {
                metadata: Some(metadata),
                ..
            } => {
                let answers = metadata
                    .get("askAnswer")
                    .and_then(|answer| answer.get("answers"))
                    .and_then(Value::as_array)
                    .expect("answers");
                assert_eq!(answers.len(), 2);
                assert_eq!(
                    answers[0].get("questionId").and_then(Value::as_str),
                    Some("q1")
                );
                assert_eq!(
                    answers[1].get("questionId").and_then(Value::as_str),
                    Some("q2")
                );
            }
            _ => panic!("expected ask send message with metadata"),
        }
    }

    #[test]
    fn ask_menu_other_enters_inline_input_on_confirm() {
        let mut app = App::new();
        app.turns.push(Turn {
            message_id: Some("message-1".to_string()),
            event_id: Some("event-1".to_string()),
            user: "u".to_string(),
            thought: None,
            answer: "需要选择".to_string(),
            metadata: Some(json!({
                "ask": {
                    "prompt": "选择下一步",
                    "continuationId": "cont-1",
                    "questions": [{
                        "id": "q1",
                        "prompt": "选择下一步",
                        "choices": ["A"]
                    }]
                }
            })),
            context_rows: Vec::new(),
            pending_continuation: None,
            footer: String::new(),
        });

        assert!(app.open_latest_ask_menu());
        app.ask_menu
            .as_mut()
            .expect("ask menu")
            .selected_by_question[0] = 1;
        app.confirm_ask_menu_selection();

        assert!(app.ask_menu.as_ref().expect("ask menu").is_editing_other());
        assert!(app.right_source.blackboard_status.contains("Other"));
    }

    #[test]
    fn ask_menu_number_selecting_other_enters_inline_input() {
        let mut app = App::new();
        app.turns.push(Turn {
            message_id: Some("message-1".to_string()),
            event_id: Some("event-1".to_string()),
            user: "u".to_string(),
            thought: None,
            answer: "需要选择".to_string(),
            metadata: Some(json!({
                "ask": {
                    "prompt": "选择下一步",
                    "continuationId": "cont-1",
                    "choices": ["A"]
                }
            })),
            context_rows: Vec::new(),
            pending_continuation: None,
            footer: String::new(),
        });

        assert!(app.open_latest_ask_menu());
        assert!(app.select_active_menu_number('2'));

        assert!(app.ask_menu.is_some());
        assert!(app.ask_menu.as_ref().expect("ask menu").is_editing_other());
        assert!(app.input.is_empty());
    }

    #[test]
    fn ask_menu_escape_from_other_inline_input_closes_menu() {
        let mut app = App::new();
        app.turns.push(Turn {
            message_id: Some("message-1".to_string()),
            event_id: Some("event-1".to_string()),
            user: "u".to_string(),
            thought: None,
            answer: "需要选择".to_string(),
            metadata: Some(json!({
                "ask": {
                    "prompt": "选择下一步",
                    "continuationId": "cont-1",
                    "choices": ["A"]
                }
            })),
            context_rows: Vec::new(),
            pending_continuation: None,
            footer: String::new(),
        });

        assert!(app.open_latest_ask_menu());
        assert!(app.select_active_menu_number('2'));
        app.input = "暂存输入".to_string();
        app.close_menus();

        assert!(app.ask_menu.is_none());
        assert!(app.input.is_empty());
    }

    #[test]
    fn ask_menu_other_free_input_submits_through_existing_reply_path() {
        let mut app = App::new();
        app.turns.push(Turn {
            message_id: Some("message-1".to_string()),
            event_id: Some("event-1".to_string()),
            user: "u".to_string(),
            thought: None,
            answer: "需要选择".to_string(),
            metadata: Some(json!({
                "ask": {
                    "prompt": "选择下一步",
                    "continuationId": "cont-1",
                    "options": ["A"]
                }
            })),
            context_rows: Vec::new(),
            pending_continuation: None,
            footer: String::new(),
        });

        assert!(app.open_latest_ask_menu());
        app.ask_menu
            .as_mut()
            .expect("ask menu")
            .selected_by_question[0] = 1;
        app.confirm_ask_menu_selection();
        app.input = "我的自定义回答".to_string();
        app.socket_connected = true;
        app.submit_input();

        let sent = app.turns.last().expect("sent turn");
        assert_eq!(sent.user, "我的自定义回答");
        assert!(app.ask_menu.is_none());
    }

    #[test]
    fn ask_menu_other_free_input_preserves_inbound_choice_id() {
        let mut app = App::new();
        let (tx, rx) = mpsc::channel();
        app.socket_tx = tx;
        app.socket_connected = true;
        app.turns.push(Turn {
            message_id: Some("message-1".to_string()),
            event_id: Some("event-1".to_string()),
            user: "u".to_string(),
            thought: None,
            answer: "需要选择".to_string(),
            metadata: Some(json!({
                "ask": {
                    "prompt": "选择下一步",
                    "continuationId": "cont-1",
                    "questions": [{
                        "id": "q1",
                        "prompt": "选择下一步",
                        "choices": [
                            { "id": "safe", "label": "稳妥", "value": "safe" },
                            { "id": "custom", "label": "自定义", "isOther": true }
                        ]
                    }]
                }
            })),
            context_rows: Vec::new(),
            pending_continuation: None,
            footer: String::new(),
        });

        assert!(app.open_latest_ask_menu());
        app.ask_menu
            .as_mut()
            .expect("ask menu")
            .selected_by_question[0] = 1;
        app.confirm_ask_menu_selection();
        app.input = "我的自定义回答".to_string();
        app.submit_input();

        let command = rx.try_recv().expect("send message command");
        match command {
            SocketCommand::SendMessage {
                metadata: Some(metadata),
                ..
            } => {
                let ask_answer = metadata.get("askAnswer").expect("askAnswer");
                let answers = ask_answer
                    .get("answers")
                    .and_then(Value::as_array)
                    .expect("answers");
                assert_eq!(answers.len(), 1);
                assert_eq!(
                    answers[0].get("questionId").and_then(Value::as_str),
                    Some("q1")
                );
                assert_eq!(
                    answers[0].get("choiceId").and_then(Value::as_str),
                    Some("custom")
                );
                assert_eq!(
                    answers[0].get("text").and_then(Value::as_str),
                    Some("我的自定义回答")
                );
                assert_eq!(
                    answers[0].get("isOther").and_then(Value::as_bool),
                    Some(true)
                );
            }
            _ => panic!("expected ask send message with metadata"),
        }
    }

    #[test]
    fn slash_command_real_enter_path_does_not_submit_while_ask_other_inline_is_active() {
        let mut app = App::new();
        app.socket_connected = true;
        app.ask_menu = Some(AskMenu::new(
            0,
            json!({ "mode": "continue", "continuationId": "cont-1" }),
            vec![AskQuestion {
                id: "q1".to_string(),
                prompt: "选择下一步".to_string(),
                recommended_choice_id: None,
                choices: vec![AskChoice {
                    id: "custom".to_string(),
                    label: "Other".to_string(),
                    value: None,
                    description: None,
                    question_id: Some("q1".to_string()),
                    recommended: false,
                    is_other: true,
                }],
            }],
        ));
        app.ask_menu
            .as_mut()
            .expect("ask menu")
            .start_current_other_input();
        app.input = "/todo".to_string();
        app.refresh_command_palette();

        handle_key(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(app.command_palette.is_none());
        assert!(app.pending_plan_action.is_none());
        assert!(app.ask_menu.is_none());
        assert_eq!(app.turns.last().expect("sent turn").user, "/todo");
    }

    #[test]
    fn pasted_newlines_insert_without_submitting_until_real_enter() {
        let mut app = App::new();
        app.socket_connected = true;
        let initial_turns = app.turns.len();

        app.insert_paste_text("第一行\r\n第二行");

        assert_eq!(app.input, "第一行\n第二行");
        assert_eq!(app.turns.len(), initial_turns);
        assert!(app.socket_rx.try_recv().is_err());

        handle_key(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(app.turns.len(), initial_turns + 1);
        assert_eq!(app.turns.last().expect("sent turn").user, "第一行\n第二行");
        assert!(app.input.is_empty());
    }

    #[test]
    fn submit_without_socket_keeps_draft_editable_and_does_not_create_sending_turn() {
        let mut app = App::new();
        app.socket_connected = false;
        let initial_turns = app.turns.len();
        app.input = "hello gateway".to_string();

        app.submit_input();

        assert_eq!(app.turns.len(), initial_turns);
        assert_eq!(app.input, "hello gateway");
        assert!(app.pending_turns.is_empty());
        assert!(matches!(app.history_status, HistoryStatus::Error));
        assert!(
            app.right_source
                .blackboard_status
                .contains("socket unavailable")
        );
    }

    #[test]
    fn disconnect_marks_pending_turns_as_send_error() {
        let mut app = App::new();
        app.socket_connected = true;
        let turn_index = app.turns.len();
        app.turns.push(Turn {
            message_id: Some("message-1".to_string()),
            event_id: None,
            user: "hello".to_string(),
            thought: None,
            answer: String::new(),
            metadata: None,
            context_rows: Vec::new(),
            pending_continuation: None,
            footer: "flyflor · sending".to_string(),
        });
        app.pending_turns
            .insert("message-1".to_string(), turn_index);

        app.apply_socket_event(SocketEvent::Disconnected("connection refused".to_string()));

        assert!(app.pending_turns.is_empty());
        let turn = &app.turns[turn_index];
        assert!(turn.answer.contains("connection refused"));
        assert_eq!(turn.footer, "flyflor · send error");
        assert!(matches!(app.history_status, HistoryStatus::Error));
    }

    #[test]
    fn arrow_keys_move_input_cursor_without_right_panel_focus() {
        let mut app = App::new();
        app.input = "ab\ncd".to_string();
        app.input_cursor = Some(app.input.len());

        handle_key(&mut app, KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        app.insert_input_char('X');
        assert_eq!(app.input, "ab\ncXd");

        handle_key(&mut app, KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        app.insert_input_char('Y');
        assert_eq!(app.input, "abY\ncXd");
    }

    #[test]
    fn ask_menu_renders_above_composer_with_choices_and_other() {
        let theme = Theme::default();
        let menu = AskMenu::new(
            0,
            json!({ "mode": "continue", "snapshotId": "ask-1" }),
            vec![AskQuestion {
                id: "q1".to_string(),
                prompt: "选择下一步".to_string(),
                recommended_choice_id: None,
                choices: vec![
                    AskChoice {
                        id: "continue".to_string(),
                        label: "继续实现".to_string(),
                        value: Some("继续实现".to_string()),
                        description: None,
                        question_id: None,
                        recommended: false,
                        is_other: false,
                    },
                    AskChoice {
                        id: "other".to_string(),
                        label: "Other 自由输入".to_string(),
                        value: None,
                        description: Some("自由输入".to_string()),
                        question_id: None,
                        recommended: false,
                        is_other: true,
                    },
                ],
            }],
        );
        let lines = render_ask_menu_lines(&menu, &theme);
        let area = composer_menu_area(Rect::new(4, 20, 80, 4), lines.len()).expect("menu area");
        let text = lines
            .iter()
            .map(line_plain_text)
            .collect::<Vec<_>>()
            .join("\n");

        assert_eq!(area, Rect::new(4, 16, 80, 4));
        assert!(text.contains("继续实现"));
        assert!(text.contains("Other 自由输入"));
        assert!(!text.contains('{'));
    }

    #[test]
    fn ask_menu_renders_inline_other_text_in_current_choice() {
        let theme = Theme::default();
        let mut menu = AskMenu::new(
            0,
            json!({ "mode": "continue", "snapshotId": "ask-1" }),
            vec![AskQuestion {
                id: "q1".to_string(),
                prompt: "选择下一步".to_string(),
                recommended_choice_id: None,
                choices: vec![AskChoice {
                    id: "other".to_string(),
                    label: "Other".to_string(),
                    value: None,
                    description: None,
                    question_id: Some("q1".to_string()),
                    recommended: false,
                    is_other: true,
                }],
            }],
        );
        menu.start_current_other_input();
        menu.set_current_freeform("自定义策略".to_string());

        let text = render_ask_menu_lines(&menu, &theme)
            .iter()
            .map(line_plain_text)
            .collect::<Vec<_>>()
            .join("\n");

        assert!(text.contains("› 1. Other: 自定义策略"));
    }

    #[test]
    fn execution_context_row_renders_below_assistant_answer() {
        let theme = Theme::default();
        let mut turn = test_turn("任务", "智能体回答正文");
        turn.context_rows.push(ContextRow {
            kind: ContextRowKind::Execution,
            summary: "进行中 · 模型 1 · 子代理 1/2 · 工具 3 · 子进程 1".to_string(),
            detail: "tool read completed".to_string(),
            expanded: false,
        });

        let rendered = render_turns(&[turn], 96, &theme, 0)
            .lines
            .into_iter()
            .map(|line| line_plain_text(&line))
            .collect::<Vec<_>>();
        let answer_index = rendered
            .iter()
            .position(|line| line.contains("智能体回答正文"))
            .expect("answer should render");
        let exo_index = rendered
            .iter()
            .position(|line| line.contains("Exo") && line.contains("子代理"))
            .expect("execution row should render");

        assert!(exo_index > answer_index);
    }

    #[test]
    fn execution_context_renders_one_expandable_row_per_subagent() {
        let mut app = App::new();
        app.turns.push(test_turn("任务", "智能体回答正文"));
        app.apply_socket_event(SocketEvent::ExecutionJobSnapshot(json!({
            "data": {
                "children": [
                    {
                        "id": "child-a",
                        "name": "文件系统代理",
                        "status": "running",
                        "task": "扫描目录",
                        "toolCalls": [{
                            "id": "tool-a",
                            "tool": "Glob",
                            "status": "completed",
                            "argsPreview": "**/*"
                        }]
                    },
                    {
                        "id": "child-b",
                        "name": "代码分析代理",
                        "status": "pending",
                        "task": "分析代码"
                    }
                ]
            }
        })));

        let turn = app.turns.last().expect("turn");
        let rows = turn
            .context_rows
            .iter()
            .filter(|row| row.kind == ContextRowKind::Execution)
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].summary.contains("(1/2) 文件系统代理"));
        assert!(rows[0].detail.contains("子代理ID: child-a"));
        assert!(rows[0].detail.contains("tool Glob"));
        assert!(rows[1].summary.contains("(2/2) 代码分析代理"));
    }

    #[test]
    fn execution_context_preserves_expanded_subagent_by_child_id() {
        let mut app = App::new();
        app.turns.push(test_turn("任务", "智能体回答正文"));
        app.apply_socket_event(SocketEvent::ExecutionJobSnapshot(json!({
            "data": {
                "children": [
                    { "id": "child-a", "name": "文件系统代理", "status": "running" },
                    { "id": "child-b", "name": "代码分析代理", "status": "pending" }
                ]
            }
        })));
        let turn = app.turns.last_mut().expect("turn");
        let first = turn
            .context_rows
            .iter_mut()
            .find(|row| row.detail.contains("子代理ID: child-a"))
            .expect("first execution row");
        first.expanded = true;

        app.apply_socket_event(SocketEvent::RuntimeEvent(json!({
            "type": "subagent.child.end",
            "payload": {
                "childId": "child-a",
                "status": "completed",
                "task": "扫描目录完成"
            }
        })));

        let turn = app.turns.last().expect("turn");
        let first = turn
            .context_rows
            .iter()
            .find(|row| row.detail.contains("子代理ID: child-a"))
            .expect("first execution row");
        let second = turn
            .context_rows
            .iter()
            .find(|row| row.detail.contains("子代理ID: child-b"))
            .expect("second execution row");
        assert!(first.expanded);
        assert!(!second.expanded);
    }

    #[test]
    fn parses_context_snapshots_into_theme_rows() {
        let thought_raw = r#"{
            "protocol": "flyflor.ws.v1",
            "id": "env-thought-1",
            "type": "thought.snapshot",
            "payload": {
                "data": {
                    "thought": {
                        "id": "thought-1",
                        "title": "回忆中 摘要",
                        "summary": "读取历史"
                    }
                }
            }
        }"#;
        let blackboard_raw = r#"{
            "protocol": "flyflor.ws.v1",
            "id": "env-blackboard-1",
            "type": "blackboard.snapshot",
            "payload": {
                "data": {
                    "blackboard": {
                        "id": "blackboard-1",
                        "title": "blackboard 摘要",
                        "summary": "当前状态"
                    }
                }
            }
        }"#;
        let ask_raw = r#"{
            "protocol": "flyflor.ws.v1",
            "id": "env-ask-1",
            "type": "ask.snapshot",
            "payload": {
                "data": {
                    "ask": {
                        "prompt": "是否继续？",
                        "snapshotId": "ask-snapshot-1",
                        "options": ["继续"]
                    }
                }
            }
        }"#;

        let thought = parse_context_snapshot(thought_raw)
            .expect("thought should parse")
            .expect("thought event");
        let blackboard = parse_context_snapshot(blackboard_raw)
            .expect("blackboard should parse")
            .expect("blackboard event");
        let ask = parse_context_snapshot(ask_raw)
            .expect("ask should parse")
            .expect("ask event");
        let mut app = App::new();
        app.apply_socket_event(thought);
        app.apply_socket_event(blackboard);
        app.apply_socket_event(ask);

        assert!(app.turns.iter().any(|turn| {
            turn.context_rows
                .iter()
                .any(|row| row.kind == ContextRowKind::Thought)
        }));
        assert!(app.turns.iter().any(|turn| {
            turn.context_rows
                .iter()
                .any(|row| row.kind == ContextRowKind::Blackboard)
        }));
        assert!(app.ask_menu.is_some());
    }

    #[test]
    fn top_level_continuation_metadata_creates_ask_resume_row() {
        let metadata = Some(json!({
            "continuation": {
                "mode": "continue",
                "continuationId": "continuation-1",
                "summary": "继续上一轮 ASK"
            }
        }));

        let rows = context_rows_from_metadata(&metadata);
        assert!(rows.iter().any(|row| {
            row.kind == ContextRowKind::AskResume && row.summary == "继续上一轮 ASK"
        }));

        let turn = Turn {
            message_id: Some("message-1".to_string()),
            event_id: Some("event-1".to_string()),
            user: "继续".to_string(),
            thought: None,
            answer: String::new(),
            metadata,
            context_rows: Vec::new(),
            pending_continuation: None,
            footer: String::new(),
        };
        let continuation = continuation_from_turn(&turn).expect("continuation should parse");
        assert_eq!(
            continuation.get("continuationId").and_then(Value::as_str),
            Some("continuation-1")
        );
    }

    #[test]
    fn executive_tool_loop_does_not_create_ask_resume_or_continuation() {
        let metadata = Some(json!({
            "executiveToolLoop": {
                "message": "工具循环等待继续",
                "resume": {
                    "mode": "continue",
                    "snapshotId": "should-not-resume"
                }
            }
        }));

        let rows = context_rows_from_metadata(&metadata);
        assert!(!rows.iter().any(|row| row.kind == ContextRowKind::AskResume));

        let turn = Turn {
            message_id: Some("message-1".to_string()),
            event_id: Some("event-1".to_string()),
            user: "继续".to_string(),
            thought: None,
            answer: String::new(),
            metadata,
            context_rows: Vec::new(),
            pending_continuation: None,
            footer: String::new(),
        };
        assert!(continuation_from_turn(&turn).is_none());
    }

    #[test]
    fn osc52_sequence_encodes_text_clipboard_write() {
        let sequence = osc52_sequence("hello", false).expect("sequence");
        assert_eq!(sequence, "\x1b]52;c;aGVsbG8=\x07");
    }

    #[test]
    fn osc52_sequence_wraps_for_tmux_passthrough() {
        let sequence = osc52_sequence("copy", true).expect("sequence");
        assert_eq!(sequence, "\x1bPtmux;\x1b\x1b]52;c;Y29weQ==\x07\x1b\\");
    }

    #[test]
    fn osc52_sequence_rejects_oversized_selection() {
        let text = "x".repeat(OSC52_MAX_BYTES + 1);
        let err = osc52_sequence(&text, false).expect_err("oversized should fail");
        assert!(err.contains("too large"), "unexpected error: {err}");
    }

    #[test]
    fn strips_transcript_rails_from_selection_text() {
        let text = " │ answer line\nplain\n   │ nested";
        assert_eq!(strip_transcript_rails(text), "answer line\nplain\nnested");
    }

    #[test]
    fn slices_display_columns_with_wide_characters() {
        assert_eq!(slice_display_columns("a你好b", 1, 5), "你好");
        assert_eq!(slice_display_columns("a你好b", 2, 4), "你好");
    }

    #[test]
    fn transcript_selection_rejects_zero_width_selection() {
        let mut app = App::new();
        app.selection.anchor = Some(SelectionPoint {
            target: SelectionTarget::Left,
            line_index: 1,
            column: 3,
        });
        app.selection.head = Some(SelectionPoint {
            target: SelectionTarget::Left,
            line_index: 1,
            column: 3,
        });
        assert!(!app.selection_has_content());
    }

    #[test]
    fn applies_selection_with_document_offset() {
        let theme = Theme::default();
        let mut lines = vec![Line::raw("alpha"), Line::raw("beta"), Line::raw("gamma")];
        let selection = TranscriptSelection {
            anchor: Some(SelectionPoint {
                target: SelectionTarget::Left,
                line_index: 11,
                column: 1,
            }),
            head: Some(SelectionPoint {
                target: SelectionTarget::Left,
                line_index: 11,
                column: 3,
            }),
            dragging: false,
        };

        apply_selection_to_lines(&mut lines, 10, selection, SelectionTarget::Left, &theme);

        assert_eq!(lines[0].spans.len(), 1);
        assert_eq!(lines[1].spans.len(), 3);
        assert_eq!(lines[2].spans.len(), 1);
    }

    #[test]
    fn selection_endpoints_must_stay_in_same_panel() {
        let selection = TranscriptSelection {
            anchor: Some(SelectionPoint {
                target: SelectionTarget::Left,
                line_index: 0,
                column: 0,
            }),
            head: Some(SelectionPoint {
                target: SelectionTarget::Right,
                line_index: 0,
                column: 4,
            }),
            dragging: false,
        };

        assert!(selection.ordered_endpoints().is_none());
    }

    #[test]
    fn selection_to_text_uses_right_panel_lines_independently() {
        let mut app = App::new();
        app.right_lines = vec![Line::raw("model flyflor"), Line::raw("status connected")];
        app.chat_lines = vec![Line::raw("left panel")];
        app.selection.anchor = Some(SelectionPoint {
            target: SelectionTarget::Right,
            line_index: 0,
            column: 6,
        });
        app.selection.head = Some(SelectionPoint {
            target: SelectionTarget::Right,
            line_index: 1,
            column: 6,
        });

        assert_eq!(app.selection_to_text().as_deref(), Some("flyflor\nstatus"));
    }

    #[test]
    fn visible_chat_lines_preserve_selection_copy_path() {
        let theme = Theme::default();
        let mut app = App::new();
        app.chat_lines = vec![
            Line::raw("alpha"),
            Line::raw("beta"),
            Line::raw("gamma"),
            Line::raw("delta"),
        ];
        app.left.scroll = 2;
        app.left.viewport_height = 1;
        app.selection.anchor = Some(SelectionPoint {
            target: SelectionTarget::Left,
            line_index: 0,
            column: 1,
        });
        app.selection.head = Some(SelectionPoint {
            target: SelectionTarget::Left,
            line_index: 1,
            column: 2,
        });

        let visible = app.visible_chat_lines(&theme);

        assert_eq!(visible.len(), 1);
        assert_eq!(line_plain_text(&visible[0]), "gamma");
        assert_eq!(app.selection_to_text().as_deref(), Some("lpha\nbe"));
    }

    #[test]
    fn visible_line_slice_clones_only_viewport_rows() {
        let lines = (0..100)
            .map(|index| Line::raw(format!("line {index}")))
            .collect::<Vec<_>>();

        let visible = visible_line_slice(&lines, 40, 5);

        assert_eq!(visible.len(), 5);
        assert_eq!(
            visible.first().map(line_plain_text).as_deref(),
            Some("line 40")
        );
        assert_eq!(
            visible.last().map(line_plain_text).as_deref(),
            Some("line 44")
        );
    }

    #[test]
    fn cached_chat_render_keeps_context_hitboxes_on_scroll() {
        let theme = Theme::default();
        let mut app = App::new();
        app.turns.push(Turn {
            message_id: Some("message-1".to_string()),
            event_id: Some("event-1".to_string()),
            user: "u".to_string(),
            thought: None,
            answer: "a".to_string(),
            metadata: Some(json!({
                "planning": {
                    "contextForks": [{ "id": "fork-1", "title": "fork 摘要" }]
                }
            })),
            context_rows: context_rows_from_metadata(&Some(json!({
                "planning": {
                    "contextForks": [{ "id": "fork-1", "title": "fork 摘要" }]
                }
            }))),
            pending_continuation: None,
            footer: String::new(),
        });
        let area = Rect::new(0, 0, 80, 8);

        app.update_left_viewport(area, &theme);
        let key = app.chat_render_key;
        assert!(!app.context_row_hitboxes.is_empty());

        app.left.scroll = 1;
        app.update_left_viewport(area, &theme);

        assert_eq!(app.chat_render_key, key);
        assert!(!app.context_row_hitboxes.is_empty());
    }

    #[test]
    fn left_scrollbar_moves_right_one_unit_without_crossing_panel_edge() {
        let mut state = ScrollState::default();
        let area = Rect::new(2, 0, 40, 8);
        let lines = (0..40)
            .map(|index| Line::raw(format!("line {index}")))
            .collect::<Vec<_>>();

        update_scroll_state_from_rendered(&lines, &mut state, area);

        assert_eq!(state.scrollbar.x, area.right() - 1);
        assert!(state.scrollbar.x < area.right());
        assert_eq!(state.scrollbar.hit_area.right(), area.right());
    }

    #[test]
    fn right_panel_sections_render_without_focus() {
        let mut app = App::new();
        let area = Rect::new(0, 0, 48, 20);
        app.update_right_viewport(area);

        assert!(app.right_sections.len() >= 4);
        assert_eq!(app.right_sections[0].title, "ACT 计划");
        let model = app.right_sections[2]
            .lines
            .iter()
            .map(line_plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(model.contains("Model / Status"));
        assert!(!model.contains('{'));
    }

    #[test]
    fn right_viewport_keeps_todo_sticky_and_scrolls_other_sections() {
        let mut app = App::new();
        let area = Rect::new(0, 0, 48, 20);
        app.update_right_viewport(area);

        assert_eq!(app.right_sections[0].title, "ACT 计划");
        assert_eq!(
            line_plain_text(&render_right_section_title(
                &app.right_sections[0],
                area.width as usize,
                true
            )),
            "› ACT 计划"
        );
        let todo_title = line_plain_text(&render_right_section_title(
            &app.right_sections[0],
            area.width as usize,
            true,
        ));
        assert_eq!(todo_title.matches("ACT 计划").count(), 1);
        let todo_body_lines = app.right_sections[0]
            .lines
            .iter()
            .map(line_plain_text)
            .collect::<Vec<_>>();
        let todo_body = todo_body_lines.join("\n");
        assert!(
            todo_body_lines
                .iter()
                .all(|line| !line.contains("ACT 计划"))
        );
        assert!(!todo_body.contains("状态：暂无计划"));
        assert_eq!(todo_body.matches("暂无计划 [-]").count(), 1);
        let scroll_text = app
            .right_lines
            .iter()
            .map(line_plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!scroll_text.contains("ACT 计划"));
        assert!(!scroll_text.contains("状态：暂无计划"));
        assert!(!scroll_text.contains("暂无计划 [-]"));
        assert!(!scroll_text.contains("ASK / Questions"));
        assert!(!scroll_text.contains("Blackboard"));
        assert!(scroll_text.contains("Model / Status"));
        assert!(!scroll_text.contains("Context Window"));
        assert!(scroll_text.contains("□"));
        assert!(scroll_text.contains("Fork / Memory"));
    }

    #[test]
    fn todo_panel_title_stays_fixed_and_body_flexes() {
        let inner = Rect::new(4, 2, 44, 24);
        let app = App::new();
        let bottom_height = right_bottom_height(
            &app.current_right_panel_data(),
            inner.width.saturating_sub(2) as usize,
            inner.height,
        );
        let layout = right_panel_layout(inner, bottom_height);
        let todo_body = right_todo_body_area(layout.todo);

        assert_eq!(
            layout.todo.height,
            inner.height - layout.separator.height - layout.bottom.height
        );
        assert_eq!(todo_body.y, layout.todo.y + 1);
        assert_eq!(todo_body.height, layout.todo.height - 1);
        assert_eq!(layout.separator.y, layout.todo.bottom());
        assert_eq!(layout.bottom.bottom(), inner.bottom());
    }

    #[test]
    fn right_sections_do_not_render_ask_or_blackboard_panels() {
        let mut app = App::new();
        app.right_source.blackboard_status = "blackboard should not render".to_string();
        app.right_source.blackboard_stream = vec!["blackboard event".to_string()];
        let sections =
            render_right_panel_sections(&app.current_right_panel_data(), &app.visible_todos(), 60);
        let text = sections
            .iter()
            .flat_map(|section| section.lines.iter())
            .map(line_plain_text)
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            sections
                .iter()
                .all(|section| section.title != "ASK / Questions")
        );
        assert!(
            sections
                .iter()
                .all(|section| !section.title.contains("Blackboard"))
        );
        assert!(!text.contains("ASK should stay near composer"));
        assert!(!text.contains("blackboard should not render"));
        assert!(!text.contains("blackboard event"));
        assert!(text.contains("Fork / Memory"));
    }

    #[test]
    fn right_sections_keep_todo_model_context_order() {
        let app = App::new();
        let sections =
            render_right_panel_sections(&app.current_right_panel_data(), &app.visible_todos(), 60);
        let titles = sections
            .iter()
            .map(|section| section.title.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            &titles[..4],
            ["ACT 计划", "Run", "Model / Status", "Context Window",]
        );
        let rendered = sections
            .iter()
            .flat_map(|section| section.lines.iter())
            .map(line_plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!rendered.contains("Context Window"));
        assert!(rendered.contains("□"));
        assert!(titles.iter().all(|title| *title != "操作提示"));
    }

    #[test]
    fn todo_panel_renders_plan_rows_under_title() {
        let todos = vec![
            TodoItem {
                marker: "●".to_string(),
                label: "实现布局".to_string(),
                status: "进行中".to_string(),
                active: true,
                plan_id: Some("plan-1".to_string()),
            },
            TodoItem {
                marker: "○".to_string(),
                label: "验证测试".to_string(),
                status: "待办".to_string(),
                active: false,
                plan_id: Some("plan-1".to_string()),
            },
        ];
        let sections =
            render_right_panel_sections(&App::new().current_right_panel_data(), &todos, 48);

        assert_eq!(sections[0].title, "ACT 计划");
        let title = line_plain_text(&render_right_section_title(&sections[0], 48, true));
        let body = sections[0]
            .lines
            .iter()
            .map(line_plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(title, "› ACT 计划");
        assert!(body.contains("实现布局"));
        assert!(body.contains("验证测试"));
    }

    #[test]
    fn fork_memory_rendered_rows_stay_one_line_after_section_rendering() {
        let mut data = App::new().current_right_panel_data();
        data.fork_memory = ForkMemorySnapshot {
            forks: (0..5)
                .map(|index| ForkMemoryItem {
                    summary: format!("这是第 {index} 条非常长的中文 fork 摘要，不应该换行展示也不应该暴露 id internal"),
                    time: Some("2026-05-24T00:00:00.000Z".to_string()),
                })
                .collect(),
            brain_db_human: Some("12.4 MB".to_string()),
            brain_db_status: Some("available".to_string()),
        };
        let width = 26;
        let sections = render_right_panel_sections(&data, &[TodoItem::empty_plan()], width);
        let fork = sections
            .iter()
            .find(|section| section.title == "Fork / Memory")
            .expect("fork memory section");
        let lines = fork.lines.iter().map(line_plain_text).collect::<Vec<_>>();
        let fork_rows = lines
            .iter()
            .filter(|line| {
                line.trim_start()
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_digit())
            })
            .collect::<Vec<_>>();

        assert_eq!(fork_rows.len(), 5);
        for (index, row) in fork_rows.iter().enumerate() {
            assert!(row.trim_start().starts_with(&format!("{}.", index + 1)));
            assert!(row.contains('…'));
            assert!(!row.contains('\n'));
            assert!(UnicodeWidthStr::width(row.as_str()) <= width);
        }
        let joined = lines.join("\n");
        assert!(!joined.contains("id:"));
        assert!(!joined.contains("internal:"));
    }

    #[test]
    fn model_status_omits_idle_fork_and_unknown_model_is_not_idle() {
        let app = App::new();
        let sections =
            render_right_panel_sections(&app.current_right_panel_data(), &app.visible_todos(), 60);
        let model = sections
            .iter()
            .find(|section| section.title == "Model / Status")
            .expect("model section")
            .lines
            .iter()
            .map(line_plain_text)
            .collect::<Vec<_>>()
            .join("\n");

        assert!(!model.contains("fork"));
        assert!(!model.contains("空闲"));
        assert!(!model.contains("ws:"));
        assert!(!model.contains(DEFAULT_WS_URL));
        assert!(model.contains("model: 未知模型"));
    }

    #[test]
    fn fork_memory_renders_recent_forks_and_brain_db_human() {
        let snapshot = ForkMemorySnapshot {
            forks: vec![
                ForkMemoryItem {
                    summary: "实现右侧布局".to_string(),
                    time: Some("2026-05-24T00:00:00.000Z".to_string()),
                },
                ForkMemoryItem {
                    summary: "接入 fork memory".to_string(),
                    time: Some("2026-05-24T00:05:00.000Z".to_string()),
                },
            ],
            brain_db_human: Some("12.4 MB".to_string()),
            brain_db_status: Some("available".to_string()),
        };
        let text = fork_memory_rows(&snapshot).join("\n");

        assert!(text.contains("fork 最近 5 条"));
        assert!(text.contains("1. 实现右侧布局 · 2026-05-24T00:00:00.000Z"));
        assert!(text.contains("2. 接入 fork memory · 2026-05-24T00:05:00.000Z"));
        assert!(text.contains("brain.db: 12.4 MB"));
        assert!(!text.contains('{'));
    }

    #[test]
    fn fork_memory_missing_and_unavailable_brain_db_labels_are_readable() {
        let missing = fork_memory_rows(&ForkMemorySnapshot::default()).join("\n");
        assert!(missing.contains("fork: 暂无数据"));
        assert!(missing.contains("brain.db: 未收到"));

        let unavailable = ForkMemorySnapshot {
            brain_db_status: Some("unavailable".to_string()),
            ..ForkMemorySnapshot::default()
        };
        assert!(
            fork_memory_rows(&unavailable)
                .join("\n")
                .contains("brain.db: 不可用")
        );
    }

    #[test]
    fn fork_memory_recent_forks_are_one_line_and_truncated_to_width() {
        let snapshot = ForkMemorySnapshot {
            forks: (0..6)
                .map(|index| ForkMemoryItem {
                    summary: format!("这是一个非常长的 fork 摘要 {index}，应该保持单行并被省略"),
                    time: Some("2026-05-24T00:00:00.000Z".to_string()),
                })
                .collect(),
            brain_db_human: Some("12.4 MB".to_string()),
            brain_db_status: Some("available".to_string()),
        };
        let width = 24;
        let rows = fork_memory_rows_for_width(&snapshot, width);
        let fork_rows = rows
            .iter()
            .filter(|row| row.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
            .collect::<Vec<_>>();

        assert_eq!(fork_rows.len(), 5);
        for (index, row) in fork_rows.iter().enumerate() {
            assert!(row.starts_with(&format!("{}.", index + 1)));
            assert!(!row.contains('\n'));
            assert!(UnicodeWidthStr::width(row.as_str()) <= width.saturating_sub(3).max(1));
        }
        assert!(fork_rows.iter().any(|row| row.contains('…')));
        assert!(!rows.join("\n").contains("id:"));
    }

    #[test]
    fn parses_fork_memory_snapshot_core_payload() {
        let raw = r#"{
            "protocol": "flyflor.ws.v1",
            "id": "env-fork-memory-1",
            "type": "fork.memory.snapshot",
            "payload": {
                "data": {
                    "brainDb": {
                        "bytes": 13002342,
                        "human": "12.4 MB",
                        "path": "/core/brain.db",
                        "status": "available"
                    },
                    "forks": [
                        {
                            "id": "fork-1",
                            "title": "标题 1",
                            "summary": "摘要 1",
                            "createdAt": "2026-05-24T00:00:00.000Z",
                            "updatedAt": "2026-05-24T00:05:00.000Z"
                        },
                        {
                            "id": "fork-2",
                            "title": "标题 2",
                            "createdAt": "2026-05-24T00:01:00.000Z"
                        }
                    ]
                }
            }
        }"#;

        let event = parse_fork_memory_snapshot(raw)
            .expect("fork memory should parse")
            .expect("fork memory event");

        match event {
            SocketEvent::ForkMemoryLoaded(snapshot) => {
                assert_eq!(snapshot.brain_db_human.as_deref(), Some("12.4 MB"));
                assert_eq!(snapshot.brain_db_status.as_deref(), Some("available"));
                assert_eq!(snapshot.forks.len(), 2);
                assert_eq!(snapshot.forks[0].summary, "摘要 1");
                assert_eq!(
                    snapshot.forks[0].time.as_deref(),
                    Some("2026-05-24T00:05:00.000Z")
                );
                assert_eq!(snapshot.forks[1].summary, "标题 2");
            }
            _ => panic!("expected fork memory loaded"),
        }
    }

    #[test]
    fn right_layout_keeps_todo_flex_and_bottom_status_fixed() {
        let inner = Rect::new(10, 5, 40, 20);
        let layout = right_panel_layout(inner, 6);

        assert_eq!(
            layout.todo.height,
            inner.height - layout.separator.height - layout.bottom.height
        );
        assert_eq!(layout.todo, Rect::new(10, 5, 38, 13));
        assert_eq!(layout.separator, Rect::new(10, 18, 38, 1));
        assert_eq!(layout.separator.y, layout.todo.bottom());
        assert_eq!(layout.bottom, Rect::new(10, 19, 40, 6));
        assert_eq!(layout.bottom.y, layout.separator.bottom());
        assert_eq!(layout.bottom.bottom(), inner.bottom());
        assert_eq!(layout.bottom_text, Rect::new(10, 19, 38, 6));
        assert_eq!(separator_text(layout.separator.width), "─".repeat(38));

        let app = App::new();
        let text_width = layout.bottom_text.width as usize;
        let bottom_height =
            right_bottom_height(&app.current_right_panel_data(), text_width, inner.height);
        let actual = right_panel_layout(inner, bottom_height);
        assert_eq!(
            actual.todo.height,
            inner.height - actual.separator.height - actual.bottom.height
        );
        assert_eq!(actual.bottom.bottom(), inner.bottom());
        assert_eq!(actual.bottom.y, actual.separator.bottom());
        let sections = render_right_panel_sections(
            &app.current_right_panel_data(),
            &[TodoItem::empty_plan()],
            text_width,
        );
        let bottom_text = flatten_right_panel_sections(scrollable_right_sections(&sections))
            .iter()
            .map(line_plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(bottom_text.contains("Model / Status"));
        assert!(!bottom_text.contains("Context Window"));
        assert!(bottom_text.contains("□"));
        assert!(bottom_text.contains("Fork / Memory"));
    }

    #[test]
    fn right_todo_scrolls_without_overall_right_scrollbar() {
        let mut app = App::new();
        app.task_todos = Some(
            (0..12)
                .map(|index| TodoItem {
                    marker: "○".to_string(),
                    label: format!("步骤 {index}"),
                    status: "todo".to_string(),
                    active: index == 0,
                    plan_id: Some("plan-scroll".to_string()),
                })
                .collect(),
        );
        app.update_right_viewport(Rect::new(10, 0, 30, 4));

        assert!(app.right.max_scroll > 0);
        assert_eq!(app.right.scrollbar.x, 39);
        assert_eq!(
            app.right_lines.first().map(line_plain_text).as_deref(),
            Some("  Run")
        );
        assert!(
            app.right_lines
                .iter()
                .map(line_plain_text)
                .any(|line| line.contains("□"))
        );
    }

    #[test]
    fn right_todo_wheel_changes_only_todo_scroll() {
        let mut app = App::new();
        app.task_todos = Some(
            (0..12)
                .map(|index| TodoItem {
                    marker: "○".to_string(),
                    label: format!("步骤 {index}"),
                    status: "todo".to_string(),
                    active: index == 0,
                    plan_id: Some("plan-scroll".to_string()),
                })
                .collect(),
        );
        app.layout.right_panel = Rect::new(60, 0, 40, 24);
        let todo_area = app.right_todo_area().expect("todo area");
        app.update_right_viewport(todo_area);
        let fixed_before = app
            .right_lines
            .iter()
            .map(line_plain_text)
            .collect::<Vec<_>>();

        handle_mouse(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: todo_area.x + 1,
                row: todo_area.y + 1,
                modifiers: KeyModifiers::empty(),
            },
        );

        assert!(app.right.scroll > 0);
        assert_eq!(
            fixed_before,
            app.right_lines
                .iter()
                .map(line_plain_text)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn right_todo_scrollbar_drag_changes_todo_scroll() {
        let mut app = App::new();
        app.task_todos = Some(
            (0..20)
                .map(|index| TodoItem {
                    marker: "○".to_string(),
                    label: format!("步骤 {index}"),
                    status: "todo".to_string(),
                    active: index == 0,
                    plan_id: Some("plan-scroll".to_string()),
                })
                .collect(),
        );
        app.layout.right_panel = Rect::new(60, 0, 40, 24);
        let todo_area = app.right_todo_area().expect("todo area");
        app.update_right_viewport(todo_area);
        let fixed_before = app
            .right_lines
            .iter()
            .map(line_plain_text)
            .collect::<Vec<_>>();
        let scrollbar = app.right.scrollbar;

        handle_mouse(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: scrollbar.x,
                row: scrollbar.track_top,
                modifiers: KeyModifiers::empty(),
            },
        );
        handle_mouse(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::Drag(MouseButton::Left),
                column: scrollbar.x,
                row: scrollbar.track_top + 4,
                modifiers: KeyModifiers::empty(),
            },
        );

        assert!(app.right.scroll > 0);
        assert_eq!(
            fixed_before,
            app.right_lines
                .iter()
                .map(line_plain_text)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn demo_fixture_makes_tui_qa_states_visible() {
        let mock = demo_mock_data();
        let text = mock
            .turns
            .iter()
            .flat_map(|turn| turn.context_rows.iter())
            .map(|row| format!("{} {}", context_row_label(row.kind), row.summary))
            .chain(mock.todos.iter().map(|todo| todo.label.clone()))
            .chain(mock.right_panel.blackboard_stream.iter().cloned())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(text.contains("回忆中"));
        assert!(text.contains("fork"));
        assert!(text.contains("blackboard"));
        assert!(text.contains("修复复制卡顿"));
        assert!(mock.right_panel.context_bar.contains('■'));
        let mut app = App::new();
        app.model_context_window_tokens = Some(12000);
        app.hot_context_tokens = Some(3360);
        app.max_output_tokens = Some(2048);
        let data = app.current_right_panel_data();
        assert!(data.context_total.contains("12k"));
        assert!(data.context_usage.contains("3.4k/12k 28.00%"));
        assert!(
            mock.turns
                .iter()
                .flat_map(|turn| turn.context_rows.iter())
                .any(|row| row.kind == ContextRowKind::AskResume)
        );
        assert!(
            mock.turns
                .iter()
                .flat_map(|turn| turn.context_rows.iter())
                .any(|row| row.kind == ContextRowKind::CreateFork)
        );
        let mut app = App::new().with_demo_state(true);
        app.todos = mock.todos.clone();
        app.task_todos = Some(mock.todos.clone());
        assert!(app.yolo_mode);
        assert_eq!(app.interaction_mode, InteractionMode::Yolo);
        assert!(app.pending_fork_create);
        let data = app.current_right_panel_data();
        assert!(data.thinking_label.contains("YOLO"));
        assert_eq!(app.plan_state(), PlanState::AwaitingConfirmation);
    }

    #[test]
    fn ctrl_c_clears_input_and_shows_exit_hint() {
        let mut app = App::new();
        app.input = "draft".to_string();

        app.handle_ctrl_c();

        assert!(app.input.is_empty());
        assert!(matches!(
            app.composer_notice,
            Some(ComposerNotice::ExitHint)
        ));
    }

    #[test]
    fn ctrl_c_on_empty_input_keeps_exit_hint_visible() {
        let mut app = App::new();

        app.handle_ctrl_c();

        assert!(app.input.is_empty());
        assert!(matches!(
            app.composer_notice,
            Some(ComposerNotice::ExitHint)
        ));
    }

    #[test]
    fn shift_tab_toggles_interaction_mode() {
        let mut app = App::new();
        assert_eq!(app.interaction_mode, InteractionMode::Act);

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
        );

        assert_eq!(app.interaction_mode, InteractionMode::Plan);
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
        );
        assert_eq!(app.interaction_mode, InteractionMode::Yolo);
        assert!(app.yolo_mode);
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
        );
        assert_eq!(app.interaction_mode, InteractionMode::Act);
        assert!(!app.yolo_mode);
    }

    #[test]
    fn shift_tab_mode_is_visible_in_right_panel() {
        let mut app = App::new();
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
        );
        let data = app.current_right_panel_data();

        assert_eq!(data.thinking_label, "PLAN");
        assert!(
            data.model_stats
                .iter()
                .any(|stat| stat.label == "mode" && stat.value == "PLAN")
        );
    }

    #[test]
    fn shift_tab_mode_is_visible_before_enter_in_footer_with_distinct_style() {
        let theme = Theme::default();
        let mut app = App::new();
        let act_footer = composer_footer_line(&app, &theme);
        let act_text = line_text(&act_footer);
        let act_color = act_footer.spans[0].style.fg;

        assert!(act_text.starts_with("ACT · Enter 发送"));
        assert_eq!(act_color, Some(theme.text));

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
        );
        let plan_footer = composer_footer_line(&app, &theme);
        let plan_text = line_text(&plan_footer);
        let plan_color = plan_footer.spans[0].style.fg;

        assert!(plan_text.starts_with("PLAN · Enter 发送"));
        assert_eq!(plan_color, Some(theme.pink));
        assert_ne!(act_color, plan_color);
    }

    #[test]
    fn yolo_status_is_visible_before_enter_in_footer_with_danger_style() {
        let theme = Theme::default();
        let mut app = App::new();
        app.interaction_mode = InteractionMode::Yolo;
        app.yolo_mode = true;

        let footer = composer_footer_line(&app, &theme);
        let text = line_text(&footer);

        assert!(text.starts_with("YOLO · Enter 发送"));
        assert_eq!(footer.spans[0].style.fg, Some(theme.danger));
    }

    #[test]
    fn empty_todo_list_displays_no_plan() {
        let app = App::new();
        let todos = app.visible_todos();

        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].label, "暂无计划");
        assert!(!todos[0].active);
    }

    #[test]
    fn task_plan_metadata_drives_todo_list() {
        let mut turn = test_turn("u", "a");
        turn.metadata = Some(json!({
            "planning": {
                "taskPlans": [{
                    "planId": "plan-1",
                    "steps": [
                        { "title": "读取协议", "status": "done" },
                        { "title": "实现 TUI", "status": "todo" }
                    ]
                }]
            }
        }));

        let todos = todos_from_turns(&[turn]);

        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0].status, "完成");
        assert_eq!(todos[1].label, "实现 TUI");
        assert!(todos[1].active);
        assert_eq!(todos[1].plan_id.as_deref(), Some("plan-1"));
    }

    #[test]
    fn todo_waiting_confirmation_renders_plan_actions_and_decides() {
        let mut app = App::new();
        app.task_todos = Some(vec![TodoItem {
            marker: "›".to_string(),
            label: "确认实施计划".to_string(),
            status: "等待确认".to_string(),
            active: true,
            plan_id: Some("plan-test-1".to_string()),
        }]);

        assert_eq!(app.plan_state(), PlanState::AwaitingConfirmation);
        let sections =
            render_right_panel_sections(&app.current_right_panel_data(), &app.visible_todos(), 60);
        let text = std::iter::once(sections[0].title.clone())
            .chain(sections[0].lines.iter().map(line_plain_text))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("状态：等待确认"));
        assert!(text.contains("确认计划"));
        assert!(text.contains("补充计划"));
        assert!(text.contains("放弃计划"));

        app.open_plan_menu();
        app.confirm_plan_menu_selection();
        assert!(app.right_source.blackboard_status.contains("plan-test-1"));

        let envelope = gateway_command_builder()
            .task_plan_decide(1, "plan-test-1", PlanAction::Revise, Some("补充边界"))
            .into_value();
        assert_eq!(
            envelope.get("type").and_then(Value::as_str),
            Some("task.plan.decide")
        );
        assert_eq!(
            envelope
                .get("payload")
                .and_then(|payload| payload.get("planId"))
                .and_then(Value::as_str),
            Some("plan-test-1")
        );
        assert_eq!(
            envelope
                .get("payload")
                .and_then(|payload| payload.get("action"))
                .and_then(Value::as_str),
            Some("revise")
        );
        assert_eq!(
            envelope
                .get("payload")
                .and_then(|payload| payload.get("revision"))
                .and_then(Value::as_str),
            Some("补充边界")
        );
    }

    #[test]
    fn plan_decision_targets_waiting_confirmation_plan_over_first_plan_id() {
        let mut app = App::new();
        let (tx, rx) = mpsc::channel();
        app.socket_tx = tx;
        app.task_todos = Some(vec![
            TodoItem {
                marker: "›".to_string(),
                label: "执行中".to_string(),
                status: "进行中".to_string(),
                active: true,
                plan_id: Some("plan-running".to_string()),
            },
            TodoItem {
                marker: "○".to_string(),
                label: "确认下一段".to_string(),
                status: "等待确认".to_string(),
                active: false,
                plan_id: Some("plan-awaiting".to_string()),
            },
        ]);

        app.open_plan_menu();
        app.confirm_plan_menu_selection();

        let command = rx.try_recv().expect("plan decide command");
        match command {
            SocketCommand::TaskPlanDecide {
                plan_id, action, ..
            } => {
                assert_eq!(plan_id, "plan-awaiting");
                assert_eq!(action, PlanAction::Confirm);
            }
            _ => panic!("expected task.plan.decide command"),
        }
    }

    #[test]
    fn plan_decision_targets_waiting_confirmation_plan_from_turn_metadata() {
        let mut app = App::new();
        let (tx, rx) = mpsc::channel();
        app.socket_tx = tx;
        let mut turn = test_turn("u", "a");
        turn.metadata = Some(json!({
            "planning": {
                "taskPlans": [
                    {
                        "planId": "plan-running-meta",
                        "steps": [{ "title": "Run", "status": "running" }]
                    },
                    {
                        "planId": "plan-awaiting-meta",
                        "steps": [{ "title": "Confirm", "status": "awaiting confirmation" }]
                    }
                ]
            }
        }));
        app.turns = vec![turn];

        app.open_plan_menu();
        app.confirm_plan_menu_selection();

        let command = rx.try_recv().expect("plan decide command");
        match command {
            SocketCommand::TaskPlanDecide { plan_id, .. } => {
                assert_eq!(plan_id, "plan-awaiting-meta");
            }
            _ => panic!("expected task.plan.decide command"),
        }
    }

    #[test]
    fn english_waiting_confirmation_status_is_normalized_for_plan_actions() {
        let mut turn = test_turn("u", "a");
        turn.metadata = Some(json!({
            "planning": {
                "taskPlans": [{
                    "planId": "plan-english",
                    "steps": [
                        { "title": "Confirm", "status": "awaiting confirmation" }
                    ]
                }]
            }
        }));

        let todos = todos_from_turns(&[turn]);

        assert_eq!(todos[0].status, "等待确认");
        assert_eq!(
            plan_state_from_todos(&todos),
            PlanState::AwaitingConfirmation
        );
    }

    #[test]
    fn task_list_snapshot_reads_payload_data() {
        let raw = r#"{
            "protocol": "flyflor.ws.v1",
            "id": "env-task-list-1",
            "type": "task.snapshot",
            "at": "2026-05-22T00:00:06.500Z",
            "payload": {
                "data": {
                    "taskPlans": [{
                        "steps": [
                            { "title": "完成历史接入", "status": "completed" },
                            { "title": "接入 task.list", "status": "in-progress" }
                        ]
                    }]
                }
            }
        }"#;

        let event = parse_task_list_snapshot(raw)
            .expect("task snapshot should parse")
            .expect("task snapshot should emit event");

        match event {
            SocketEvent::TaskListLoaded(todos) => {
                assert_eq!(todos.len(), 2);
                assert_eq!(todos[0].status, "完成");
                assert_eq!(todos[1].label, "接入 task.list");
                assert!(todos[1].active);
            }
            _ => panic!("expected task list event"),
        }
    }

    #[test]
    fn task_plan_written_event_requests_task_refresh() {
        let mut app = App::new();

        app.apply_socket_event(SocketEvent::RuntimeEvent(json!({
            "type": "memory.task_plan.written",
            "payload": { "planId": "plan-test-1" }
        })));

        assert!(
            app.right_source
                .blackboard_status
                .contains("task plan updated")
        );
        assert!(
            app.run_timeline
                .items
                .iter()
                .any(|item| item.title == "memory task_plan written")
        );
    }

    #[test]
    fn runtime_event_job_id_in_data_requests_execution_job_detail() {
        let mut app = App::new();
        let (tx, rx) = mpsc::channel();
        app.socket_tx = tx;

        app.apply_socket_event(SocketEvent::RuntimeEvent(json!({
            "type": "subagent.child.start",
            "data": {
                "jobId": "job-data-1",
                "childId": "child-1"
            }
        })));

        match rx.try_recv().expect("job detail command") {
            SocketCommand::ExecutionJobDetailGet { job_id } => {
                assert_eq!(job_id, "job-data-1");
            }
            _ => panic!("expected execution job detail command"),
        }
    }

    #[test]
    fn blackboard_subscription_events_update_structured_stream() {
        let raw = r#"{
            "protocol": "flyflor.ws.v1",
            "id": "env-event-1",
            "type": "event.publish",
            "payload": {
                "type": "blackboard.message.appended",
                "data": { "text": "记录复制路径优化" }
            }
        }"#;
        let event = parse_subscription_event(raw)
            .expect("event should parse")
            .expect("blackboard event");
        let mut app = App::new();

        app.apply_socket_event(event);

        assert!(
            app.right_source
                .blackboard_stream
                .iter()
                .any(|line| line.contains("记录复制路径优化"))
        );
        assert!(
            app.run_timeline
                .items
                .iter()
                .any(|item| item.title == "blackboard message appended")
        );
    }

    #[test]
    fn runtime_events_update_run_timeline_panel() {
        let raw = r#"{
            "protocol": "flyflor.ws.v1",
            "id": "env-event-1",
            "type": "event.publish",
            "payload": {
                "type": "subagent.child.end",
                "payload": {
                    "batchId": "batch-1",
                    "childId": "child-1",
                    "status": "needs_user",
                    "summary": "Pick an option"
                }
            }
        }"#;
        let event = parse_subscription_event(raw)
            .expect("event should parse")
            .expect("runtime event");
        let mut app = App::new();

        app.apply_socket_event(event);
        app.update_right_viewport(Rect::new(0, 0, 60, 20));
        let right = app
            .right_lines
            .iter()
            .map(line_plain_text)
            .collect::<Vec<_>>()
            .join("\n");

        assert!(right.contains("Run"));
        assert!(!right.contains("Subagents"));
        assert!(right.contains("child-1"));
    }

    #[test]
    fn context_window_estimate_reports_missing_window_without_model_limit() {
        let estimate = estimate_context_window(
            &[test_turn("hello", "world")],
            &None,
            &StatusSnapshot::default(),
        );

        assert_eq!(estimate.total, "未收到上下文窗口");
        assert_eq!(estimate.percent, "未知");
        assert!(estimate.usage.contains("/未知 未知"));
        assert!(estimate.bar.contains('□'));
        assert!(!estimate.bar.contains('─'));
    }

    #[test]
    fn context_window_uses_model_mapping_for_deepseek() {
        let status = StatusSnapshot {
            hot_context_tokens: Some(12_345),
            model_name: Some("deepseek-chat".to_string()),
            ..StatusSnapshot::default()
        };
        let estimate = estimate_context_window(&[], &None, &status);

        assert_eq!(estimate.total, "最大 1M tokens");
        assert_eq!(estimate.percent, "1.23%");
        assert_eq!(
            estimate.usage,
            "□□□□□□□□□□□□□□□□□□□□□□□□□□□□□□□□ 12.3k/1M 1.23%"
        );
        assert!(!estimate.usage.contains("12,345/1,000,000"));
        assert!(estimate.usage.contains('□'));
        assert!(!estimate.usage.contains('─'));
    }

    #[test]
    fn context_usage_adapts_to_width_without_truncation_or_wrapping() {
        let mut data = RightPanelData {
            thinking_label: String::new(),
            blackboard_status: String::new(),
            blackboard_stream: Vec::new(),
            model_stats: Vec::new(),
            token_stats: Vec::new(),
            context_total: "最大 1M tokens".to_string(),
            context_percent: "1.23%".to_string(),
            context_bar: context_bar(0.0123, DEFAULT_CONTEXT_BAR_WIDTH),
            context_usage: String::new(),
            context_ratio: 0.0123,
            context_used_tokens: 12_345,
            context_max_tokens: Some(1_000_000),
            context_used: "12.3k".to_string(),
            context_max: "1M".to_string(),
            fork_memory: ForkMemorySnapshot::default(),
            run_timeline: RunTimeline::new(),
            footer: String::new(),
        };

        let normal = context_usage_for_width(&data, 72);
        assert!(normal.contains('□'));
        assert!(normal.chars().filter(|ch| *ch == '■' || *ch == '□').count() >= 32);
        assert!(normal.contains("12.3k/1M 1.23%"));
        assert!(!normal.contains("12,345/1,000,000"));
        assert!(!normal.contains('─'));

        data.context_used_tokens = 12_345_678;
        data.context_max_tokens = Some(1_000_000_000);
        data.context_used = "12.3M".to_string();
        data.context_max = "1B".to_string();
        let narrow_width = 24;
        let content_width = narrow_width - 3;
        let narrow = context_usage_for_width(&data, narrow_width);
        assert!(!narrow.contains('\n'));
        assert!(!narrow.contains('…'));
        assert!(UnicodeWidthStr::width(narrow.as_str()) <= content_width);
        assert!(narrow.contains("12.3M/1B 1.23%"));
    }

    #[test]
    fn status_snapshot_uses_context_window_not_max_output() {
        let raw = r#"{
            "protocol": "flyflor.ws.v1",
            "id": "env-status-1",
            "type": "gateway.status.snapshot",
            "at": "2026-05-22T00:00:06.500Z",
            "payload": {
                "data": {
                    "model": {
                        "contextWindowTokens": 12000,
                        "maxOutputTokens": 2048
                    },
                    "context": {
                        "hotContextTokens": 3000,
                        "remainingContextTokens": 9000
                    },
                    "cache": {
                        "cacheReadTokens": 120,
                        "cacheWriteTokens": 30
                    }
                }
            }
        }"#;

        let event = parse_status_snapshot(raw)
            .expect("status snapshot should parse")
            .expect("status snapshot should emit event");

        match event {
            SocketEvent::StatusLoaded(status) => {
                assert_eq!(status.context_window_tokens, Some(12000));
                assert_eq!(status.max_output_tokens, Some(2048));
                assert_eq!(status.hot_context_tokens, Some(3000));
                assert_eq!(status.remaining_context_tokens, Some(9000));
                let estimate =
                    estimate_context_window(&[test_turn("hello", "world")], &None, &status);
                assert_eq!(estimate.total, "最大 12k tokens");
                assert_eq!(estimate.percent, "25.00%");
                assert_eq!(
                    estimate.usage,
                    "■■■■■■■■□□□□□□□□□□□□□□□□□□□□□□□□ 3k/12k 25.00%"
                );
                assert!(estimate.usage.contains('■'));
                assert!(estimate.usage.contains('□'));
                assert!(!estimate.usage.contains('─'));
            }
            _ => panic!("expected status loaded event"),
        }
    }

    #[test]
    fn status_snapshot_uses_core_gateway_status_shape() {
        let raw = r#"{
            "protocol": "flyflor.ws.v1",
            "id": "env-status-core",
            "type": "gateway.status.snapshot",
            "at": "2026-05-24T00:00:00.000Z",
            "payload": {
                "status": {
                    "model": {
                        "provider": "deepseek",
                        "providerId": "deepseek-prod",
                        "model": "deepseek-chat",
                        "contextWindowTokens": 1000000,
                        "contextUsedTokens": 12345,
                        "currentTokens": 12000,
                        "contextWindowPercent": 1.23,
                        "contextStatus": "ok"
                    },
                    "context": {
                        "currentTokens": 12345,
                        "contextWindowPercent": 1.23,
                        "contextStatus": "ok"
                    }
                }
            }
        }"#;

        let event = parse_status_snapshot(raw)
            .expect("status snapshot should parse")
            .expect("status snapshot should emit event");

        match event {
            SocketEvent::StatusLoaded(status) => {
                assert_eq!(status.model_name, Some("deepseek-chat".to_string()));
                assert_eq!(status.model_provider, Some("deepseek".to_string()));
                assert_eq!(status.context_window_tokens, Some(1_000_000));
                assert_eq!(status.hot_context_tokens, Some(12_345));
                assert_eq!(status.context_window_percent, Some(1.23));
                assert_eq!(status.context_status, Some("ok".to_string()));

                let estimate = estimate_context_window(&[], &None, &status);
                assert_eq!(estimate.total, "最大 1M tokens");
                assert_eq!(estimate.percent, "1.23%");
                assert_eq!(
                    estimate.usage,
                    "□□□□□□□□□□□□□□□□□□□□□□□□□□□□□□□□ 12.3k/1M 1.23%"
                );
                let mut app = App::new();
                app.apply_socket_event(SocketEvent::StatusLoaded(status));
                let data = app.current_right_panel_data();
                let model_values: Vec<&str> = data
                    .model_stats
                    .iter()
                    .map(|item| item.value.as_str())
                    .collect();
                assert!(model_values.contains(&"deepseek-chat"));
                assert!(model_values.contains(&"deepseek"));
                assert!(!model_values.contains(&"未知模型"));
                assert!(!model_values.contains(&"暂无数据"));
                assert_eq!(
                    data.context_usage,
                    "□□□□□□□□□□□□□□□□□□□□□□□□□□□□□□□□ 12.3k/1M 1.23%"
                );
            }
            _ => panic!("expected status loaded event"),
        }
    }

    #[test]
    fn max_output_only_does_not_become_context_window() {
        let status = status_from_data(&json!({
            "model": {
                "maxOutputTokens": 2048
            }
        }));
        let estimate = estimate_context_window(&[test_turn("hello", "world")], &None, &status);

        assert_eq!(status.context_window_tokens, None);
        assert_eq!(estimate.total, "未收到上下文窗口");
        assert!(estimate.usage.contains("/未知 未知"));
    }

    #[test]
    fn top_bar_title_uses_default_ws_url() {
        assert_eq!(
            shared::top_bar_title_for_url(DEFAULT_WS_URL),
            "FlyFlor · Powered By ws://127.0.0.1:8787/ws"
        );
    }

    #[test]
    fn app_layout_keeps_main_composer_and_footer_separate() {
        let root = Rect::new(1, 1, 158, 39);
        let layout = app_layout(root, 1, "hello");

        assert_eq!(layout.header, Rect::new(1, 1, 158, 1));
        assert_eq!(layout.divider.height, layout.left_main.height);
        assert_eq!(layout.extended_divider.bottom(), layout.footer_border.y);
        assert_eq!(layout.divider.x, layout.left_main.right());
        assert_eq!(layout.right_main.x, layout.divider.right());
        assert_eq!(layout.left_composer.x, layout.left_main.x);
        assert_eq!(layout.left_composer.width, layout.left_main.width);
        assert_eq!(layout.right_composer_gap.x, layout.divider.x);
        assert_eq!(layout.right_composer_gap.right(), layout.right_main.right());
        assert_eq!(
            layout.footer_border,
            Rect::new(root.x, root.bottom() - 3, root.width, 1)
        );
        assert_eq!(
            layout.footer,
            Rect::new(root.x, root.bottom() - 2, root.width, 2)
        );
        assert_eq!(
            layout.footer_text,
            Rect::new(root.x + 1, root.bottom() - 2, root.width - 2, 1)
        );
        assert_eq!(
            layout.footer_padding_bottom,
            Rect::new(root.x, root.bottom() - 1, root.width, 1)
        );
        assert_eq!(layout.footer.width, root.width);
        assert!(layout.left_main.bottom() <= layout.left_composer.y);
        assert_eq!(layout.left_composer.height, 2);
        assert_eq!(layout.left_composer.bottom(), layout.footer_border.y);
        assert_eq!(layout.footer_border.bottom(), layout.footer.y);
        assert_eq!(layout.footer.y, layout.footer_text.y);
        assert_eq!(layout.footer_text.bottom(), layout.footer_padding_bottom.y);
        let input_inner = Rect::new(
            layout.left_composer.x + 1,
            layout.left_composer.y + 1,
            layout.left_composer.width.saturating_sub(2),
            layout.left_composer.height.saturating_sub(1),
        );
        assert_eq!(input_inner.bottom(), layout.footer_border.y);
        assert_eq!(layout.footer_border.y - input_inner.bottom(), 0);
        assert_eq!(
            input_cursor_position("hello", "hello".len(), input_inner, 0),
            Some(Position::new(input_inner.x + 5, input_inner.y))
        );
    }

    #[test]
    fn app_layout_expands_composer_for_soft_wrapped_input() {
        let root = Rect::new(0, 0, 120, 32);
        let input = "x".repeat(90);
        let layout = app_layout(root, 1, &input);
        let input_inner_width = layout.left_composer.width.saturating_sub(2) as usize;

        assert!(input_inner_width < input.len());
        assert!(layout.left_composer.height > 2);
        let rendered = render_input_lines(&input, input_inner_width, &Theme::default());
        assert!(rendered.len() > 1);
    }

    #[test]
    fn app_layout_narrow_width_does_not_overlap_columns() {
        let root = Rect::new(0, 0, 96, 28);
        let layout = app_layout(root, 1, "hello");

        assert_eq!(layout.left_main.right(), layout.divider.x);
        assert_eq!(layout.divider.right(), layout.right_main.x);
        assert_eq!(layout.extended_divider.bottom(), layout.footer_border.y);
        assert!(layout.left_main.width > 0);
        assert!(layout.right_main.width > 0);
        assert!(layout.footer_border.y >= layout.left_composer.bottom());
        assert_eq!(layout.footer_border.x, root.x);
        assert_eq!(layout.footer_border.width, root.width);
        assert_eq!(layout.footer_border.bottom(), layout.footer.y);
        assert!(layout.footer_text.x >= layout.footer.x);
        assert!(layout.footer_text.right() <= layout.footer.right());
        assert!(layout.footer_padding_bottom.bottom() <= root.bottom());
        assert_eq!(layout.footer.width, root.width);
    }

    #[test]
    fn content_root_uses_full_terminal_area() {
        let root = content_root(Rect::new(0, 0, 100, 40));

        assert_eq!(root, Rect::new(0, 0, 100, 40));
    }

    #[test]
    fn working_light_line_uses_gradient_spans() {
        let theme = Theme::default();
        let line = working_light_line(12, 0, &theme);
        let colors = line
            .spans
            .iter()
            .filter_map(|span| span.style.fg)
            .collect::<Vec<_>>();

        assert_eq!(line.spans.len(), 12);
        assert!(line.spans.iter().all(|span| span.content.as_ref() == "━"));
        assert!(colors.contains(&theme.purple));
        assert!(colors.iter().any(|color| *color != colors[0]));
        assert!(
            colors
                .iter()
                .any(|color| matches!(color, Color::Rgb(_, _, _)))
        );
    }

    #[test]
    fn working_light_phase_changes_on_reduced_tick() {
        assert_eq!(working_light_phase(0), working_light_phase(119));
        assert_ne!(working_light_phase(0), working_light_phase(120));
    }

    #[test]
    fn app_is_working_when_turn_pending() {
        let mut app = App::new();
        app.pending_turns.insert("message-1".to_string(), 0);

        assert!(app.is_working());
    }

    #[test]
    fn gateway_message_builder_includes_context_fork_and_continuation_metadata() {
        let envelope = gateway_command_builder()
            .gateway_message_send(
                1,
                gateway_message_payload(
                    "message-1",
                    "继续",
                    Some("fork-1"),
                    Some(&json!({
                        "continuation": {
                            "mode": "continue",
                            "snapshotId": "behavior-1"
                        }
                    })),
                    InteractionMode::Plan,
                    true,
                ),
            )
            .into_value();

        assert_eq!(
            envelope
                .get("payload")
                .and_then(|payload| payload.get("context"))
                .and_then(|context| context.get("contextForkId"))
                .and_then(Value::as_str),
            Some("fork-1")
        );
        assert_eq!(
            envelope
                .get("payload")
                .and_then(|payload| payload.get("metadata"))
                .and_then(|metadata| metadata.get("continuation"))
                .and_then(|continuation| continuation.get("snapshotId"))
                .and_then(Value::as_str),
            Some("behavior-1")
        );
        assert_eq!(
            envelope
                .get("payload")
                .and_then(|payload| payload.get("metadata"))
                .and_then(|metadata| metadata.get("interaction"))
                .and_then(|interaction| interaction.get("mode"))
                .and_then(Value::as_str),
            Some("plan")
        );
        assert_eq!(
            envelope
                .get("payload")
                .and_then(|payload| payload.get("metadata"))
                .and_then(|metadata| metadata.get("interaction"))
                .and_then(|interaction| interaction.get("yolo"))
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            envelope
                .get("payload")
                .and_then(|payload| payload.get("metadata"))
                .and_then(|metadata| metadata.get("tui"))
                .and_then(|tui| tui.get("mode"))
                .and_then(Value::as_str),
            Some("plan")
        );
        assert_eq!(
            envelope
                .get("payload")
                .and_then(|payload| payload.get("metadata"))
                .and_then(|metadata| metadata.get("tui"))
                .and_then(|tui| tui.get("yolo"))
                .and_then(Value::as_bool),
            Some(true)
        );
        assert!(
            envelope
                .get("payload")
                .and_then(|payload| payload.get("context"))
                .and_then(|context| context.get("mode"))
                .is_none()
        );
    }

    #[test]
    fn event_subscribe_builder_uses_known_runtime_event_types_only() {
        let envelope = gateway_command_builder().event_subscribe(42).into_value();
        let types = envelope
            .get("payload")
            .and_then(|payload| payload.get("types"))
            .and_then(Value::as_array)
            .expect("types array")
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();

        assert_eq!(types, tui::gateway::subscription::SUBSCRIPTION_EVENT_TYPES);
        assert!(types.contains(&"executive.loop.paused"));
        assert!(types.contains(&"blackboard.message.appended"));
        assert!(types.contains(&"subagent.child.end"));
        assert!(
            !types
                .iter()
                .any(|event_type| event_type.starts_with("fork.memory."))
        );
        assert!(
            types
                .iter()
                .any(|event_type| event_type.starts_with("blackboard."))
        );
        assert!(
            envelope
                .get("payload")
                .and_then(|payload| payload.get("classes"))
                .is_none()
        );
    }

    #[test]
    fn parses_fork_snapshot_into_created_event() {
        let raw = r#"{
            "protocol": "flyflor.ws.v1",
            "id": "env-fork-snapshot-1",
            "type": "fork.snapshot",
            "at": "2026-05-22T00:00:06.500Z",
            "payload": {
                "data": {
                    "fork": {
                        "id": "fork-1",
                        "title": "Implementation fork",
                        "summary": "Keep the implementation context.",
                        "continuitySummary": "Keep socket docs and tests in view.",
                        "maxContextTokens": 12000
                    },
                    "session": {
                        "activeForkId": "fork-1",
                        "parentId": "parent-1",
                        "rootId": "root-1"
                    }
                }
            }
        }"#;

        let event = parse_fork_snapshot(raw)
            .expect("fork snapshot should parse")
            .expect("fork snapshot should emit event");

        match event {
            SocketEvent::ForkCreated {
                fork_id,
                parent_id,
                root_id,
                summary,
            } => {
                assert_eq!(fork_id, "fork-1");
                assert_eq!(parent_id.as_deref(), Some("parent-1"));
                assert_eq!(root_id.as_deref(), Some("root-1"));
                assert_eq!(summary.as_deref(), Some("Keep the implementation context."));
            }
            _ => panic!("expected fork created event"),
        }
    }

    #[test]
    fn parses_fork_snapshot_with_direct_payload_shape() {
        let raw = r#"{
            "protocol": "flyflor.ws.v1",
            "id": "env-fork-snapshot-direct",
            "type": "fork.snapshot",
            "at": "2026-05-22T00:00:06.500Z",
            "payload": {
                "fork": {
                    "id": "fork-direct",
                    "title": "Direct fork"
                },
                "session": {
                    "activeForkId": "fork-direct",
                    "rootId": "root-direct"
                }
            }
        }"#;

        let event = parse_fork_snapshot(raw)
            .expect("fork snapshot should parse")
            .expect("fork snapshot should emit event");

        match event {
            SocketEvent::ForkCreated {
                fork_id,
                root_id,
                summary,
                ..
            } => {
                assert_eq!(fork_id, "fork-direct");
                assert_eq!(root_id.as_deref(), Some("root-direct"));
                assert_eq!(summary.as_deref(), Some("Direct fork"));
            }
            _ => panic!("expected fork created event"),
        }
    }

    #[test]
    fn fork_created_event_updates_active_context_fork_id() {
        let mut app = App::new();
        app.turns.push(test_turn("root", "root answer"));
        app.pending_fork_create = true;

        app.apply_socket_event(SocketEvent::ForkCreated {
            fork_id: "fork-created".to_string(),
            parent_id: Some("parent-created".to_string()),
            root_id: Some("root-created".to_string()),
            summary: Some("created summary".to_string()),
        });

        assert_eq!(app.active_context_fork_id.as_deref(), Some("fork-created"));
        assert_eq!(
            app.active_fork.as_ref().map(|fork| fork.fork_id.as_str()),
            Some("fork-created")
        );
        assert_eq!(
            app.active_fork
                .as_ref()
                .and_then(|fork| fork.parent_fork_id.as_deref()),
            None
        );
        assert_eq!(
            app.active_fork
                .as_ref()
                .and_then(|fork| fork.root_id.as_deref()),
            Some("root-created")
        );
        assert_eq!(app.root_turns.len(), 1);
        assert!(app.turns.is_empty());
        assert!(!app.pending_fork_create);
        assert!(app.right_source.blackboard_status.contains("已进入 fork"));
    }

    #[test]
    fn fork_session_messages_and_history_use_active_fork_id() {
        let message = gateway_command_builder()
            .gateway_message_send(
                1,
                gateway_message_payload(
                    "message-fork",
                    "fork 内继续",
                    Some("fork-active"),
                    None,
                    InteractionMode::Act,
                    false,
                ),
            )
            .into_value();
        assert_eq!(
            message
                .get("payload")
                .and_then(|payload| payload.get("context"))
                .and_then(|context| context.get("contextForkId"))
                .and_then(Value::as_str),
            Some("fork-active")
        );

        let history = gateway_command_builder()
            .history_list_with_before(1, 20, Some("fork-active"), None)
            .into_value();
        assert_eq!(
            history
                .get("payload")
                .and_then(|payload| payload.get("contextForkId"))
                .and_then(Value::as_str),
            Some("fork-active")
        );
        assert!(
            history
                .get("payload")
                .and_then(|payload| payload.get("context"))
                .is_none()
        );
        let root_history = gateway_command_builder()
            .history_list_with_before(1, 20, None, None)
            .into_value();
        assert!(
            root_history
                .get("payload")
                .and_then(|payload| payload.get("contextForkId"))
                .is_none()
        );
    }

    #[test]
    fn exit_fork_command_is_only_available_inside_fork_and_restores_root() {
        let mut app = App::new();
        app.input = "/exit f".to_string();
        app.refresh_command_palette();
        assert!(
            app.command_palette.as_ref().is_none_or(|menu| {
                menu.items.iter().all(|command| command.name != "exit fork")
            })
        );

        app.turns = vec![test_turn("root", "root answer")];
        app.apply_socket_event(SocketEvent::ForkCreated {
            fork_id: "fork-session".to_string(),
            parent_id: None,
            root_id: Some("root-session".to_string()),
            summary: Some("分支会话".to_string()),
        });
        app.turns = vec![test_turn("fork", "fork answer")];
        app.input = "/exit f".to_string();
        app.refresh_command_palette();
        assert_eq!(
            app.command_palette
                .as_ref()
                .and_then(|menu| menu.items.first())
                .map(|command| command.name),
            Some("exit fork")
        );

        app.confirm_command_palette_selection();
        assert!(app.active_fork.is_none());
        assert!(app.active_context_fork_id.is_none());
        assert_eq!(app.turns.len(), 1);
        assert_eq!(app.turns[0].user, "root");
        assert!(!app.should_quit);
    }

    #[test]
    fn root_fork_exit_clears_context_even_when_snapshot_has_parent_id() {
        let mut app = App::new();
        app.turns = vec![test_turn("root", "root answer")];

        app.apply_socket_event(SocketEvent::ForkCreated {
            fork_id: "fork-session".to_string(),
            parent_id: Some("parent-from-server".to_string()),
            root_id: Some("root-session".to_string()),
            summary: Some("分支会话".to_string()),
        });

        app.exit_fork_session();

        assert!(app.active_fork.is_none());
        assert!(app.active_context_fork_id.is_none());
        assert_eq!(app.turns[0].user, "root");
    }

    #[test]
    fn nested_fork_exit_restores_parent_fork_before_root() {
        let mut app = App::new();
        app.turns = vec![test_turn("root", "root answer")];

        app.apply_socket_event(SocketEvent::ForkCreated {
            fork_id: "fork-parent".to_string(),
            parent_id: None,
            root_id: Some("root-session".to_string()),
            summary: Some("父 fork".to_string()),
        });
        app.turns = vec![test_turn("parent", "parent answer")];

        app.apply_socket_event(SocketEvent::ForkCreated {
            fork_id: "fork-child".to_string(),
            parent_id: Some("fork-parent".to_string()),
            root_id: Some("root-session".to_string()),
            summary: Some("子 fork".to_string()),
        });
        app.turns = vec![test_turn("child", "child answer")];

        app.exit_fork_session();

        assert_eq!(app.active_context_fork_id.as_deref(), Some("fork-parent"));
        assert_eq!(
            app.active_fork.as_ref().map(|fork| fork.fork_id.as_str()),
            Some("fork-parent")
        );
        assert_eq!(app.turns.len(), 1);
        assert_eq!(app.turns[0].user, "parent");

        app.exit_fork_session();

        assert!(app.active_fork.is_none());
        assert!(app.active_context_fork_id.is_none());
        assert_eq!(app.turns.len(), 1);
        assert_eq!(app.turns[0].user, "root");
    }

    #[test]
    fn slash_exit_still_quits_tui_at_root() {
        let mut app = App::new();
        app.input = "/exit".to_string();
        app.refresh_command_palette();
        app.confirm_command_palette_selection();

        assert!(app.should_quit);
        assert!(app.active_fork.is_none());
    }

    #[test]
    fn fork_create_command_uses_structured_turn_anchors() {
        let turn = Turn {
            message_id: Some("message-1".to_string()),
            event_id: Some("event-1".to_string()),
            user: "把这条线分出去".to_string(),
            thought: None,
            answer: "可以从这里创建 fork。".to_string(),
            metadata: Some(json!({
                "ask": {
                    "askId": "ask-1",
                    "snapshotId": "behavior-1"
                }
            })),
            context_rows: Vec::new(),
            pending_continuation: None,
            footer: String::new(),
        };

        let command = fork_create_command_from_turn(&turn, &Some("parent-fork".to_string()))
            .expect("command should be created");

        match command {
            SocketCommand::ForkCreate { payload, .. } => {
                assert_eq!(
                    payload.get("sourceEventId").and_then(Value::as_str),
                    Some("event-1")
                );
                assert_eq!(
                    payload.get("sourceAskId").and_then(Value::as_str),
                    Some("ask-1")
                );
                assert_eq!(
                    payload
                        .get("context")
                        .and_then(|context| context.get("contextForkId"))
                        .and_then(Value::as_str),
                    Some("parent-fork")
                );
            }
            _ => panic!("expected fork create command"),
        }
    }

    #[test]
    fn answer_markdown_wraps_to_final_thread_body_width() {
        let theme = Theme::default();
        let width = 32;
        let turn = test_turn("u", "abcdefghijklmnopqrstuvwxyz0123456789");

        let rendered = render_turns(&[turn], width, &theme, 0);

        assert!(rendered.lines.iter().all(|line| line_width(line) <= width));
    }

    #[test]
    fn answer_code_block_wraps_to_final_thread_body_width() {
        let theme = Theme::default();
        let width = 32;
        let turn = test_turn("u", "```text\nabcdefghijklmnopqrstuvwxyz0123456789\n```");

        let rendered = render_turns(&[turn], width, &theme, 0);

        assert!(rendered.lines.iter().all(|line| line_width(line) <= width));
    }

    fn line_width(line: &Line<'_>) -> usize {
        line.spans
            .iter()
            .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
            .sum()
    }

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    fn test_turn(user: &str, answer: &str) -> Turn {
        Turn {
            message_id: None,
            event_id: None,
            user: user.to_string(),
            thought: None,
            answer: answer.to_string(),
            metadata: None,
            context_rows: context_rows_from_metadata(&None),
            pending_continuation: None,
            footer: String::new(),
        }
    }
}

fn load_mock_data() -> MockData {
    if tui::demo_enabled() {
        return demo_mock_data();
    }
    MockData {
        turns: Vec::new(),
        todos: Vec::new(),
        right_panel: RightPanelData {
            thinking_label: "Socket".to_string(),
            blackboard_status: "waiting for flyflor socket".to_string(),
            blackboard_stream: Vec::new(),
            model_stats: vec![
                StatItem {
                    label: "transport".to_string(),
                    value: "websocket".to_string(),
                },
                StatItem {
                    label: "endpoint".to_string(),
                    value: ws_url(),
                },
            ],
            token_stats: Vec::new(),
            context_total: "未收到上下文窗口".to_string(),
            context_percent: "未收到".to_string(),
            context_bar: String::new(),
            context_usage: "暂无数据".to_string(),
            context_ratio: 0.0,
            context_used_tokens: 0,
            context_max_tokens: None,
            context_used: "0".to_string(),
            context_max: "未知".to_string(),
            fork_memory: ForkMemorySnapshot::default(),
            run_timeline: RunTimeline::new(),
            footer: "flyflor-cli".to_string(),
        },
        fork_memory: ForkMemorySnapshot::default(),
    }
}

fn demo_mock_data() -> MockData {
    let metadata = Some(json!({
        "ask": {
            "prompt": "这轮要优先修复制卡顿，还是先做右侧分区？",
            "snapshotId": "ask-demo-1",
            "options": [
                { "label": "先修复制卡顿", "value": "先修复制卡顿" },
                { "label": "先做右侧分区", "value": "先做右侧分区" },
                "两者一起收口"
            ]
        },
        "planning": {
            "contextForks": [
                {
                    "id": "fork-demo-1",
                    "title": "TUI QA 审查 fork",
                    "summary": "保留复制、ASK、blackboard 和上下文窗口调试上下文。",
                    "continuitySummary": "后续消息继续带 active fork。",
                    "maxContextTokens": 12000
                }
            ],
            "replays": [
                {
                    "id": "recall-demo-1",
                    "kind": "recall",
                    "title": "回忆中 摘要",
                    "summary": "上一轮已接入 slash 命令、ASK 菜单和 task/status 查询。"
                },
                {
                    "id": "blackboard-demo-1",
                    "kind": "blackboard",
                    "title": "blackboard 摘要",
                    "summary": "右侧黑板应流式展示关键事件，避免原始 JSON。"
                }
            ],
            "taskPlans": [{
                "planId": "plan-demo-1",
                "steps": [
                    { "title": "展示 QA fixture", "status": "completed" },
                    { "title": "修复复制卡顿", "status": "等待确认" },
                    { "title": "接入 blackboard event", "status": "todo" }
                ]
            }]
        }
    }));
    let mut turns = vec![
        Turn {
            message_id: Some("demo-message-1".to_string()),
            event_id: Some("demo-event-1".to_string()),
            user: "帮我检查 TUI 所有状态是否可见".to_string(),
            thought: Some(ThoughtData {
                summary: "thinking 状态".to_string(),
                duration_ms: Some(420),
                expanded: false,
                content: "正在检查 TODO、Context Window、ASK、blackboard、fork、recall、history。"
                    .to_string(),
            }),
            answer: "已进入 demo QA 模式。左侧展示历史、回忆、fork、blackboard 和 ASK 入口；右侧展示 TODO、模型状态、Context Window 和流式黑板。"
                .to_string(),
            metadata,
            context_rows: Vec::new(),
            pending_continuation: None,
            footer: "flyflor demo · PLAN + YOLO · final".to_string(),
        },
        Turn {
            message_id: Some("demo-message-streaming".to_string()),
            event_id: Some("demo-event-streaming".to_string()),
            user: "模拟一个正在输出的回合".to_string(),
            thought: None,
            answer: "正在生成回答片段，用于检查顶部细光带和 thinking 状态。".to_string(),
            metadata: None,
            context_rows: Vec::new(),
            pending_continuation: None,
            footer: "flyflor · fork 创建中 · streaming".to_string(),
        },
    ];
    for turn in &mut turns {
        turn.context_rows = context_rows_from_metadata(&turn.metadata);
    }

    MockData {
        turns,
        todos: vec![
            TodoItem {
                marker: "✓".to_string(),
                label: "展示 QA fixture".to_string(),
                status: "完成".to_string(),
                active: false,
                plan_id: Some("plan-demo-1".to_string()),
            },
            TodoItem {
                marker: "›".to_string(),
                label: "修复复制卡顿".to_string(),
                status: "等待确认".to_string(),
                active: true,
                plan_id: Some("plan-demo-1".to_string()),
            },
            TodoItem {
                marker: "○".to_string(),
                label: "订阅 blackboard 事件".to_string(),
                status: "待办".to_string(),
                active: false,
                plan_id: Some("plan-demo-1".to_string()),
            },
        ],
        right_panel: RightPanelData {
            thinking_label: "DEMO".to_string(),
            blackboard_status: "demo blackboard · 等待 core 事件时使用模拟流".to_string(),
            blackboard_stream: vec![
                "流式记录：复制路径改为分区 buffer".to_string(),
                "流式记录：Context Window 使用真实 telemetry 优先".to_string(),
                "回合结束：demo turn 已写入 blackboard 摘要".to_string(),
            ],
            model_stats: vec![
                StatItem {
                    label: "model".to_string(),
                    value: "demo-model".to_string(),
                },
                StatItem {
                    label: "provider".to_string(),
                    value: "demo".to_string(),
                },
            ],
            token_stats: vec![StatItem {
                label: "最大输出".to_string(),
                value: "2048".to_string(),
            }],
            context_total: "最大 12k tokens".to_string(),
            context_percent: "28%".to_string(),
            context_bar: context_bar(0.28, DEFAULT_CONTEXT_BAR_WIDTH),
            context_usage: context_usage_line(
                &context_bar(0.28, DEFAULT_CONTEXT_BAR_WIDTH),
                "3.4k",
                "12k",
                "28.00%",
            ),
            context_ratio: 0.28,
            context_used_tokens: 3360,
            context_max_tokens: Some(12000),
            context_used: "3.4k".to_string(),
            context_max: "12k".to_string(),
            fork_memory: ForkMemorySnapshot {
                forks: vec![
                    ForkMemoryItem {
                        summary: "TUI QA 审查 fork".to_string(),
                        time: Some("2026-05-24T00:00:00.000Z".to_string()),
                    },
                    ForkMemoryItem {
                        summary: "右侧布局联调".to_string(),
                        time: Some("2026-05-24T00:05:00.000Z".to_string()),
                    },
                ],
                brain_db_human: Some("12.4 MB".to_string()),
                brain_db_status: Some("available".to_string()),
            },
            run_timeline: RunTimeline::new(),
            footer: "Shift+Tab 切换模式 · ←/→ 分区 · y 复制分区".to_string(),
        },
        fork_memory: ForkMemorySnapshot {
            forks: vec![
                ForkMemoryItem {
                    summary: "TUI QA 审查 fork".to_string(),
                    time: Some("2026-05-24T00:00:00.000Z".to_string()),
                },
                ForkMemoryItem {
                    summary: "右侧布局联调".to_string(),
                    time: Some("2026-05-24T00:05:00.000Z".to_string()),
                },
            ],
            brain_db_human: Some("12.4 MB".to_string()),
            brain_db_status: Some("available".to_string()),
        },
    }
}

#[derive(Clone, Copy, Default)]
struct LayoutSnapshot {
    left_panel: Rect,
    right_panel: Rect,
}

fn dev_mode_enabled() -> bool {
    env::args().any(|arg| arg == "--dev")
        || env::var("FLYFLOR_DEV")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "on" | "ON"))
            .unwrap_or(false)
}
