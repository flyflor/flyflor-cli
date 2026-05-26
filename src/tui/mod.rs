use std::env;

pub mod content;
pub mod gateway;
pub mod layout;
pub mod service;
pub mod state;
pub mod terminal;
pub mod ws;

pub const DEMO_ENV: &str = "FLYFLOR_DEMO";

pub fn demo_enabled() -> bool {
    env::args().any(|arg| arg == "--demo")
        || env::var(DEMO_ENV)
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "on" | "ON"))
            .unwrap_or(false)
}
