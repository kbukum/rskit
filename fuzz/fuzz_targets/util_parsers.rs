#![no_main]

use std::fmt;

use libfuzzer_sys::fuzz_target;
use rskit_util::template::{Placeholder, Template};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum Token {
    Name,
    Args,
}

impl Placeholder for Token {
    fn token(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Args => "args",
        }
    }
}

impl fmt::Display for Token {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.token())
    }
}

const TOKENS: &[Token] = &[Token::Name, Token::Args];

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    let _ = rskit_util::bytes::parse_bytes(text);
    let _ = rskit_util::time::parse_duration(text);

    if let Ok(template) = Template::parse(text, TOKENS) {
        let _ = template.render_with(|placeholder| match placeholder {
            Token::Name => Ok::<_, &'static str>("name".to_string()),
            Token::Args => Ok::<_, &'static str>("args".to_string()),
        });
    }
});
