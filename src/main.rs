mod app;
mod context;
mod layout;
mod state;

use std::{env, io, time::Duration};

use app::App;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
        PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    DefaultTerminal,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use serde::Deserialize;
use state::AppState;
use unicode_width::UnicodeWidthStr;

fn main() -> io::Result<()> {
    let mouse_capture = mouse_capture_enabled();
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    push_keyboard_enhancement_flags(&mut stdout);
    if mouse_capture {
        execute!(stdout, EnableMouseCapture)?;
    }
    let terminal = ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(stdout))?;
    let result = run(terminal, mouse_capture);
    disable_raw_mode()?;
    pop_keyboard_enhancement_flags(&mut io::stdout());
    if mouse_capture {
        execute!(io::stdout(), DisableMouseCapture)?;
    }
    execute!(io::stdout(), LeaveAlternateScreen)?;
    result
}

fn run(mut terminal: DefaultTerminal, mouse_capture_enabled_by_default: bool) -> io::Result<()> {
    let mock = load_mock_data();
    let state = AppState::new(
        mock.turns,
        mock.right_panel,
        mock.todos,
        dev_mode_enabled(),
        !mouse_capture_enabled_by_default,
    );
    let mut app = App::new(state);
    let mut mouse_capture_enabled = mouse_capture_enabled_by_default;

    loop {
        if app.state.native_selection_mode == mouse_capture_enabled {
            if app.state.native_selection_mode {
                execute!(io::stdout(), DisableMouseCapture)?;
                mouse_capture_enabled = false;
            } else {
                execute!(io::stdout(), EnableMouseCapture)?;
                mouse_capture_enabled = true;
            }
        }

        terminal.draw(|frame| app.draw(frame))?;
        if let Some(cursor) = app.state.cursor {
            terminal.show_cursor()?;
            terminal.set_cursor_position(cursor)?;
        } else {
            terminal.hide_cursor()?;
        }
        if !event::poll(Duration::from_millis(50))? {
            continue;
        }

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => app.handle_key(key),
            Event::Mouse(mouse) => app.handle_mouse(mouse),
            _ => {}
        }

        if app.state.should_quit {
            return Ok(());
        }
    }
}

pub fn draw_scrollbar(frame: &mut ratatui::Frame, scrollbar: ScrollbarGeometry, theme: &Theme) {
    if scrollbar.track_height == 0 {
        return;
    }
    for offset in 0..scrollbar.track_height {
        let y = scrollbar.track_top + offset;
        let symbol = if y == scrollbar.thumb_top { "●" } else { "○" };
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

pub fn metric_line<'a>(key: &'a str, value: &'a str, theme: &Theme) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{key:<12}"), Style::default().fg(theme.muted)),
        Span::styled(value.to_string(), Style::default().fg(theme.text)),
    ])
}

pub fn center_text(text: &str, width: usize) -> String {
    let text_width = UnicodeWidthStr::width(text);
    if width <= text_width {
        return text.to_string();
    }
    let left = (width - text_width) / 2;
    let right = width - text_width - left;
    format!("{}{}{}", " ".repeat(left), text, " ".repeat(right))
}

pub fn in_rect(x: u16, y: u16, rect: Rect) -> bool {
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}

#[derive(Clone, Copy, Default)]
pub struct ScrollbarGeometry {
    pub x: u16,
    pub track_top: u16,
    pub track_height: u16,
    pub thumb_top: u16,
    pub thumb_height: u16,
    pub hit_area: Rect,
}

impl ScrollbarGeometry {
    pub fn contains(&self, x: u16, y: u16) -> bool {
        in_rect(x, y, self.hit_area)
    }
}

#[derive(Default, Clone)]
pub struct ScrollState {
    pub scroll: usize,
    pub viewport_height: usize,
    pub wrap_width: usize,
    pub total_visual_lines: usize,
    pub max_scroll: usize,
    pub initial_scroll_applied: bool,
    pub stick_to_bottom: bool,
    pub scrollbar: ScrollbarGeometry,
}

#[derive(Clone, Copy)]
pub struct ThoughtRegion {
    pub turn_index: usize,
    pub line_index: usize,
}

pub struct LeftRender {
    pub lines: Vec<Line<'static>>,
    pub thought_regions: Vec<ThoughtRegion>,
    pub block_regions: Vec<BlockRegion>,
}

#[derive(Clone, Copy)]
pub struct ThoughtHitbox {
    pub turn_index: usize,
    pub rect: Rect,
}

#[derive(Clone, Copy)]
pub enum BlockKind {
    User,
    Thought,
    Answer,
    Footer,
}

#[derive(Clone, Copy)]
pub struct BlockRegion {
    pub turn_index: usize,
    pub kind: BlockKind,
    pub start_line: usize,
    pub end_line: usize,
}

pub fn update_scroll_state_from_rendered(lines: &[Line<'_>], state: &mut ScrollState, area: Rect) {
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

pub fn update_scroll_state(lines: &[Line<'_>], state: &mut ScrollState, area: Rect) {
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

pub fn apply_scroll_delta(state: &mut ScrollState, delta: isize) {
    let next = if delta.is_negative() {
        state.scroll.saturating_sub(delta.unsigned_abs())
    } else {
        (state.scroll + delta as usize).min(state.max_scroll)
    };
    state.scroll = next;
    state.stick_to_bottom = state.scroll >= state.max_scroll;
}

pub fn drag_scroll(state: &mut ScrollState, anchor_scroll: usize, delta_rows: isize) {
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

pub fn compute_scrollbar(area: Rect, scroll: usize, max_scroll: usize) -> ScrollbarGeometry {
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

pub fn count_visual_lines(lines: &[Line<'_>], width: usize) -> usize {
    lines
        .iter()
        .map(|line| wrapped_line_count(line, width))
        .sum::<usize>()
        .max(1)
}

pub fn wrapped_line_count(line: &Line<'_>, width: usize) -> usize {
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

pub fn render_turns(turns: &[Turn], width: usize, theme: &Theme) -> LeftRender {
    let mut lines = Vec::new();
    let mut thought_regions = Vec::new();
    let mut block_regions = Vec::new();

    for (turn_index, turn) in turns.iter().enumerate() {
        if turn_index > 0 {
            lines.push(empty_content_line(width, theme));
        }

        let user_start = lines.len();
        lines.extend(render_user_block(&turn.user, width, theme));
        let user_end = lines.len().saturating_sub(1);
        if user_end >= user_start {
            block_regions.push(BlockRegion {
                turn_index,
                kind: BlockKind::User,
                start_line: user_start,
                end_line: user_end,
            });
        }
        lines.push(thread_line(Line::raw(""), width, theme, ThreadTone::Rail));

        if let Some(thought) = &turn.thought {
            let line_index = lines.len();
            lines.push(render_thought_header(thought, width, theme));
            thought_regions.push(ThoughtRegion {
                turn_index,
                line_index,
            });
            if thought.expanded {
                let thought_start = lines.len().saturating_sub(1);
                lines.push(thread_line(Line::raw(""), width, theme, ThreadTone::Rail));
                for line in render_markdown_block(
                    &thought.content,
                    width.saturating_sub(theme.thread_gutter),
                    theme,
                    MarkdownTone::Thought,
                ) {
                    lines.push(thread_line(line, width, theme, ThreadTone::Thought));
                }
                let thought_end = lines.len().saturating_sub(1);
                if thought_end >= thought_start {
                    block_regions.push(BlockRegion {
                        turn_index,
                        kind: BlockKind::Thought,
                        start_line: thought_start,
                        end_line: thought_end,
                    });
                }
            } else {
                block_regions.push(BlockRegion {
                    turn_index,
                    kind: BlockKind::Thought,
                    start_line: line_index,
                    end_line: line_index,
                });
            }
        }

        lines.push(thread_line(Line::raw(""), width, theme, ThreadTone::Rail));
        let answer_start = lines.len();
        for line in render_markdown_block(
            &turn.answer,
            width.saturating_sub(theme.thread_gutter),
            theme,
            MarkdownTone::Answer,
        ) {
            lines.push(thread_line(line, width, theme, ThreadTone::Rail));
        }
        let answer_end = lines.len().saturating_sub(1);
        if answer_end >= answer_start {
            block_regions.push(BlockRegion {
                turn_index,
                kind: BlockKind::Answer,
                start_line: answer_start,
                end_line: answer_end,
            });
        }
        if !turn.footer.trim().is_empty() {
            let footer_line = lines.len();
            lines.push(render_footer_line(&turn.footer, width, theme));
            block_regions.push(BlockRegion {
                turn_index,
                kind: BlockKind::Footer,
                start_line: footer_line,
                end_line: footer_line,
            });
        }
        lines.push(thread_line(Line::raw(""), width, theme, ThreadTone::Rail));
    }

    LeftRender {
        lines,
        thought_regions,
        block_regions,
    }
}

pub fn render_user_block(text: &str, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let outer_pad = theme.thread_gutter.saturating_sub(2);
    let leading_gap = " ".repeat(outer_pad.saturating_sub(1));
    let gutter = " ";
    let bubble_width = width
        .saturating_sub(outer_pad + theme.user_right_gap)
        .max(theme.user_pad * 2 + 1);
    let mut lines = Vec::new();

    let empty_row = " ".repeat(bubble_width);
    lines.push(Line::from(vec![
        Span::raw(leading_gap.clone()),
        Span::styled(gutter, Style::default().bg(theme.user_bubble)),
        Span::styled(empty_row.clone(), Style::default().bg(theme.user_bg)),
    ]));

    for row in wrap_plain_text(text, bubble_width.saturating_sub(theme.user_pad * 2)) {
        let content = pad_to_width(
            &format!(
                "{}{}{}",
                " ".repeat(theme.user_pad),
                row,
                " ".repeat(theme.user_pad)
            ),
            bubble_width,
        );
        lines.push(Line::from(vec![
            Span::raw(leading_gap.clone()),
            Span::styled(gutter, Style::default().bg(theme.user_bubble)),
            Span::styled(content, Style::default().bg(theme.user_bg).fg(theme.text)),
        ]));
    }

    lines.push(Line::from(vec![
        Span::raw(leading_gap),
        Span::styled(gutter, Style::default().bg(theme.user_bubble)),
        Span::styled(empty_row, Style::default().bg(theme.user_bg)),
    ]));
    lines
}

pub fn render_thought_header(thought: &ThoughtData, width: usize, theme: &Theme) -> Line<'static> {
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

pub fn thread_line(
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

pub fn empty_content_line(width: usize, theme: &Theme) -> Line<'static> {
    thread_line(Line::raw(""), width, theme, ThreadTone::Rail)
}

pub fn render_footer_line(footer: &str, width: usize, theme: &Theme) -> Line<'static> {
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
    Line::from(spans)
}

#[derive(Clone, Copy)]
pub enum MarkdownTone {
    Thought,
    Answer,
}

#[derive(Clone, Copy)]
pub enum ThreadTone {
    Rail,
    Thought,
}

pub fn render_markdown_block(
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

pub fn heading_style(theme: &Theme, tone: MarkdownTone, level: usize) -> Style {
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

pub fn body_style(theme: &Theme, tone: MarkdownTone) -> Style {
    match tone {
        MarkdownTone::Thought => Style::default().fg(theme.muted),
        MarkdownTone::Answer => Style::default().fg(theme.text),
    }
}

pub fn quote_style(theme: &Theme, tone: MarkdownTone) -> Style {
    match tone {
        MarkdownTone::Thought => Style::default().fg(theme.purple),
        MarkdownTone::Answer => Style::default().fg(theme.blue),
    }
}

pub fn code_style(theme: &Theme, tone: MarkdownTone) -> Style {
    match tone {
        MarkdownTone::Thought => Style::default().fg(theme.purple).bg(theme.code_bg),
        MarkdownTone::Answer => Style::default().fg(theme.text).bg(theme.code_bg),
    }
}

pub fn render_code_block(
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
    lines.push(Line::styled(
        truncate_to_width(&label, width),
        Style::default()
            .fg(theme.code_label)
            .add_modifier(Modifier::BOLD),
    ));
    for row in code_lines {
        for wrapped in wrap_plain_text(row, width) {
            lines.push(Line::styled(
                pad_to_width(&wrapped, width),
                code_style(theme, tone),
            ));
        }
    }
    lines
}

pub fn render_mermaid_block(
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

pub fn render_table_block(
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

    let rows: Vec<Vec<String>> = table_lines.iter().skip(2).map(|line| split_table_row(line)).collect();
    let column_count = header.len();
    let mut widths = vec![0usize; column_count];

    for (index, cell) in header.iter().enumerate() {
        widths[index] = widths[index].max(UnicodeWidthStr::width(cell.as_str()));
    }
    for row in &rows {
        for (index, cell) in row.iter().enumerate().take(column_count) {
            widths[index] = widths[index].max(UnicodeWidthStr::width(cell.as_str()));
        }
    }

    let separator_width = widths.iter().sum::<usize>() + column_count.saturating_mul(3) + 1;
    if separator_width > width {
        return table_lines
            .iter()
            .flat_map(|line| wrap_inline_text(line, "", "", width, body_style(theme, tone), theme))
            .collect();
    }

    let mut output = Vec::new();
    output.push(Line::styled(
        format_table_row(&header, &widths),
        heading_style(theme, tone, 3),
    ));
    output.push(Line::styled(
        format_table_separator(&widths),
        Style::default().fg(theme.dim),
    ));
    for row in rows {
        output.push(Line::styled(
            format_table_row(&row, &widths),
            body_style(theme, tone),
        ));
    }
    output
}

fn format_table_row(row: &[String], widths: &[usize]) -> String {
    let mut cells = Vec::new();
    for (index, width) in widths.iter().enumerate() {
        let cell = row.get(index).map(String::as_str).unwrap_or("");
        cells.push(format!(" {:width$} ", cell, width = *width));
    }
    format!("|{}|", cells.join("|"))
}

fn format_table_separator(widths: &[usize]) -> String {
    let mut cells = Vec::new();
    for width in widths {
        cells.push(format!(" {} ", "─".repeat(*width)));
    }
    format!("|{}|", cells.join("|"))
}

pub fn split_table_row(row: &str) -> Vec<String> {
    row.trim_matches('|')
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect()
}

fn is_alignment_row(cells: &[String]) -> bool {
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let trimmed = cell.trim();
            !trimmed.is_empty() && trimmed.chars().all(|ch| ch == '-' || ch == ':' || ch == ' ')
        })
}

fn is_table_start(lines: &[&str], index: usize) -> bool {
    if index + 1 >= lines.len() {
        return false;
    }
    lines[index].contains('|') && lines[index + 1].contains('|')
}

pub fn ordered_prefix(line: &str) -> Option<(String, &str)> {
    let digits = line.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    let rest = &line[digits..];
    let rest = rest.strip_prefix(". ")?;
    Some((format!("{}.", &line[..digits]), rest))
}

fn prefix_width(prefix: &str) -> usize {
    UnicodeWidthStr::width(prefix) + 1
}

fn wrap_inline_text(
    text: &str,
    first_prefix: &str,
    rest_prefix: &str,
    width: usize,
    style: Style,
    _theme: &Theme,
) -> Vec<Line<'static>> {
    let mut output = Vec::new();
    let available_first = width.saturating_sub(UnicodeWidthStr::width(first_prefix));
    let available_rest = width.saturating_sub(UnicodeWidthStr::width(rest_prefix));
    let wrapped = wrap_plain_text(text, available_first.max(1));

    for (index, line) in wrapped.into_iter().enumerate() {
        let prefix = if index == 0 { first_prefix } else { rest_prefix };
        let available = if index == 0 { available_first } else { available_rest }.max(1);
        output.push(Line::from(vec![
            Span::raw(prefix.to_string()),
            Span::styled(pad_to_width(&line, available), style),
        ]));
    }
    if output.is_empty() {
        output.push(Line::from(vec![
            Span::raw(first_prefix.to_string()),
            Span::styled(" ".repeat(available_first.max(1)), style),
        ]));
    }
    output
}

fn is_hr(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && trimmed.chars().all(|ch| matches!(ch, '-' | '*' | '_'))
}

#[derive(Default)]
pub struct MermaidGraph {
    edges: Vec<MermaidEdge>,
    labels: std::collections::HashMap<String, String>,
}

pub struct MermaidEdge {
    from: String,
    to: String,
    label: String,
}

pub fn render_mermaid_ascii(graph: &MermaidGraph, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for edge in &graph.edges {
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
        let mut line = format!("[{from}] -> [{to}]");
        if !edge.label.is_empty() {
            line = format!("[{from}] -{}-> [{to}]", edge.label);
        }
        lines.push(truncate_to_width(&line, width));
    }
    lines
}

pub fn parse_mermaid_graph(code_lines: &[String]) -> MermaidGraph {
    let mut graph = MermaidGraph::default();
    for line in code_lines {
        if let Some((from, label, to)) = parse_mermaid_edge(line) {
            graph.labels.entry(from.0.clone()).or_insert(from.1);
            graph.labels.entry(to.0.clone()).or_insert(to.1);
            graph.edges.push(MermaidEdge {
                from: from.0,
                to: to.0,
                label,
            });
        }
    }
    graph
}

type MermaidNode = (String, String);

pub fn parse_mermaid_edge(line: &str) -> Option<(MermaidNode, String, MermaidNode)> {
    let trimmed = line.trim();
    let arrow_index = trimmed.find("-->")?;
    let left = trimmed[..arrow_index].trim();
    let right = trimmed[arrow_index + 3..].trim();
    let from = parse_mermaid_node(left)?;
    let to = parse_mermaid_node(right)?;
    Some((from, String::new(), to))
}

fn parse_mermaid_node(raw: &str) -> Option<MermaidNode> {
    let trimmed = raw.trim();
    if let Some(start) = trimmed.find('[') {
        let end = trimmed.rfind(']')?;
        let id = trimmed[..start].trim();
        let label = trimmed[start + 1..end].trim();
        Some((id.to_string(), label.to_string()))
    } else {
        Some((trimmed.to_string(), trimmed.to_string()))
    }
}

pub fn wrap_plain_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }

    let mut rows = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    for word in text.split_whitespace() {
        let word_width = UnicodeWidthStr::width(word);
        let spacer = if current.is_empty() { 0 } else { 1 };
        if !current.is_empty() && current_width + spacer + word_width > width {
            rows.push(current);
            current = String::new();
            current_width = 0;
        }

        if word_width > width {
            if !current.is_empty() {
                rows.push(current);
                current = String::new();
                current_width = 0;
            }
            rows.extend(break_word(word, width));
            continue;
        }

        if spacer == 1 {
            current.push(' ');
            current_width += 1;
        }
        current.push_str(word);
        current_width += word_width;
    }

    if !current.is_empty() {
        rows.push(current);
    }
    if rows.is_empty() {
        rows.push(String::new());
    }
    rows
}

fn break_word(word: &str, width: usize) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut used = 0usize;

    for ch in word.chars() {
        let ch_width = string_width_char(ch);
        if used + ch_width > width && !current.is_empty() {
            parts.push(current);
            current = String::new();
            used = 0;
        }
        current.push(ch);
        used += ch_width;
    }
    if !current.is_empty() {
        parts.push(current);
    }
    if parts.is_empty() {
        parts.push(String::new());
    }
    parts
}

pub fn pad_to_width(text: &str, width: usize) -> String {
    let current = UnicodeWidthStr::width(text);
    if current >= width {
        text.to_string()
    } else {
        format!("{text}{}", " ".repeat(width - current))
    }
}

pub fn truncate_to_width(text: &str, width: usize) -> String {
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

fn string_width_char(ch: char) -> usize {
    UnicodeWidthStr::width(ch.encode_utf8(&mut [0; 4]))
}

pub fn build_runtime_turn(index: usize, user_text: String) -> Turn {
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
        footer: format!(
            "Sisyphus - Ultraworker · DeepSeek V4 Flash · {}.{}s",
            duration / 1000,
            (duration % 1000) / 100
        ),
    }
}

pub struct Theme {
    pub bg: Color,
    pub text: Color,
    pub muted: Color,
    pub dim: Color,
    pub blue: Color,
    pub purple: Color,
    pub pink: Color,
    pub green: Color,
    pub user_bubble: Color,
    pub dev: Color,
    pub overlay: Color,
    pub scroll_thumb: Color,
    pub scroll_track: Color,
    pub status_active_bg: Color,
    pub status_idle_bg: Color,
    pub user_bg: Color,
    pub code_bg: Color,
    pub rail: Color,
    pub thought_bar: Color,
    pub thought_text: Color,
    pub footer_icon_color: Color,
    pub footer_primary: Color,
    pub footer_muted: Color,
    pub code_label: Color,
    pub mermaid_label: Color,
    pub mermaid_text: Color,
    pub rail_char: char,
    pub thought_bar_char: char,
    pub footer_icon: char,
    pub thread_gutter: usize,
    pub user_pad: usize,
    pub user_right_gap: usize,
}

#[derive(Clone, Deserialize)]
pub struct ThoughtData {
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub expanded: bool,
    #[serde(default)]
    pub content: String,
}

#[derive(Clone, Deserialize)]
pub struct Turn {
    pub user: String,
    #[serde(default)]
    pub thought: Option<ThoughtData>,
    pub answer: String,
    #[serde(default)]
    pub footer: String,
}

#[derive(Clone, Deserialize)]
pub struct TodoItem {
    pub marker: String,
    pub label: String,
    pub status: String,
    pub active: bool,
}

#[derive(Clone, Deserialize)]
pub struct StatItem {
    pub label: String,
    pub value: String,
}

#[derive(Clone, Deserialize)]
pub struct RightPanelData {
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
            user_bubble: Color::Rgb(126, 160, 255),
            dev: Color::Rgb(96, 165, 250),
            overlay: Color::Rgb(10, 14, 28),
            scroll_thumb: Color::Rgb(218, 220, 228),
            scroll_track: Color::Rgb(107, 116, 144),
            status_active_bg: Color::Rgb(42, 38, 84),
            status_idle_bg: Color::Rgb(28, 34, 55),
            user_bg: Color::Rgb(24, 24, 24),
            code_bg: Color::Rgb(18, 20, 24),
            rail: Color::Rgb(150, 150, 156),
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
            footer_icon: '◻',
            thread_gutter: 3,
            user_pad: 1,
            user_right_gap: 3,
        }
    }
}

fn load_mock_data() -> MockData {
    serde_json::from_str(include_str!("../mock-data.json")).expect("invalid mock-data.json")
}

fn dev_mode_enabled() -> bool {
    env::args().any(|arg| arg == "--dev")
        || env::var("FLYFLOR_DEV")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "on" | "ON"))
            .unwrap_or(false)
}

fn mouse_capture_enabled() -> bool {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|arg| arg == "--mouse-capture") {
        return true;
    }
    if args.iter().any(|arg| arg == "--no-mouse-capture") {
        return false;
    }
    match env::var("FLYFLOR_MOUSE_CAPTURE") {
        Ok(value) if matches!(value.as_str(), "1" | "true" | "TRUE" | "on" | "ON") => true,
        Ok(value) if matches!(value.as_str(), "0" | "false" | "FALSE" | "off" | "OFF") => false,
        _ => true,
    }
}

fn push_keyboard_enhancement_flags<W: io::Write>(writer: &mut W) {
    #[cfg(windows)]
    {
        let _ = write!(writer, "\x1b[>0u");
        let _ = writer.flush();
    }
    #[cfg(not(windows))]
    {
        let _ = execute!(
            writer,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        );
    }
}

fn pop_keyboard_enhancement_flags<W: io::Write>(writer: &mut W) {
    #[cfg(windows)]
    {
        let _ = write!(writer, "\x1b[<1u");
        let _ = writer.flush();
    }
    #[cfg(not(windows))]
    {
        let _ = execute!(writer, PopKeyboardEnhancementFlags);
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
