use std::{
    env, io,
    io::{IsTerminal, Write},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

use arboard::Clipboard;
use base64::Engine as _;

const CLIPBOARD_INIT_TIMEOUT: Duration = Duration::from_millis(500);
pub(crate) const OSC52_MAX_BYTES: usize = 100 * 1024;

fn clipboard_with_timeout() -> Option<Clipboard> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(Clipboard::new().ok());
    });
    rx.recv_timeout(CLIPBOARD_INIT_TIMEOUT).ok().flatten()
}

pub(crate) fn read_clipboard_text() -> Result<String, String> {
    let Some(mut clipboard) = clipboard_with_timeout() else {
        return Err("system clipboard unavailable or timed out".to_string());
    };
    clipboard.get_text().map_err(|error| error.to_string())
}

pub(crate) fn write_text_to_clipboard(text: &str) -> Result<(), String> {
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

pub(crate) fn osc52_sequence(text: &str, in_tmux: bool) -> Result<String, String> {
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
