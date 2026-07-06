# rskit-cli — CLI Framework

Parser-agnostic CLI UX toolkit: theming, structured output, progress, interactive prompts, and signal handling.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/rskit-cli.svg)](https://crates.io/crates/rskit-cli) [![docs.rs](https://docs.rs/rskit-cli/badge.svg)](https://docs.rs/rskit-cli) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kbukum/rskit/blob/main/LICENSE) [![MSRV: 1.91](https://img.shields.io/badge/MSRV-1.91-orange.svg)](https://www.rust-lang.org/)

## Modules

- `theme` — visual vocabulary: `Palette` semantic color and `Glyphs` status symbols (✓ ✗ ⚠ ℹ • → …), both honouring `NO_COLOR`, TTY detection, and UTF-8 capability (with a pure-ASCII glyph fallback).
- `render` — structured, non-interactive display: `OutputTable`, `OutputKV`, the `ErrorRenderer`/`ExitCode` convention, and one-off `StatusReporter` feedback lines (success/warn/step/heading) for guided flows.
- `progress` — `ProgressBar` / `MultiProgress` preset styles (Bar, Spinner, Download, Finished) over `indicatif`.
- `prompt` — interactive prompts (`select`, `multi_select`, `confirm`, `text`/`text_with`) that speak through a `Terminal` seam: a line-driven numbered list over pipes, a rich raw-mode arrow-key widget on a TTY (radio `◉`/`○` and checkbox `[x]`/`[ ]` lists, behind the `interactive` feature), or a scripted double in tests — with a non-interactive fallback that resolves to declared defaults and never hangs.
- `signal` — `CancellationToken` cooperative Ctrl+C handling for async tasks.

## Features

- `interactive` (off by default) — rich, raw-mode prompts with arrow-key navigation via `crossterm`. The line-driven prompt path is always available and dependency-free, so enable this only when you want the live TUI experience.

## Usage

```toml
[dependencies]
rskit-cli = "0.2.0-alpha.1"
```

```rust
use rskit_cli::{ErrorRenderer, ExitCode, OutputFormat};
use rskit_errors::{AppError, ErrorCode};

let error = AppError::new(ErrorCode::InvalidInput, "bad argument");
let (rendered, exit_code) = ErrorRenderer::new(OutputFormat::Json).render(&error);
assert!(rendered.contains("\"code\""));
assert_eq!(exit_code, ExitCode::from(error.code()));
assert_eq!(exit_code.as_i32(), 2);
```

```rust
use rskit_cli::{ProgressBar, ProgressStyle, CancellationToken};
use std::time::Duration;

async fn example() {
    let bar = ProgressBar::new(100, ProgressStyle::Bar);
    bar.set_prefix("Downloading");
    for i in 0..=100 {
        bar.set_position(i);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    bar.finish_with_message("Done!");

    let token = CancellationToken::new();
    // Clone and pass to spawned tasks for cooperative shutdown
    let _child = token.clone();
}
```

```rust
use rskit_cli::{Choice, Prompter, PromptMode, ScriptedTerminal, Palette};

// Bind a scripted terminal for a deterministic example; real programs use
// `Prompter::from_env(color)`, which auto-selects a rich raw-mode terminal on a
// TTY (with the `interactive` feature) or a line terminal otherwise.
let mut prompter = Prompter::new(
    ScriptedTerminal::line_driven().with_line("2"),
    PromptMode::Interactive,
    Palette::new(false),
);
let choice = prompter.select(
    "Which ecosystem?",
    &[
        Choice::new("rust", "Rust").recommended(),
        Choice::new("go", "Go"),
    ],
).expect("a choice");
assert_eq!(choice.as_str(), "go");
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
