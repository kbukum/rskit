# rskit-git — Git Integration

Composable git repository interfaces backed by libgit2 and the `git` CLI.

## Features

- Capability traits for reads, writes, refs, remotes, config, and maintenance.
- Bounded blob reads through `TreeReader::file_at_bounded(revision, path, max_bytes)`, which checks the object-database size header before materializing content and rejects an oversized, repository-controlled file with `GitError::FileTooLarge` instead of copying it into memory.
- Embedded `git2` backend for repository operations.
- `GitCli` backend for command-oriented workflows using argv-only subprocess execution.
- Authenticated push/fetch through a pluggable `AuthProvider` seam: `DefaultAuthProvider` (git's own credential lookup), `StaticAuthProvider`, `EnvTokenAuthProvider` (token from an environment variable), and `ChainAuthProvider` (try providers in order), all yielding a `TransportAuth` credential.
- Deterministic repository init: `main` is pinned as the default initial branch through the public `DEFAULT_BRANCH` constant, so `init`/`init_bare` never inherit the host's `init.defaultBranch` (git2 would otherwise fall back to `master`); `init_with` takes an explicit `InitOptions::initial_branch` override, validated before any repository is created.
- Annotated tag signing through `RefManager::create_signed_tag(name, target, message, &SignOptions)`. Signing always uses the git CLI (`git tag -s`) under a bounded timeout because libgit2 cannot create signed tag objects; `Repo` routes signed tags to the CLI backend, while the embedded backend reports `GitError::SigningNotSupported` when used directly.
- `SignOptions` can pin `gpg.format` (`openpgp`, `ssh`, or `x509`) and `user.signingkey`, or inherit either value from the repository/global git config when left unset. The CLI backend preflights the effective signing key and returns `GitError::SigningKeyMissing` before invoking the signer when neither a non-blank explicit key nor configured `user.signingkey` is available.
- Typed `GitError` variants converted into `AppError` for consistent rskit error handling.

## Cargo features

- `vendored-openssl` (off by default): statically link a source-built OpenSSL into the embedded `git2` backend instead of resolving one from the host. The default builds against the system OpenSSL so it stays on the platform's security updates; opt in for portable or cross-compiled release builds where a target OpenSSL is not available.
- `testutil` (off by default): expose the in-crate test helpers.

## Usage

```toml
[dependencies]
rskit-git = "0.2.0-alpha.5"
```

```rust
use rskit_git::{Differ, Repository, open};

let repo = open("/path/to/repo")?;
let dirty = repo.is_dirty()?;
let status = repo.status()?;
# Ok::<(), rskit_git::AppError>(())
```

The CLI backend builds commands with `ProcessSpec::new("git").args(...)` and sets `GIT_TERMINAL_PROMPT=0` to avoid interactive credential prompts in automation. Command failures preserve argv, exit code, stdout/stderr, and truncation metadata.
