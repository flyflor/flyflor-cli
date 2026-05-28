use std::env;

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

use crate::{DEFAULT_WS_URL, i18n::text_key, tui::theme::Theme};

pub const WORKING_SHIMMER_PHASES: usize = 192;

pub fn ws_url() -> String {
    env::var("FLYFLOR_WS_URL").unwrap_or_else(|_| DEFAULT_WS_URL.to_string())
}

pub fn top_bar_title() -> String {
    top_bar_title_for_url(&ws_url())
}

pub fn top_bar_title_for_url(url: &str) -> String {
    format!("{} {url}", text_key("processBar.poweredByPrefix"))
}

pub fn working_light_phase(now_millis: u64) -> usize {
    ((now_millis / 35) % WORKING_SHIMMER_PHASES as u64) as usize
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

pub fn working_shimmer_style(index: usize, phase: usize, _theme: &Theme) -> Style {
    let position =
        ((index * 7 + phase) % WORKING_SHIMMER_PHASES) as f64 / WORKING_SHIMMER_PHASES as f64;
    let wave = ((position * std::f64::consts::TAU).sin() + 1.0) * 0.5;
    let eased = wave * wave * (3.0 - 2.0 * wave);
    Style::default().fg(interpolate_color(
        Color::Rgb(92, 96, 106),
        Color::Rgb(242, 244, 248),
        eased,
    ))
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
