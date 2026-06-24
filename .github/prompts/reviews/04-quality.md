# Pass 04 — Quality: simplicity, maintainability, freshness

Catch debt and drift that compiles cleanly but should not land. None of this is style-bikeshedding — it maps to rskit's pre-stable, redesign-first stance.

> **Run in a separate, clean-context agent** — never inline in the session that wrote the code. An independent reviewer re-derives every judgment from the code and the principles instead of trusting prior reasoning. A plan/spec may be passed in as a scope checklist only; it never excuses a baseline violation.

**Scope note.** *Changes mode:* judge the diff against simpler alternatives and check the style gates on touched public items. *Project mode:* hunt for dead code, lingering compatibility shims, and outdated patterns across the crate(s).

## Checks

- **Root-cause over patches.** Pre-stable: no compatibility shims. Prefer a clean redesign over a symptom patch; flag shims as should-fix with a redesign suggestion.
- **Dead / useless code.** No-caller code, speculative generality (one impl, no near-term second), commented-out blocks, leftover scaffolding. Remove.
- **Maintainability.** Obvious to the next reader without the author? Do names match rskit vocabulary? No hidden coupling across crates? Prefer focused, well-named modules/files over piling logic into one large file.
- **Outdated patterns.** Edition 2024 / msrv 1.91 is the floor — flag patterns superseded by current idioms (manual impls where `derive` suffices, needless clones clippy-pedantic would catch).
- **Style gates.**
  - `cargo fmt` (edition 2024, max_width 100) and clippy clean (`-D warnings`, msrv 1.91).
  - `#![warn(missing_docs)]` satisfied — all public items documented.
  - `#[must_use]` on `with_*` builders; `#[non_exhaustive]` on public enums that may grow.
  - `parking_lot::Mutex`, never `std::sync::Mutex`.
  - No `unsafe` without a `// SAFETY:` comment (and justify it).
  - `AppResult<T>` for error handling throughout.

## Detection starters

```bash
rg 'unsafe ' core/ contrib/ | rg -v 'SAFETY'            # unsafe without a SAFETY comment nearby
rg 'std::sync::Mutex' core/ contrib/                     # must be parking_lot::Mutex
rg 'TODO|FIXME|XXX|HACK|deprecated|legacy|back.?compat|for now' core/ contrib/
rg 'pub fn with_' core/ contrib/                          # confirm each carries #[must_use]
rg '^\s*//\s*(let|fn|if|match|self\.)' core/ contrib/      # commented-out code
```

Then let clippy do the mechanical pass: `make lint` (clippy `-D warnings`).
