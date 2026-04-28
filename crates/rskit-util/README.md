# rskit-util

Pure utility functions for the rskit ecosystem.

## Modules

| Module     | Description                                    |
|------------|------------------------------------------------|
| `sanitize` | String sanitisation and basic safety checks    |
| `parse`    | Human-readable size parsing and secret masking |
| `clock`    | Deterministic clock trait for testable code    |
| `merge`    | Deep-merge for `serde_json::Value` maps        |

## Usage

```toml
[dependencies]
rskit-util = { path = "../rskit-util" }
```

```rust
use rskit_util::{sanitize_string, parse_size, SystemClock, Clock, deep_merge};

let clean = sanitize_string("  hello\x00world  ");
let bytes = parse_size("10MB", 0);
let now = SystemClock.now();
```

## Cross-kit alignment

This crate mirrors the utility modules in:

- **gokit** — `github.com/kbukum/gokit/util`
- **pykit** — `pykit-util` package
