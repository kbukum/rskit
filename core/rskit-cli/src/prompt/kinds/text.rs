//! Freeform text entry with an optional default and optional validation.

use rskit_errors::AppResult;

use super::{Ask, cancelled, closed_input, non_interactive_error, with_raw_mode};
use crate::prompt::key::Key;
use crate::prompt::render::{self, Style};
use crate::prompt::terminal::Terminal;
use crate::prompt::validate::Validator;

/// Ask for freeform text, dispatching on mode and terminal capability.
///
/// When `validator` rejects an answer the reason is shown and the question is re-asked;
/// in non-interactive mode a rejected default is a typed error.
pub(crate) fn run(
    terminal: &mut (impl Terminal + ?Sized),
    ask: Ask,
    default: Option<&str>,
    validator: Option<&dyn Validator>,
) -> AppResult<String> {
    if !ask.mode.is_interactive() {
        let value = default.ok_or_else(|| non_interactive_error(ask.prompt))?;
        if let Some(v) = validator {
            v.validate(value).map_err(|reason| {
                rskit_errors::AppError::invalid_input(
                    "prompt",
                    format!("default for {} is invalid: {reason}", ask.prompt),
                )
            })?;
        }
        return Ok(value.to_string());
    }
    if terminal.capabilities().is_key_driven() {
        key_driven(terminal, ask, default, validator)
    } else {
        line_driven(terminal, ask, default, validator)
    }
}

/// Resolve a raw answer against the default and validator.
///
/// Returns `Ok(None)` when the answer is empty and there is no default (a value is required),
/// `Ok(Some(value))` when accepted, or `Err(reason)` when the validator rejects it.
fn accept(
    value: &str,
    default: Option<&str>,
    validator: Option<&dyn Validator>,
) -> Result<Option<String>, String> {
    let resolved = if value.is_empty() {
        default
    } else {
        Some(value)
    };
    let Some(resolved) = resolved else {
        return Ok(None);
    };
    if let Some(v) = validator {
        v.validate(resolved)?;
    }
    Ok(Some(resolved.to_string()))
}

fn key_driven(
    terminal: &mut (impl Terminal + ?Sized),
    ask: Ask,
    default: Option<&str>,
    validator: Option<&dyn Validator>,
) -> AppResult<String> {
    terminal.write_line(&heading(ask.style, ask.prompt, default))?;
    with_raw_mode(terminal, |terminal| {
        let mut buffer = String::new();
        let mut error: Option<String> = None;
        let mut drawn = draw(terminal, ask.style, &buffer, error.as_deref())?;
        loop {
            match terminal.read_key()? {
                Key::Enter => match accept(buffer.trim(), default, validator) {
                    Ok(Some(value)) => return Ok(value),
                    Ok(None) => error = Some("a value is required".to_string()),
                    Err(reason) => error = Some(reason),
                },
                Key::Escape | Key::Interrupt => return Err(cancelled(ask.prompt)),
                Key::Backspace => {
                    buffer.pop();
                    error = None;
                }
                Key::Space => {
                    buffer.push(' ');
                    error = None;
                }
                Key::Char(c) => {
                    buffer.push(c);
                    error = None;
                }
                _ => continue,
            }
            terminal.clear_last_lines(drawn)?;
            drawn = draw(terminal, ask.style, &buffer, error.as_deref())?;
        }
    })
}

fn draw(
    terminal: &mut (impl Terminal + ?Sized),
    style: Style,
    buffer: &str,
    error: Option<&str>,
) -> AppResult<u16> {
    terminal.write_line(&format!(
        "  {} {buffer}",
        style.palette().dim(style.glyphs().answer())
    ))?;
    let mut lines = 1;
    if let Some(message) = error {
        terminal.write_line(&format!("  {}", style.palette().warn(message)))?;
        lines += 1;
    }
    terminal.flush()?;
    Ok(lines)
}

fn line_driven(
    terminal: &mut (impl Terminal + ?Sized),
    ask: Ask,
    default: Option<&str>,
    validator: Option<&dyn Validator>,
) -> AppResult<String> {
    loop {
        terminal.write(&format!("{}: ", heading(ask.style, ask.prompt, default)))?;
        terminal.flush()?;
        let Some(line) = terminal.read_line()? else {
            return Err(closed_input(ask.prompt));
        };
        match accept(line.trim(), default, validator) {
            Ok(Some(value)) => return Ok(value),
            Ok(None) => {
                let notice = ask.style.palette().warn("a value is required").into_owned();
                terminal.write_line(&format!("  {notice}"))?;
            }
            Err(reason) => {
                let notice = ask.style.palette().warn(&reason).into_owned();
                terminal.write_line(&format!("  {notice}"))?;
            }
        }
    }
}

fn heading(style: Style, prompt: &str, default: Option<&str>) -> String {
    let base = render::heading(style, prompt);
    match default {
        Some(value) => format!("{base} [{value}]"),
        None => base,
    }
}
