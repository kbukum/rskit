# rskit-testutil — Test Utilities

Test utilities, mock providers, and assertion helpers for rskit services.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/rskit-testutil.svg)](https://crates.io/crates/rskit-testutil) [![docs.rs](https://docs.rs/rskit-testutil/badge.svg)](https://docs.rs/rskit-testutil) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kbukum/rskit/blob/main/LICENSE) [![MSRV: 1.91](https://img.shields.io/badge/MSRV-1.91-orange.svg)](https://www.rust-lang.org/)

## Features

- `MockProvider<I, O>` — generic mock with `will_return` / `will_fail` queues and call recording
- `assert_ok(result)` — unwrap `AppResult` or panic with context
- `assert_err_code(result, code)` — assert a specific `ErrorCode`
- `TestWorkspace` / `test_workspace!` — managed temp workspaces with fixture loading and copying
- `golden` — golden/snapshot verification: caller-supplied normalization rules, matcher tiers (`Exact` / `Normalized` / `LineSet` / `Subset`), and a `Golden` file handle that compares or — with `RSKIT_BLESS` set — regenerates expected output
- Thread-safe via `parking_lot::Mutex`

`TestWorkspace` is the generic fixture harness for rskit tests. Use `with_fixture_dir` when fixtures live outside the default crate layout, or `test_workspace!` when a crate follows the conventional `tests/fixtures` directory.

## Usage

```toml
[dev-dependencies]
rskit-testutil = "0.2.0-alpha.5"
```

```rust
use rskit_testutil::{MockProvider, assert_ok, assert_err_code};
use rskit_errors::ErrorCode;

let mock = MockProvider::<String, u64>::new();
mock.will_return(42);

let result = mock.execute("hello".into());
assert_ok(result);
assert_eq!(mock.call_count(), 1);

mock.will_fail(rskit_errors::AppError::new(ErrorCode::NotFound, "gone"));
assert_err_code(mock.execute("bye".into()), ErrorCode::NotFound);
```

```rust
use rskit_testutil::test_workspace;

let workspace = test_workspace!("config");
let config_path = workspace
    .copy_fixture("config/example.toml", "app.toml")
    .unwrap();

assert!(config_path.starts_with(workspace.path()));
```

```rust
use rskit_testutil::TestWorkspace;

let fixtures = std::path::PathBuf::from("custom-fixtures");
let workspace = TestWorkspace::new("custom").with_fixture_dir(fixtures);
let _ = workspace.fixture_path("config/app.toml");
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
