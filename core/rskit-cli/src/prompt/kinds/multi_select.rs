//! Multi-choice selection: a checkbox list in key-driven mode, a comma-separated
//! numbered list in line-driven mode.

use rskit_errors::AppResult;

use super::{
    cancelled, closed_input, focus_down, focus_up, notice, parse_index, with_raw_mode, write_answer,
};
use crate::prompt::choice::{Choice, ChoiceId};
use crate::prompt::key::Key;
use crate::prompt::mode::PromptMode;
use crate::prompt::render::{self, Style};
use crate::prompt::terminal::Terminal;

/// Ask for zero or more choices, dispatching on mode and terminal capability.
pub fn run(
    terminal: &mut (impl Terminal + ?Sized),
    style: Style,
    mode: PromptMode,
    prompt: &str,
    choices: &[Choice],
) -> AppResult<Vec<ChoiceId>> {
    if choices.is_empty() {
        return Err(rskit_errors::AppError::invalid_input(
            "prompt",
            format!("multi_select requires at least one choice: {prompt}"),
        ));
    }
    let defaults: Vec<usize> = choices
        .iter()
        .enumerate()
        .filter_map(|(index, choice)| choice.is_recommended().then_some(index))
        .collect();

    if !mode.is_interactive() {
        return Ok(ids(choices, &defaults));
    }

    if terminal.capabilities().is_key_driven() {
        key_driven(terminal, style, prompt, choices, defaults)
    } else {
        line_driven(terminal, style, prompt, choices, &defaults)
    }
}

fn ids(choices: &[Choice], indices: &[usize]) -> Vec<ChoiceId> {
    indices.iter().map(|&i| choices[i].id().clone()).collect()
}

fn key_driven(
    terminal: &mut (impl Terminal + ?Sized),
    style: Style,
    prompt: &str,
    choices: &[Choice],
    defaults: Vec<usize>,
) -> AppResult<Vec<ChoiceId>> {
    let len = choices.len();
    let mut cursor = 0;
    let mut selected = defaults;
    selected.sort_unstable();
    with_raw_mode(terminal, |terminal| {
        let mut drawn = draw(terminal, style, prompt, choices, cursor, &selected)?;
        loop {
            let key = terminal.read_key()?;
            match key {
                Key::Enter => return Ok(ids(choices, &selected)),
                Key::Escape | Key::Interrupt => return Err(cancelled(prompt)),
                Key::Up => cursor = focus_up(cursor, len),
                Key::Down | Key::Tab => cursor = focus_down(cursor, len),
                Key::Home => cursor = 0,
                Key::End => cursor = len - 1,
                Key::Space => toggle(&mut selected, cursor),
                _ => continue,
            }
            terminal.clear_last_lines(drawn)?;
            drawn = draw(terminal, style, prompt, choices, cursor, &selected)?;
        }
    })
}

fn toggle(selected: &mut Vec<usize>, index: usize) {
    if let Some(position) = selected.iter().position(|&i| i == index) {
        selected.remove(position);
    } else {
        selected.push(index);
        selected.sort_unstable();
    }
}

fn draw(
    terminal: &mut (impl Terminal + ?Sized),
    style: Style,
    prompt: &str,
    choices: &[Choice],
    cursor: usize,
    selected: &[usize],
) -> AppResult<u16> {
    terminal.write_line(&render::heading(style, prompt))?;
    for row in render::frame_rows(style, choices, cursor, Some(selected), None) {
        terminal.write_line(&row)?;
    }
    terminal.write_line(&render::key_hint(style, true))?;
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
    defaults: &[usize],
) -> AppResult<Vec<ChoiceId>> {
    terminal.write_line(&render::heading(style, prompt))?;
    for row in render::numbered_rows(style, choices, true, None) {
        terminal.write_line(&row)?;
    }
    loop {
        write_answer(terminal, Some(&default_hint(defaults)))?;
        let Some(line) = terminal.read_line()? else {
            return Err(closed_input(prompt));
        };
        let answer = line.trim();
        if answer.is_empty() {
            return Ok(ids(choices, defaults));
        }
        if let Some(indices) = parse_indices(answer, choices.len()) {
            return Ok(ids(choices, &indices));
        }
        notice(
            terminal,
            style,
            &format!(
                "enter comma-separated numbers between 1 and {}",
                choices.len()
            ),
        )?;
    }
}

fn default_hint(defaults: &[usize]) -> String {
    if defaults.is_empty() {
        return "[none]".to_string();
    }
    let list = defaults
        .iter()
        .map(|index| (index + 1).to_string())
        .collect::<Vec<_>>()
        .join(",");
    format!("[{list}]")
}

fn parse_indices(input: &str, len: usize) -> Option<Vec<usize>> {
    let mut indices = Vec::new();
    for token in input.split(',') {
        let index = parse_index(token.trim(), len)?;
        if !indices.contains(&index) {
            indices.push(index);
        }
    }
    Some(indices)
}
