# rskit-fs

Local filesystem primitives for rskit.

`rskit-fs` is a low-level foundation crate. It owns reusable local filesystem mechanics; higher-level crates such as `rskit-storage`, cache backends, HTTP download helpers, and test utilities should consume it instead of reimplementing path safety, temp files, atomic writes, or tree traversal.

## Modules

| Module | Responsibility |
| --- | --- |
| `path` | Safe relative paths, root joins, absolute/canonical paths, root confinement, parent-dir helpers |
| `file` | Read, write, copy, rename, move, delete, metadata, atomic writes |
| `dir` | Create, list, inspect, remove, and recursively remove directories |
| `tree` | Recursive tree walking, listing, copying, and removal with symlink policy |
| `temp` | Managed temp files/dirs and sibling temp path generation |
| `link` | Hard links and symbolic links |
| `permissions` | Read-only flags, capability checks, and Unix mode helpers |

## Security defaults

- Use `path::safe_join` for user-provided relative paths before touching disk.
- Use `path::confine_existing_path` for existing user-provided paths and `path::confine_path` for output paths that must stay under a caller-owned root after symlink resolution.
- Tree copy/list operations do not follow symlinks unless explicitly requested.
- Use `file::write_atomic` for same-filesystem writes without exposing partial files. Replacing an existing file is atomic on Unix-like platforms; Windows replacement returns an error instead of silently degrading.
- Use `permissions::can_read` / `permissions::can_write` before optional user-facing operations.

## Scope

This crate covers local filesystem primitives only. Content hashing, object storage backends, MIME detection, and storage registries belong in separate crates such as `rskit-storage` or a future digest-focused foundation crate.
