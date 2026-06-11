//! Deterministic helpers for the interactive agent shell.

use std::path::PathBuf;

/// Parsed non-interactive representation of a shell command.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ShellCommand {
    /// Show help.
    Help,
    /// Analyze a media file.
    Analyze { path: PathBuf },
    /// Resize an image.
    Resize {
        path: PathBuf,
        width: u32,
        height: u32,
    },
    /// Run the image pipeline.
    Pipeline { path: PathBuf },
    /// Process a synthetic batch.
    Batch { count: usize },
    /// Review a file.
    Review { path: PathBuf },
    /// Launch the demo scenario.
    Demo,
    /// Cancel a running task.
    Cancel { id: usize },
    /// Show task status.
    Status,
    /// Show task details.
    Detail { id: usize },
    /// Show worker stats.
    Stats,
    /// Clear completed tasks.
    Clear,
    /// Quit the shell.
    Quit,
}

/// Parse a user-entered shell command without performing side effects.
pub fn parse_command(input: &str) -> Result<Option<ShellCommand>, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let cmd_line = trimmed.strip_prefix('/').unwrap_or(trimmed);
    let parts: Vec<&str> = cmd_line.splitn(3, ' ').collect();
    match parts[0] {
        "help" | "h" | "?" | "" => Ok(Some(ShellCommand::Help)),
        "analyze" | "a" if parts.len() >= 2 => Ok(Some(ShellCommand::Analyze {
            path: resolve_path(parts[1]),
        })),
        "resize" | "r" if parts.len() >= 2 => {
            let (width, height) = parts
                .get(2)
                .and_then(|value| parse_dimensions(value))
                .unwrap_or((200, 200));
            Ok(Some(ShellCommand::Resize {
                path: resolve_path(parts[1]),
                width,
                height,
            }))
        }
        "pipeline" | "p" if parts.len() >= 2 => Ok(Some(ShellCommand::Pipeline {
            path: resolve_path(parts[1]),
        })),
        "batch" | "b" => Ok(Some(ShellCommand::Batch {
            count: parts
                .get(1)
                .and_then(|value| value.parse().ok())
                .unwrap_or(30),
        })),
        "review" | "rv" if parts.len() >= 2 => Ok(Some(ShellCommand::Review {
            path: resolve_path(parts[1]),
        })),
        "demo" | "d" => Ok(Some(ShellCommand::Demo)),
        "cancel" if parts.len() >= 2 => {
            parse_id(parts[1], "cancel").map(|id| Some(ShellCommand::Cancel { id }))
        }
        "status" | "s" => Ok(Some(ShellCommand::Status)),
        "detail" if parts.len() >= 2 => {
            parse_id(parts[1], "detail").map(|id| Some(ShellCommand::Detail { id }))
        }
        "stats" => Ok(Some(ShellCommand::Stats)),
        "clear" | "c" => Ok(Some(ShellCommand::Clear)),
        "quit" | "q" | "exit" => Ok(Some(ShellCommand::Quit)),
        other => Err(format_unknown_command(other)),
    }
}

/// Render an unknown-command message matching the interactive shell.
pub fn format_unknown_command(command: &str) -> String {
    format!(
        "  \x1b[31m✗ Unknown command:\x1b[0m {command}\n  Type \x1b[1m/help\x1b[0m for commands."
    )
}

/// Render the interactive command help panel.
pub fn format_help() -> String {
    let mut s = String::new();
    s.push_str("\n  \x1b[1;36m┌─────────────────────────────────────────────────┐\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[1mAgent Commands\x1b[0m                                 \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m├─────────────────────────────────────────────────┤\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/demo\x1b[0m                Launch 4 parallel agents  \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/analyze\x1b[0m <file>      Detect MIME & metadata    \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/resize\x1b[0m  <file> [WxH] Resize image             \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/pipeline\x1b[0m <file>     Resize → crop → rotate    \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/review\x1b[0m  <file>      Code review simulation    \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/batch\x1b[0m   [count]     Batch processing (×30)    \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m├─────────────────────────────────────────────────┤\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/status\x1b[0m              Show all tasks             \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/detail\x1b[0m  <id>        Task details               \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/cancel\x1b[0m  <id>        Cancel running task        \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/stats\x1b[0m               Worker pool stats          \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/clear\x1b[0m               Clear completed tasks      \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m│\x1b[0m  \x1b[36m/quit\x1b[0m                Exit                       \x1b[1;36m│\x1b[0m\n");
    s.push_str("  \x1b[1;36m└─────────────────────────────────────────────────┘\x1b[0m\n");
    s
}

/// Resolve a user path, falling back to the shared fixtures for relative fixture paths.
pub fn resolve_path(input: &str) -> PathBuf {
    let path = PathBuf::from(input);
    if path.is_absolute() || path.exists() {
        path
    } else {
        fixture_dir().join(input)
    }
}

/// Return the repository fixture directory used by the examples.
pub fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("agent-demo manifest is nested under examples")
        .join("tests/fixtures")
}

/// Parse dimensions in `WIDTHxHEIGHT` form.
pub fn parse_dimensions(input: &str) -> Option<(u32, u32)> {
    let (width, height) = input.split_once('x')?;
    Some((width.parse().ok()?, height.parse().ok()?))
}

fn parse_id(input: &str, command: &str) -> Result<usize, String> {
    input
        .parse()
        .map_err(|_| format!("  \x1b[31m✗ Invalid {command} task ID:\x1b[0m {input}"))
}

/// Static banner text used by the interactive shell.
pub const BANNER: &str = concat!(
    "\n",
    "  \x1b[1;36m🚀 rskit Agent Demo\x1b[0m — Media Processing Pipeline\n",
    "  \x1b[2mShowcasing background workers, progress tracking, and stream processing\x1b[0m\n",
    "  \x1b[2mrskit::worker │ rskit::cli │ rskit::storage │ rskit::media_image\x1b[0m\n",
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_command_recognizes_help_aliases_and_empty_input() {
        assert_eq!(parse_command("").unwrap(), None);
        assert_eq!(parse_command("/help").unwrap(), Some(ShellCommand::Help));
        assert_eq!(parse_command("?").unwrap(), Some(ShellCommand::Help));
        assert_eq!(parse_command("h").unwrap(), Some(ShellCommand::Help));
    }

    #[test]
    fn parse_command_recognizes_file_commands() {
        assert!(matches!(
            parse_command("/analyze image/real-photo.jpg").unwrap(),
            Some(ShellCommand::Analyze { path }) if path.ends_with("image/real-photo.jpg")
        ));
        assert!(matches!(
            parse_command("p image/sample.png").unwrap(),
            Some(ShellCommand::Pipeline { path }) if path.ends_with("image/sample.png")
        ));
        assert!(matches!(
            parse_command("rv image/sample.png").unwrap(),
            Some(ShellCommand::Review { path }) if path.ends_with("image/sample.png")
        ));
    }

    #[test]
    fn parse_command_resize_uses_defaults_or_explicit_dimensions() {
        assert_eq!(
            parse_command("resize image/sample.png").unwrap(),
            Some(ShellCommand::Resize {
                path: fixture_dir().join("image/sample.png"),
                width: 200,
                height: 200,
            })
        );
        assert_eq!(
            parse_command("r image/sample.png 320x180").unwrap(),
            Some(ShellCommand::Resize {
                path: fixture_dir().join("image/sample.png"),
                width: 320,
                height: 180,
            })
        );
        assert_eq!(
            parse_command("r image/sample.png wide").unwrap(),
            Some(ShellCommand::Resize {
                path: fixture_dir().join("image/sample.png"),
                width: 200,
                height: 200,
            })
        );
    }

    #[test]
    fn parse_command_batch_defaults_invalid_count_to_thirty() {
        assert_eq!(
            parse_command("batch").unwrap(),
            Some(ShellCommand::Batch { count: 30 })
        );
        assert_eq!(
            parse_command("b 7").unwrap(),
            Some(ShellCommand::Batch { count: 7 })
        );
        assert_eq!(
            parse_command("b many").unwrap(),
            Some(ShellCommand::Batch { count: 30 })
        );
    }

    #[test]
    fn parse_command_recognizes_state_commands() {
        assert_eq!(parse_command("demo").unwrap(), Some(ShellCommand::Demo));
        assert_eq!(
            parse_command("cancel 42").unwrap(),
            Some(ShellCommand::Cancel { id: 42 })
        );
        assert_eq!(parse_command("status").unwrap(), Some(ShellCommand::Status));
        assert_eq!(
            parse_command("detail 9").unwrap(),
            Some(ShellCommand::Detail { id: 9 })
        );
        assert_eq!(parse_command("stats").unwrap(), Some(ShellCommand::Stats));
        assert_eq!(parse_command("clear").unwrap(), Some(ShellCommand::Clear));
        assert_eq!(parse_command("exit").unwrap(), Some(ShellCommand::Quit));
    }

    #[test]
    fn parse_command_reports_invalid_or_unknown_commands() {
        assert!(
            parse_command("cancel no")
                .unwrap_err()
                .contains("Invalid cancel")
        );
        assert!(
            parse_command("detail no")
                .unwrap_err()
                .contains("Invalid detail")
        );
        assert!(
            parse_command("bogus")
                .unwrap_err()
                .contains("Unknown command")
        );
    }

    #[test]
    fn resolve_path_uses_fixtures_for_relative() {
        let p = resolve_path("image/real-photo.jpg");
        assert!(
            p.to_string_lossy()
                .contains("tests/fixtures/image/real-photo.jpg")
        );
    }

    #[test]
    fn resolve_path_keeps_absolute() {
        let p = resolve_path("/not-tmp/test.jpg");
        assert_eq!(p, PathBuf::from("/not-tmp/test.jpg"));
    }

    #[test]
    fn parse_dimensions_valid() {
        assert_eq!(parse_dimensions("200x150"), Some((200, 150)));
        assert_eq!(parse_dimensions("1920x1080"), Some((1920, 1080)));
    }

    #[test]
    fn parse_dimensions_invalid() {
        assert_eq!(parse_dimensions("abc"), None);
        assert_eq!(parse_dimensions("200"), None);
        assert_eq!(parse_dimensions("200xabc"), None);
        assert_eq!(parse_dimensions("200X150"), None);
    }

    #[test]
    fn fixture_dir_exists() {
        let dir = fixture_dir();
        assert!(dir.exists(), "fixtures dir should exist at {dir:?}");
    }

    #[test]
    fn format_help_contains_all_commands() {
        let help = format_help();
        for cmd in &[
            "/demo",
            "/analyze",
            "/resize",
            "/pipeline",
            "/review",
            "/batch",
            "/status",
            "/detail",
            "/cancel",
            "/stats",
            "/clear",
            "/quit",
        ] {
            assert!(help.contains(cmd), "help should contain {cmd}");
        }
    }

    #[test]
    fn format_unknown_command_points_to_help() {
        let output = format_unknown_command("wat");
        assert!(output.contains("wat"));
        assert!(output.contains("/help"));
    }

    #[test]
    fn banner_is_not_empty() {
        assert!(BANNER.len() > 1);
        assert!(BANNER.contains("rskit"));
    }
}
