# rskit-git — Git Integration

Composable git repository interfaces backed by libgit2 and the `git` CLI.

## Features

- Capability traits for reads, writes, refs, remotes, config, and maintenance.
- Embedded `git2` backend for repository operations.
- `GitCli` backend for command-oriented workflows using argv-only subprocess execution.
- Annotated tag signing through `RefManager::create_signed_tag(name, target, message, &SignOptions)`. Signing always uses the git CLI (`git tag -s`) because libgit2 cannot create signed tag objects; `Repo` routes signed tags to the CLI backend, while the embedded backend reports `GitError::SigningNotSupported` when used directly.
- `SignOptions` can pin `gpg.format` (`openpgp`, `ssh`, or `x509`) and `user.signingkey`, or inherit either value from the repository/global git config when left unset. The CLI backend preflights the effective signing key and returns `GitError::SigningKeyMissing` before invoking the signer when neither a non-blank explicit key nor configured `user.signingkey` is available.
- Typed `GitError` variants converted into `AppError` for consistent rskit error handling.

## Cargo features

- `vendored-openssl` (off by default): statically link a source-built OpenSSL into the embedded `git2` backend instead of resolving one from the host. The default builds against the system OpenSSL so it stays on the platform's security updates; opt in for portable or cross-compiled release builds where a target OpenSSL is not available.
- `testutil` (off by default): expose the in-crate test helpers.

## Usage

```toml
[dependencies]
rskit-git = "0.2.0-alpha.4"
```

```rust
use rskit_git::{Differ, Repository, open};

let repo = open("/path/to/repo")?;
let dirty = repo.is_dirty()?;
let status = repo.status()?;
# Ok::<(), rskit_git::AppError>(())
```

The CLI backend builds commands with `ProcessSpec::new("git").args(...)` and sets `GIT_TERMINAL_PROMPT=0` to avoid interactive credential prompts in automation. Command failures preserve argv, exit code, stdout/stderr, and truncation metadata.
