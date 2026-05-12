//! Slash-command registry for interactive agent sessions.
//!
//! Commands are prefixed with `/` and dispatched through a [`CommandRegistry`].
//! Call [`register_builtins`] to register the default `/help`, `/clear`,
//! `/model`, and `/compact` commands.

use std::collections::HashMap;

use rskit_errors::{AppError, ErrorCode};

// ── CommandHandler trait ────────────────────────────────────────────────────

/// Handler for a single slash command.
pub trait CommandHandler: Send + Sync {
    /// Execute the command with the given arguments string.
    fn execute(&self, args: &str) -> Result<String, AppError>;
}

/// Blanket implementation: any `Fn(&str) -> Result<String, AppError>` can be
/// used as a handler.
impl<F> CommandHandler for F
where
    F: Fn(&str) -> Result<String, AppError> + Send + Sync,
{
    fn execute(&self, args: &str) -> Result<String, AppError> {
        (self)(args)
    }
}

// ── Command ─────────────────────────────────────────────────────────────────

/// A registered slash command.
pub struct Command {
    pub name: String,
    pub description: String,
    pub usage: String,
    pub handler: Box<dyn CommandHandler>,
}

// ── CommandRegistry ─────────────────────────────────────────────────────────

/// Registry of slash commands.
pub struct CommandRegistry {
    commands: HashMap<String, Command>,
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    /// Register a command. Returns an error if the name is empty.
    /// Overwrites any existing command with the same name.
    pub fn register(&mut self, cmd: Command) -> Result<(), AppError> {
        if cmd.name.trim().is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "command name must not be empty",
            ));
        }
        self.commands.insert(cmd.name.clone(), cmd);
        Ok(())
    }

    /// Look up a command by name (without the leading `/`).
    pub fn get(&self, name: &str) -> Option<&Command> {
        self.commands.get(name)
    }

    /// Return all registered commands (sorted by name for deterministic output).
    pub fn list(&self) -> Vec<&Command> {
        let mut cmds: Vec<&Command> = self.commands.values().collect();
        cmds.sort_by(|a, b| a.name.cmp(&b.name));
        cmds
    }

    /// Check whether the input looks like a slash command.
    pub fn is_command(input: &str) -> bool {
        let trimmed = input.trim();
        trimmed.starts_with('/')
            && trimmed
                .chars()
                .nth(1)
                .is_some_and(|c| c.is_ascii_alphabetic())
    }

    /// Parse input into `(command_name, args)`.  Returns `None` if the input
    /// is not a slash command.
    pub fn parse_command(input: &str) -> Option<(&str, &str)> {
        let trimmed = input.trim();
        if !Self::is_command(trimmed) {
            return None;
        }

        let without_slash = &trimmed[1..];
        match without_slash.find(|c: char| c.is_whitespace()) {
            Some(pos) => {
                let name = &without_slash[..pos];
                let args = without_slash[pos..].trim_start();
                Some((name, args))
            }
            None => Some((without_slash, "")),
        }
    }

    /// Execute a slash-command input string.
    pub fn execute(&self, input: &str) -> Result<String, AppError> {
        let (name, args) = Self::parse_command(input).ok_or_else(|| {
            AppError::new(ErrorCode::InvalidInput, "input is not a slash command")
        })?;

        let cmd = self.get(name).ok_or_else(|| {
            AppError::new(ErrorCode::InvalidInput, format!("unknown command: /{name}"))
        })?;

        cmd.handler.execute(args)
    }
}

// ── Built-in commands ───────────────────────────────────────────────────────

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_command() {
        assert!(CommandRegistry::is_command("/help"));
        assert!(CommandRegistry::is_command("  /clear  "));
        assert!(!CommandRegistry::is_command("hello"));
        assert!(!CommandRegistry::is_command("/ nope"));
        assert!(!CommandRegistry::is_command("/"));
        assert!(!CommandRegistry::is_command(""));
    }

    #[test]
    fn test_parse_command_no_args() {
        let (name, args) = CommandRegistry::parse_command("/help").unwrap();
        assert_eq!(name, "help");
        assert_eq!(args, "");
    }

    #[test]
    fn test_parse_command_with_args() {
        let (name, args) = CommandRegistry::parse_command("/model gpt-4o").unwrap();
        assert_eq!(name, "model");
        assert_eq!(args, "gpt-4o");
    }

    #[test]
    fn test_parse_command_not_a_command() {
        assert!(CommandRegistry::parse_command("hello").is_none());
    }

    #[test]
    fn test_register_and_get() {
        let mut reg = CommandRegistry::new();
        reg.register(Command {
            name: "ping".to_string(),
            description: "Ping".to_string(),
            usage: "/ping".to_string(),
            handler: Box::new(|_: &str| Ok("pong".to_string())),
        })
        .unwrap();
        assert!(reg.get("ping").is_some());
        assert!(reg.get("missing").is_none());
    }

    #[test]
    fn test_list_sorted() {
        let mut reg = CommandRegistry::new();
        for name in &["zebra", "alpha", "mid"] {
            reg.register(Command {
                name: name.to_string(),
                description: String::new(),
                usage: String::new(),
                handler: Box::new(|_: &str| Ok(String::new())),
            })
            .unwrap();
        }
        let names: Vec<&str> = reg.list().iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "mid", "zebra"]);
    }

    #[test]
    fn test_execute_known_command() {
        let mut reg = CommandRegistry::new();
        reg.register(Command {
            name: "echo".to_string(),
            description: "Echo args".to_string(),
            usage: "/echo <text>".to_string(),
            handler: Box::new(|args: &str| Ok(format!("echo: {args}"))),
        })
        .unwrap();
        let result = reg.execute("/echo hello world").unwrap();
        assert_eq!(result, "echo: hello world");
    }

    #[test]
    fn test_execute_unknown_command() {
        let reg = CommandRegistry::new();
        let err = reg.execute("/nope").unwrap_err();
        assert!(err.message.contains("unknown command"));
    }

    #[test]
    fn test_builtins() {
        let mut reg = CommandRegistry::new();
        register_builtins(&mut reg).unwrap();

        assert!(reg.get("help").is_some());
        assert!(reg.get("clear").is_some());
        assert!(reg.get("model").is_some());
        assert!(reg.get("compact").is_some());

        let help_out = reg.execute("/help").unwrap();
        assert!(help_out.contains("/help"));

        let model_out = reg.execute("/model gpt-4o").unwrap();
        assert!(model_out.contains("gpt-4o"));
    }
}
