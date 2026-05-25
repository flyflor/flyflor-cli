use std::{env, io};

use crossterm::{
    event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};

#[derive(Clone, Copy)]
pub struct TerminalMode {
    pub use_mouse_capture: bool,
}

pub fn mouse_capture_enabled_from_env_args() -> bool {
    if env::args().any(|arg| arg == "--mouse-capture") {
        return true;
    }
    if env::args().any(|arg| arg == "--no-mouse-capture") {
        return false;
    }
    env::var("FLYFLOR_MOUSE_CAPTURE")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "on" | "ON"))
        .unwrap_or_else(|_| default_mouse_capture_enabled())
}

fn default_mouse_capture_enabled() -> bool {
    if cfg!(windows) {
        let wt_session = env::var("WT_SESSION")
            .ok()
            .filter(|value| !value.is_empty());
        let conemu_pid = env::var("ConEmuPID").ok().filter(|value| !value.is_empty());
        return wt_session.is_some() || conemu_pid.is_some();
    }
    !matches!(
        env::var("TERMINAL_EMULATOR").ok().as_deref(),
        Some(value) if value.eq_ignore_ascii_case("JetBrains-JediTerm")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(windows))]
    fn default_mouse_capture_is_on_for_alternate_screen_terminals() {
        assert!(default_mouse_capture_enabled());
    }
}

pub fn enter_terminal<W: io::Write>(writer: &mut W, mode: TerminalMode) -> io::Result<()> {
    execute!(writer, EnterAlternateScreen)?;
    if mode.use_mouse_capture {
        execute!(writer, EnableMouseCapture)?;
    }
    execute!(writer, EnableBracketedPaste)
}

pub fn leave_terminal<W: io::Write>(writer: &mut W, mode: TerminalMode) -> io::Result<()> {
    execute!(writer, DisableBracketedPaste)?;
    execute!(writer, LeaveAlternateScreen)?;
    if mode.use_mouse_capture {
        execute!(writer, DisableMouseCapture)?;
    }
    Ok(())
}
