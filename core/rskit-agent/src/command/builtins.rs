// ── Built-in commands ───────────────────────────────────────────────────────

use rskit_errors::AppError;

use super::{Command, CommandRegistry};

/// Register the default built-in commands: `/help`, `/clear`, `/model`, `/compact`.
///
/// These handlers return descriptive strings;
/// the caller is responsible for hooking them into actual agent behaviour.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_register_and_execute_expected_responses() {
        let mut registry = CommandRegistry::new();
        register_builtins(&mut registry).unwrap();

        assert!(
            registry
                .execute("/help")
                .unwrap()
                .contains("Available commands")
        );
        assert_eq!(
            registry.execute("/clear").unwrap(),
            "Conversation history cleared."
        );
        assert!(
            registry
                .execute("/model")
                .unwrap()
                .contains("Usage: /model")
        );
        assert_eq!(
            registry.execute("/model claude").unwrap(),
            "Model switched to: claude"
        );
        assert_eq!(registry.execute("/compact").unwrap(), "Context compacted.");
        assert_eq!(registry.list().len(), 4);
    }
}
