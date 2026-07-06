//! Yes/no confirmation with an explicit default.

use rskit_errors::AppResult;

use super::{cancelled, closed_input, with_raw_mode};
use crate::prompt::key::Key;
use crate::prompt::mode::PromptMode;
use crate::prompt::render::{self, Style};
use crate::prompt::terminal::Terminal;

/// Ask a yes/no question, dispatching on mode and terminal capability.
pub fn run(
    terminal: &mut (impl Terminal + ?Sized),
    style: Style,
    mode: PromptMode,
    prompt: &str,
    default: bool,
) -> AppResult<bool> {
    if !mode.is_interactive() {
        return Ok(default);
    }
    if terminal.capabilities().is_key_driven() {
        key_driven(terminal, style, prompt, default)
    } else {
        line_driven(terminal, style, prompt, default)
    }
}

const fn suffix(default: bool) -> &'static str {
    if default { "[Y/n]" } else { "[y/N]" }
}

fn key_driven(
    terminal: &mut (impl Terminal + ?Sized),
    style: Style,
    prompt: &str,
    default: bool,
) -> AppResult<bool> {
    with_raw_mode(terminal, |terminal| {
        let line = format!("{} {}", render::heading(style, prompt), suffix(default));
        terminal.write_line(&line)?;
        terminal.flush()?;
        loop {
            match terminal.read_key()? {
                Key::Enter => return Ok(default),
                Key::Escape | Key::Interrupt => return Err(cancelled(prompt)),
                Key::Char('y' | 'Y') => return Ok(true),
                Key::Char('n' | 'N') => return Ok(false),
                _ => {}
            }
        }
    })
}

fn line_driven(
    terminal: &mut (impl Terminal + ?Sized),
    style: Style,
    prompt: &str,
    default: bool,
) -> AppResult<bool> {
    loop {
        terminal.write(&format!(
            "{} {}: ",
            render::heading(style, prompt),
            suffix(default)
        ))?;
        terminal.flush()?;
        let Some(line) = terminal.read_line()? else {
            return Err(closed_input(prompt));
        };
        match line.trim().to_ascii_lowercase().as_str() {
            "" => return Ok(default),
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => {
                let notice = style
                    .palette()
                    .warn("please answer 'y' or 'n'")
                    .into_owned();
                terminal.write_line(&format!("  {notice}"))?;
            }
        }
    }
}
