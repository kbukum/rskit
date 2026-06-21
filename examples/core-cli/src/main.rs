//! `core-cli` binary entry point.
//!
//! A command-line tool built entirely on rskit **core** crates — no transport
//! or server crate, and no rskit facade. See `docs/CONSUMER-CLASSES.md`.
//!
//! Usage:
//!
//! ```text
//! core-cli version
//! core-cli show <config.toml>
//! core-cli run <units>
//! ```

use std::process::ExitCode as ProcessExit;

use core_cli::commands;
use rskit_cli::{CancellationToken, ErrorRenderer, ExitCode};
use rskit_errors::{AppError, AppResult};

#[tokio::main]
async fn main() -> ProcessExit {
    match dispatch(std::env::args().skip(1).collect()).await {
        Ok(()) => ProcessExit::SUCCESS,
        Err(err) => {
            let (rendered, code) = ErrorRenderer::default().render(&err);
            eprintln!("{rendered}");
            to_process_exit(code)
        }
    }
}

async fn dispatch(args: Vec<String>) -> AppResult<()> {
    let mut args = args.into_iter();
    match args.next().as_deref() {
        Some("version") => {
            print!("{}", commands::version::render());
            Ok(())
        }
        Some("show") => {
            let path = args
                .next()
                .ok_or_else(|| AppError::invalid_input("show", "missing <config.toml> argument"))?;
            let (_guard, rendered) = commands::show::execute(&path)?;
            print!("{rendered}");
            Ok(())
        }
        Some("run") => {
            let units = args
                .next()
                .ok_or_else(|| AppError::invalid_input("run", "missing <units> argument"))?
                .parse::<u32>()
                .map_err(|err| {
                    AppError::invalid_input("units", "expected a non-negative integer")
                        .with_cause(err)
                })?;
            let token = install_signal_handler();
            print!("{}", commands::run::execute(units, &token).await);
            Ok(())
        }
        other => Err(AppError::invalid_input(
            "command",
            format!(
                "unknown command `{}`; expected version|show|run",
                other.unwrap_or("<none>")
            ),
        )),
    }
}

/// Spawn a task that cancels the returned token on Ctrl+C.
fn install_signal_handler() -> CancellationToken {
    let token = CancellationToken::new();
    let trigger = token.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            trigger.cancel();
        }
    });
    token
}

fn to_process_exit(code: ExitCode) -> ProcessExit {
    ProcessExit::from(u8::try_from(code.as_i32()).unwrap_or(1))
}
