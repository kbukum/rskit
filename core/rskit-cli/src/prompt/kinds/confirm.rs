//! Yes/no confirmation with an explicit default.

use rskit_errors::AppResult;

use super::{Ask, cancelled, closed_input, with_raw_mode};
use crate::prompt::key::Key;
use crate::prompt::render::{self};
use crate::prompt::terminal::Terminal;

/// Ask a yes/no question, dispatching on mode and terminal capability.
pub(crate) fn run(
    terminal: &mut (impl Terminal + ?Sized),
    ask: Ask,
    default: bool,
) -> AppResult<bool> {
    if !ask.mode.is_interactive() {
        return Ok(default);
    }
    if terminal.capabilities().is_key_driven() {
        key_driven(terminal, ask, default)
    } else {
        line_driven(terminal, ask, default)
    }
}

const fn suffix(default: bool) -> &'static str {
    if default { "[Y/n]" } else { "[y/N]" }
}

fn key_driven(terminal: &mut (impl Terminal + ?Sized), ask: Ask, default: bool) -> AppResult<bool> {
    with_raw_mode(terminal, |terminal| {
        let line = format!(
            "{} {}",
            render::heading(ask.style, ask.prompt),
            suffix(default)
        );
        terminal.write_line(&line)?;
        terminal.flush()?;
        loop {
            match terminal.read_key()? {
                Key::Enter => return Ok(default),
                Key::Escape | Key::Interrupt => return Err(cancelled(ask.prompt)),
                Key::Char('y' | 'Y') => return Ok(true),
                Key::Char('n' | 'N') => return Ok(false),
                _ => {}
            }
        }
    })
}

fn line_driven(
    terminal: &mut (impl Terminal + ?Sized),
    ask: Ask,
    default: bool,
) -> AppResult<bool> {
    loop {
        terminal.write(&format!(
            "{} {}: ",
            render::heading(ask.style, ask.prompt),
            suffix(default)
        ))?;
        terminal.flush()?;
        let Some(line) = terminal.read_line()? else {
            return Err(closed_input(ask.prompt));
        };
        match line.trim().to_ascii_lowercase().as_str() {
            "" => return Ok(default),
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => {
                let notice = ask
                    .style
                    .palette()
                    .warn("please answer 'y' or 'n'")
                    .into_owned();
                terminal.write_line(&format!("  {notice}"))?;
            }
        }
    }
}
