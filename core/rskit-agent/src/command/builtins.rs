// ── Built-in commands ───────────────────────────────────────────────────────

use rskit_errors::AppError;

use super::{Command, CommandRegistry};

/// Register the default built-in commands: `/help`, `/clear`, `/model`,
/// `/compact`.
///
/// These handlers return descriptive strings; the caller is responsible for
/// hooking them into actual agent behaviour.
pub fn register_builtins(registry: &mut CommandRegistry) -> Result<(), AppError> {
    registry.register(Command {
        name: "help".to_string(),
        description: "List available commands".to_string(),
        usage: "/help".to_string(),
        handler: Box::new(|_args: &str| -> Result<String, AppError> {
            Ok("Available commands: /help, /clear, /model, /compact\n\
                Use /help <command> for details."
                .to_string())
        }),
    })?;

    registry.register(Command {
        name: "clear".to_string(),
        description: "Clear conversation history".to_string(),
        usage: "/clear".to_string(),
        handler: Box::new(|_args: &str| -> Result<String, AppError> {
            Ok("Conversation history cleared.".to_string())
        }),
    })?;

    registry.register(Command {
        name: "model".to_string(),
        description: "Show or switch the current model".to_string(),
        usage: "/model [name]".to_string(),
        handler: Box::new(|args: &str| -> Result<String, AppError> {
            if args.is_empty() {
                Ok("Usage: /model <name> — switch the active model.".to_string())
            } else {
                Ok(format!("Model switched to: {args}"))
            }
        }),
    })?;

    registry.register(Command {
        name: "compact".to_string(),
        description: "Compact conversation context".to_string(),
        usage: "/compact".to_string(),
        handler: Box::new(|_args: &str| -> Result<String, AppError> {
            Ok("Context compacted.".to_string())
        }),
    })?;
    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────────────
