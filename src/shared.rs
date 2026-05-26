use std::env;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{DEFAULT_WS_URL, Theme};

pub fn ws_url() -> String {
    env::var("FLYFLOR_WS_URL").unwrap_or_else(|_| DEFAULT_WS_URL.to_string())
}

pub fn top_bar_title() -> String {
    top_bar_title_for_url(&ws_url())
}

pub fn top_bar_title_for_url(url: &str) -> String {
    format!("FlyFlor · Powered By {url}")
}

pub fn working_light_phase(now_millis: u64) -> usize {
    ((now_millis / 120) % 48) as usize
}

pub fn working_light_line(width: usize, phase: usize, theme: &Theme) -> Line<'static> {
    let colors = [theme.purple, theme.blue, theme.pink, theme.purple];
    Line::from(
        (0..width)
            .map(|index| {
                Span::styled(
                    "━",
                    Style::default().fg(interpolated_gradient_color(
                        &colors,
                        index + phase,
                        width.max(1),
                    )),
                )
            })
            .collect::<Vec<_>>(),
    )
}

pub fn working_shimmer_style(index: usize, phase: usize, theme: &Theme) -> Style {
    let colors = [theme.pink, theme.purple, theme.blue, theme.purple];
    Style::default().fg(colors[(index + phase) % colors.len()])
}

fn interpolated_gradient_color(colors: &[Color], position: usize, width: usize) -> Color {
    let stops = colors.len().saturating_sub(1).max(1);
    let cycle = width.max(stops * 2);
    let scaled = (position % cycle) as f64 / cycle as f64 * stops as f64;
    let start = scaled.floor() as usize;
    let end = (start + 1).min(colors.len().saturating_sub(1));
    let t = scaled - start as f64;
    interpolate_color(colors[start], colors[end], t)
}

fn interpolate_color(a: Color, b: Color, t: f64) -> Color {
    let (ar, ag, ab) = rgb_components(a);
    let (br, bg, bb) = rgb_components(b);
    Color::Rgb(lerp_u8(ar, br, t), lerp_u8(ag, bg, t), lerp_u8(ab, bb, t))
}

fn rgb_components(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Indexed(value) => (value, value, value),
        Color::Gray => (128, 128, 128),
        Color::Black => (0, 0, 0),
        Color::Red => (255, 0, 0),
        Color::Green => (0, 255, 0),
        Color::Yellow => (255, 255, 0),
        Color::Blue => (0, 0, 255),
        Color::Magenta => (255, 0, 255),
        Color::Cyan => (0, 255, 255),
        Color::White => (255, 255, 255),
        _ => (180, 180, 220),
    }
}

fn lerp_u8(a: u8, b: u8, t: f64) -> u8 {
    (a as f64 + (b as f64 - a as f64) * t.clamp(0.0, 1.0)).round() as u8
}

pub fn metric_line(key: &str, value: &str, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key:<12}"), Style::default().fg(theme.muted)),
        Span::styled(value.to_string(), Style::default().fg(theme.text)),
    ])
}

pub fn draw_separator(frame: &mut Frame, area: Rect, theme: &Theme) {
    if area.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new("─".repeat(area.width as usize)).style(Style::default().fg(theme.dim)),
        area,
    );
}

pub fn in_rect(x: u16, y: u16, area: Rect) -> bool {
    x >= area.x
        && x < area.x.saturating_add(area.width)
        && y >= area.y
        && y < area.y.saturating_add(area.height)
}

pub fn wrap_plain_text(text: &str, width: usize) -> Vec<String> {
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
            let ch_width = ch.width().unwrap_or(0).max(1);
            if current_width + ch_width > width && !current.is_empty() {
                rows.push(std::mem::take(&mut current));
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

pub fn center_text(text: &str, width: usize) -> String {
    let text_width = UnicodeWidthStr::width(text);
    if text_width >= width {
        return text.to_string();
    }
    let left = (width - text_width) / 2;
    let right = width - text_width - left;
    format!("{}{}{}", " ".repeat(left), text, " ".repeat(right))
}

#[allow(dead_code)]
pub fn selection_bg() -> Color {
    Color::Rgb(60, 76, 120)
}
