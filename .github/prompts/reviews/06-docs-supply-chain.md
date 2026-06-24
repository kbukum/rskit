# Pass 06 — Documentation and supply chain

The last gate: public surfaces are documented, and the supply chain stays scanned, pinned, and clean. rskit ships to downstream consumers, so dependency and release hygiene are first-class.

> **Run in a separate, clean-context agent** — never inline in the session that wrote the code. An independent reviewer re-derives every judgment from the code and the principles instead of trusting prior reasoning. A plan/spec may be passed in as a scope checklist only; it never excuses a baseline violation.

**Scope note.** *Changes mode:* check the docs touched by (or owed by) the diff, plus any dependency or CI-action it introduces. *Project mode:* audit the whole CI/supply-chain surface (`cargo-deny` config, action pins, `Cargo.lock`, SBOM tooling) and confirm every crate carries crate-level docs.

## Documentation

- **Public API documented.** Public API changes are reflected in `///` docs and `make doc` (`-D warnings`) passes. `#![warn(missing_docs)]` is satisfied — every public item is documented.
- **New crate docs.** A new crate carries `#![warn(missing_docs)]` and crate-level (`//!`) docs explaining its purpose and place in the toolkit.
- **Conventional Commits.** `feat` / `fix` / `docs` / `refactor` / `test` / `chore`.

## Supply chain

- **`Cargo.lock` committed.** And updated with any dependency change, consistent with the manifests (`make check-workspace-deps-sync` for shared-version drift).
- **Dependencies scanned.** Vulnerabilities *and* licenses — `cargo-deny` clean (`make deny`, which also runs the L7-edge, workspace-dep-sync, topology, and public-API checks).
- **New dependency justified.** Maintained, no open CVE, not duplicating a core crate or std (ties back to pass `01` currency). Reinventing a std facility or re-pulling a concern an owner already covers is a should-fix.
- **CI actions pinned by SHA.** Any new or changed GitHub Actions step is pinned to a commit SHA, not a floating tag.
- **Release artifacts.** When the change touches release tooling: artifacts signed, SBOM/provenance attached (`make release-sbom`, `make release-readiness`).

## Detection starters

```bash
# crate-level docs / missing_docs lint present in each crate
rg '#!\[warn\(missing_docs\)\]' core/rskit-*/src/lib.rs contrib/*/*/src/lib.rs --files-without-match
# unpinned actions (uses: owner/repo@vX or @branch instead of @<sha>)
rg 'uses:\s+\S+@(?!.{40})' .github/workflows
# new/changed deps to justify
git diff <base>...HEAD -- '**/Cargo.toml' Cargo.lock
```

Then `make doc` and `make deny` for the gates, plus `make release-readiness` / `make release-sbom` if release tooling changed, and confirm `Cargo.lock` is staged.
