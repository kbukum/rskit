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
    assert!(err.message().contains("unknown command"));
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
