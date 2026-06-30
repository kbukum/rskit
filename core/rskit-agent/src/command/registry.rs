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
    /// Command name without the leading slash.
    pub name: String,
    /// Short human-readable description shown in command listings.
    pub description: String,
    /// Usage string showing accepted arguments.
    pub usage: String,
    /// Handler invoked when the command is executed.
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
        let Some(pos) = without_slash.find(|c: char| c.is_whitespace()) else {
            return Some((without_slash, ""));
        };
        let name = &without_slash[..pos];
        let args = without_slash[pos..].trim_start();
        Some((name, args))
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
