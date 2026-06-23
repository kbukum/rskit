# rskit-util

L0 domain-free utility primitives for the rskit ecosystem.

`rskit-util` owns low-level, domain-free primitives that are useful across foundation and higher-level crates. It has no internal workspace crate dependencies; small external dependencies are limited to capabilities that must live at L0, such as serde support and zeroizing secret storage.

## What belongs here

Use `rskit-util` for reusable helpers that have no service, transport, config, storage, validation, or AI domain ownership. Domain-owned helpers stay in their owning crates:

- Secret masking primitive: `rskit_util::SecretString`
- Validation rules and `AppError` conversion: `rskit-validation`
- Schema generation and JSON Schema validation: `rskit-schema`
- Filesystem operations and safe paths: `rskit-fs`
- Generic clocks and UTC formatting: `rskit_util::time`

## Modules

| Module | Purpose |
| ------ | ------- |
| `backoff` | Stateless exponential backoff and deterministic jitter calculations |
| `bytes` | Human-readable byte size formatting and parsing |
| `collections` | `chunk`, `group_by`, `index_by`, and `partition` helpers |
| `env` | Environment variable lookup, non-empty filtering, parsing, and defaults |
| `secret` | Secret string masking for logs, debug output, and serialization |
| `strings` | Case conversion and UTF-8-safe truncation |
| `template` | Typed `{placeholder}` template parsing and rendering |
| `time` | Duration formatting/parsing, UTC civil date/time conversion, RFC 3339/compact UTC helpers, injectable clocks, and timing helpers |

## Usage

```toml
[dependencies]
rskit-util = "0.1.0-alpha.3"
```

### Secret values

```rust
use rskit_util::SecretString;

let password = SecretString::new("hunter2");
assert_eq!(password.expose(), "hunter2");
assert_eq!(password.to_string(), "***");
assert_eq!(format!("{password:?}"), "SecretString(***)");
```

### Parsing sizes and durations

```rust
use std::time::Duration;

assert_eq!(rskit_util::bytes::parse_bytes("1.5 MB"), Some(1_572_864));
assert_eq!(rskit_util::time::parse_duration("2m"), Some(Duration::from_secs(120)));
```

### Environment lookup

```rust
assert_eq!(rskit_util::env::get_non_empty("RSKIT_MISSING"), None);
let value = rskit_util::env::get_or("RSKIT_MISSING", "fallback");
assert_eq!(value, "fallback");
```

### UTC date/time helpers

```rust
use rskit_util::time::{
    Clock, FixedClock, format_compact_utc, format_rfc3339, parse_rfc3339_utc,
};

assert_eq!(format_rfc3339(0), Some("1970-01-01T00:00:00Z".to_owned()));
assert_eq!(format_compact_utc(0), Some("19700101-000000".to_owned()));
assert_eq!(parse_rfc3339_utc("1970-01-01T00:00:00Z"), Some(0));

let clock = FixedClock::new(1_700_000_000, 42);
assert_eq!(clock.epoch_seconds(), 1_700_000_000);
```

### Typed templates

```rust
use std::fmt;
use rskit_util::template::{Placeholder, Template};

#[derive(Clone, Copy, Eq, PartialEq)]
enum Token {
    Name,
}

impl Placeholder for Token {
    fn token(self) -> &'static str {
        match self {
            Self::Name => "name",
        }
    }
}

impl fmt::Display for Token {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.token())
    }
}

let template = Template::parse("hello {name}", &[Token::Name])?;
let rendered = template.render_with(|token| match token {
    Token::Name => Ok::<_, std::convert::Infallible>("rskit".to_string()),
})?;

assert_eq!(rendered, "hello rskit");
# Ok::<(), rskit_util::template::TemplateError>(())
```

## Cross-kit alignment

This crate mirrors the utility modules in:

- **gokit** — `github.com/kbukum/gokit/util`
- **pykit** — `pykit-util` package
