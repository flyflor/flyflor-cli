use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Block, BorderType, Borders, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use crate::{Theme, shared::draw_separator};

pub struct ShellLayout {
    pub root: Rect,
    pub header: Rect,
    pub left_main: Rect,
    pub divider: Rect,
    pub extended_divider: Rect,
    pub right_main: Rect,
    pub left_composer: Rect,
    pub right_composer_gap: Rect,
    pub footer_border: Rect,
    pub footer: Rect,
    pub footer_text: Rect,
    pub footer_padding_bottom: Rect,
}

pub fn content_root(area: Rect) -> Rect {
    area
}

pub fn app_layout(root: Rect, header_height: u16, input: &str) -> ShellLayout {
    let footer_border_height = u16::from(root.height > header_height + 2);
    let footer_text_height = u16::from(root.height > header_height + footer_border_height + 1);
    let footer_padding_bottom_height =
        u16::from(root.height > header_height + footer_border_height + footer_text_height + 2);
    let footer_height = footer_text_height + footer_padding_bottom_height;
    let composer_height = composer_height(input, root.width as usize, root.height).min(
        root.height
            .saturating_sub(header_height)
            .saturating_sub(footer_border_height)
            .saturating_sub(footer_height),
    );
    let main_height = root
        .height
        .saturating_sub(header_height)
        .saturating_sub(composer_height)
        .saturating_sub(footer_border_height)
        .saturating_sub(footer_height);
    let header = Rect::new(root.x, root.y, root.width, header_height.min(root.height));
    let main = Rect::new(root.x, header.bottom(), root.width, main_height);
    let composer = Rect::new(root.x, main.bottom(), root.width, composer_height);
    let footer_border = Rect::new(root.x, composer.bottom(), root.width, footer_border_height);
    let footer = Rect::new(root.x, footer_border.bottom(), root.width, footer_height);
    let footer_padding_bottom = if footer_padding_bottom_height == 0 {
        Rect::new(root.x, footer.bottom(), root.width, 0)
    } else {
        Rect::new(
            root.x,
            footer.bottom().saturating_sub(footer_padding_bottom_height),
            root.width,
            footer_padding_bottom_height,
        )
    };
    let footer_text_host = Rect::new(root.x, footer.y, root.width, footer_text_height);
    let footer_text = if footer_text_host.height == 0 {
        footer_text_host
    } else {
        Rect::new(
            footer_text_host.x + u16::from(footer_text_host.width > 2),
            footer_text_host.y,
            footer_text_host
                .width
                .saturating_sub(2 * u16::from(footer_text_host.width > 2)),
            footer_text_host.height,
        )
    };
    let main_cols = split_main_columns(main);
    let composer_cols = split_main_columns(composer);
    let right_composer_gap = Rect::new(
        composer_cols.1.x,
        composer_cols.1.y,
        composer_cols.1.width + composer_cols.2.width,
        composer_cols.1.height,
    );
    ShellLayout {
        root,
        header,
        left_main: main_cols.0,
        divider: main_cols.1,
        extended_divider: Rect::new(
            main_cols.1.x,
            main_cols.1.y,
            main_cols.1.width,
            footer_border.y.saturating_sub(main_cols.1.y),
        ),
        right_main: main_cols.2,
        left_composer: composer_cols.0,
        right_composer_gap,
        footer_border,
        footer,
        footer_text,
        footer_padding_bottom,
    }
}

pub fn draw_shell(frame: &mut Frame, layout: &ShellLayout, theme: &Theme) {
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.bg)),
        layout.root,
    );
    draw_left_shell(frame, layout.left_main, theme);
    draw_vertical_divider(frame, layout.extended_divider, theme);
    draw_separator(frame, layout.footer_border, theme);
}

fn split_main_columns(area: Rect) -> (Rect, Rect, Rect) {
    let right_width = if area.width >= 150 {
        58
    } else if area.width >= 120 {
        46
    } else {
        34
    }
    .min(area.width.saturating_sub(45));
    let divider_width = u16::from(area.width > right_width);
    let left_width = area
        .width
        .saturating_sub(divider_width)
        .saturating_sub(right_width);
    let left = Rect::new(area.x, area.y, left_width, area.height);
    let divider = Rect::new(left.right(), area.y, divider_width, area.height);
    let right = Rect::new(divider.right(), area.y, right_width, area.height);
    (left, divider, right)
}

fn composer_height(input: &str, width: usize, height: u16) -> u16 {
    let content_width = width.saturating_sub(2).max(1);
    let visual_lines = input_visual_line_count(input, content_width);
    let desired = (visual_lines + 1) as u16;
    let max_height = (height / 2).max(2);
    desired.clamp(2, max_height)
}

fn input_visual_line_count(input: &str, content_width: usize) -> usize {
    if input.is_empty() {
        return 1;
    }

    let mut total = 0usize;
    for line in input.split('\n') {
        let width = UnicodeWidthStr::width(line);
        total += width.div_ceil(content_width).max(1);
    }
    if input.ends_with('\n') {
        total += 1;
    }
    total.max(1)
}

fn draw_left_shell(frame: &mut Frame, area: Rect, theme: &Theme) {
    frame.render_widget(
        Block::default()
            .borders(Borders::LEFT)
            .border_type(BorderType::Plain)
            .border_style(Style::default().fg(theme.dim)),
        area,
    );
}

fn draw_vertical_divider(frame: &mut Frame, area: Rect, theme: &Theme) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let line = Paragraph::new(
        (0..area.height)
            .map(|_| Line::styled("│", Style::default().fg(theme.dim)))
            .collect::<Vec<_>>(),
    );
    frame.render_widget(line, area);
}
