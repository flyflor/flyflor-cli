use std::env;

pub mod ask;
pub mod clipboard;
pub mod confirm;
pub mod context;
pub mod execution;
pub mod fork;
pub mod gateway;
pub mod input;
pub mod layout;
pub mod plan;
pub mod run_timeline;
pub mod shared;
pub mod subagent;
pub mod terminal;
pub mod theme;

pub const DEMO_ENV: &str = "FLYFLOR_DEMO";

pub fn demo_enabled() -> bool {
    env::args().any(|arg| arg == "--demo")
        || env::var(DEMO_ENV)
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "on" | "ON"))
            .unwrap_or(false)
}
