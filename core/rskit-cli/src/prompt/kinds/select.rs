//! Single-choice selection: a radio list in key-driven mode, a numbered list in
//! line-driven mode.

use rskit_errors::AppResult;

use super::{
    cancelled, closed_input, focus_down, focus_up, non_interactive_error, notice, parse_index,
    with_raw_mode, write_answer,
};
use crate::prompt::choice::{Choice, ChoiceId};
use crate::prompt::key::Key;
use crate::prompt::mode::PromptMode;
use crate::prompt::render::{self, Style};
use crate::prompt::terminal::Terminal;

/// Ask for exactly one choice, dispatching on mode and terminal capability.
pub fn run(
    terminal: &mut (impl Terminal + ?Sized),
    style: Style,
    mode: PromptMode,
    prompt: &str,
    choices: &[Choice],
) -> AppResult<ChoiceId> {
    if choices.is_empty() {
        return Err(rskit_errors::AppError::invalid_input(
            "prompt",
            format!("select requires at least one choice: {prompt}"),
        ));
    }
    let default = choices.iter().position(Choice::is_recommended);

    if !mode.is_interactive() {
        return default.map_or_else(
            || Err(non_interactive_error(prompt)),
            |index| Ok(choices[index].id().clone()),
        );
    }

    if terminal.capabilities().is_key_driven() {
        key_driven(terminal, style, prompt, choices, default)
    } else {
        line_driven(terminal, style, prompt, choices, default)
    }
}

fn key_driven(
    terminal: &mut (impl Terminal + ?Sized),
    style: Style,
    prompt: &str,
    choices: &[Choice],
    default: Option<usize>,
) -> AppResult<ChoiceId> {
    let len = choices.len();
    let mut cursor = default.unwrap_or(0);
    with_raw_mode(terminal, |terminal| {
        let mut drawn = draw(terminal, style, prompt, choices, cursor, default)?;
        loop {
            let key = terminal.read_key()?;
            match key {
                Key::Enter => return Ok(choices[cursor].id().clone()),
                Key::Escape | Key::Interrupt => return Err(cancelled(prompt)),
                Key::Up => cursor = focus_up(cursor, len),
                Key::Down | Key::Tab => cursor = focus_down(cursor, len),
                Key::Home => cursor = 0,
                Key::End => cursor = len - 1,
                _ => continue,
            }
            terminal.clear_last_lines(drawn)?;
            drawn = draw(terminal, style, prompt, choices, cursor, default)?;
        }
    })
}

fn draw(
    terminal: &mut (impl Terminal + ?Sized),
    style: Style,
    prompt: &str,
    choices: &[Choice],
    cursor: usize,
    default: Option<usize>,
) -> AppResult<u16> {
    terminal.write_line(&render::heading(style, prompt))?;
    for row in render::frame_rows(style, choices, cursor, None, default) {
        terminal.write_line(&row)?;
    }
    terminal.write_line(&render::key_hint(style, false))?;
    terminal.flush()?;
    Ok(u16::try_from(choices.len())
        .unwrap_or(u16::MAX)
        .saturating_add(2))
}

fn line_driven(
    terminal: &mut (impl Terminal + ?Sized),
    style: Style,
    prompt: &str,
    choices: &[Choice],
    default: Option<usize>,
) -> AppResult<ChoiceId> {
    terminal.write_line(&render::heading(style, prompt))?;
    for row in render::numbered_rows(style, choices, false, default) {
        terminal.write_line(&row)?;
    }
    loop {
        let hint = default.map(|index| format!("[{}]", index + 1));
        write_answer(terminal, hint.as_deref())?;
        let Some(line) = terminal.read_line()? else {
            return Err(closed_input(prompt));
        };
        let answer = line.trim();
        if answer.is_empty() {
            if let Some(index) = default {
                return Ok(choices[index].id().clone());
            }
            notice(terminal, style, "a choice is required")?;
            continue;
        }
        if let Some(index) = parse_index(answer, choices.len()) {
            return Ok(choices[index].id().clone());
        }
        notice(
            terminal,
            style,
            &format!("enter a number between 1 and {}", choices.len()),
        )?;
    }
}
