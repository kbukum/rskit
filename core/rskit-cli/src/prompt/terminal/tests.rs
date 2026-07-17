use super::{Capabilities, Terminal};
use crate::prompt::key::Key;
use crate::prompt::terminal::ScriptedTerminal;

#[test]
fn boxed_terminal_forwards_every_method_to_the_inner_terminal() {
    let scripted = ScriptedTerminal::key_driven()
        .with_line("typed")
        .with_key(Key::Enter);
    let mut boxed: Box<dyn Terminal> = Box::new(scripted);

    assert_eq!(boxed.capabilities(), Capabilities::key_driven());
    boxed.write("prompt").expect("write");
    boxed.write_line("line").expect("write_line");
    boxed.flush().expect("flush");
    boxed.begin_interactive().expect("begin");
    boxed.clear_last_lines(1).expect("clear");
    assert_eq!(boxed.read_line().expect("line"), Some("typed".to_string()));
    assert_eq!(boxed.read_key().expect("key"), Key::Enter);
    boxed.end_interactive().expect("end");
}
