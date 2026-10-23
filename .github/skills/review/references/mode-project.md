# Mode: review the project

Audit a crate, domain, workspace, or the whole tree, independent of any diff.

1. **Scope.** Name the boundary first: one crate, a `contrib/<domain>`, a workspace (`W=core|contrib|examples`), or a group (`make check-<group>`; list with `toven groups`).
2. **Map.** List crates and internal edges before reading code: `toven groups` and `rg -n '^rskit-' core/rskit-*/Cargo.toml contrib/*/*/Cargo.toml`.
3. **Lenses in priority order.** Apply every lens in [the review skill](../SKILL.md#lenses), top to bottom. Stop and report if placement and ownership has blockers.
4. **Gates.** Run the full gates for the scope: `make check` (or `W=`/`G=` scoped targets), `make deny`, `make coverage`, and `toven run readiness`.
5. **Report** findings in the [one-line format](../SKILL.md#findings), grouped by crate and lens, plus skipped checks.
