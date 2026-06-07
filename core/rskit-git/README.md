# rskit-git — Git Integration

Composable git repository interfaces backed by libgit2 and the `git` CLI.

## Features

- Capability traits for reads, writes, refs, remotes, config, and maintenance.
- Embedded `git2` backend for repository operations.
- `GitCli` backend for command-oriented workflows using argv-only subprocess execution.
- Typed `GitError` variants converted into `AppError` for consistent rskit error handling.

## Usage

```toml
[dependencies]
rskit-git = "0.1"
```

```rust
use rskit_git::{Differ, Repository, open};

let repo = open("/path/to/repo")?;
let dirty = repo.is_dirty()?;
let status = repo.status()?;
# Ok::<(), rskit_git::AppError>(())
```

The CLI backend builds commands with `ProcessSpec::new("git").args(...)` and
sets `GIT_TERMINAL_PROMPT=0` to avoid interactive credential prompts in
automation. Command failures preserve argv, exit code, stdout/stderr, and
truncation metadata.
