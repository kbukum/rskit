.PHONY: all setup build test test-default test-doc test-python test-affected coverage coverage-changed test-coverage lint fmt fmt-check check check-fast check-facade-features \
       doc deny check-l7-edges check-workspace-deps-sync check-topology check-public-api release-plan release-status release-readiness release-coverage \
       release-bump release-tag publish-dry-run release-publish release-sbom depgraphs clean help ci ci-test ci-lint ci-fmt ensure-act structure toven-canary lock

CORE_MANIFEST = core/Cargo.toml
CONTRIB_MANIFEST = contrib/Cargo.toml
EXAMPLES_MANIFEST = examples/Cargo.toml
FUZZ_MANIFEST = fuzz/Cargo.toml
PYTHON ?= python3
# Toven binary that runs every gate. Defaults to a `toven` on PATH; point it at
# a freshly built binary to dogfood a candidate, e.g.
# `make TOVEN=../toven/target/release/toven check`.
TOVEN ?= toven
# Baseline ref for changed-scope coverage selection.
CHANGED_BASE ?= origin/main
# Every workspace that owns a Cargo.lock, including the fuzz harness Toven
# excludes. `lock` stays on plain cargo so the fuzz lockfile is refreshed too.
LOCK_MANIFESTS = $(CORE_MANIFEST) $(CONTRIB_MANIFEST) $(EXAMPLES_MANIFEST) $(FUZZ_MANIFEST)
THRESHOLD ?=

# Toven module selection: C=<crate>, W=core|contrib|examples, G=<group>.
# With none set, a task runs over every module.
_SEL = $(if $(C),--module rust:$(C)) $(if $(W),--workspace rust:$(W)) $(if $(G),--group $(G)) $(if $(JOBS),--jobs $(JOBS))
# nextest passthrough: PROFILE=<profile> and the T=<pattern> filter.
_TEST_ARGS = $(if $(PROFILE)$(T),-- $(if $(PROFILE),--profile $(PROFILE)) $(T))

## Default target
all: check

## Install or verify local development tooling
setup:
	@PYTHON="$(PYTHON)" ./scripts/setup.sh

define run_coverage_target
	@$(TOVEN) coverage $(1) \
		$(if $(THRESHOLD),--line $(THRESHOLD)) \
		$(if $(JOBS),--jobs $(JOBS))
endef

## Build (C=<crate>, W=core|contrib|examples, G=<group>)
build:
	@$(TOVEN) build $(_SEL)

## Refresh every workspace Cargo.lock after version or dependency changes.
## Re-resolves workspace members only (cargo update --workspace); pass
## OFFLINE=1 to resolve without hitting the registry.
lock:
	@echo "==> Refreshing lockfiles..."
	@set -e; \
	for manifest in $(LOCK_MANIFESTS); do \
		cargo update --workspace --manifest-path $$manifest $(if $(OFFLINE),--offline,); \
	done
	@echo "✓ Lockfiles refreshed"

## Run tests with nextest (C=, W=, G=, T=<pattern>, PROFILE=<nextest profile>)
test:
	@$(TOVEN) test $(_SEL) $(_TEST_ARGS)

## Run tests with default features only (C=, W=, G=, T=<pattern>, PROFILE=)
test-default:
	@$(TOVEN) run test-default $(_SEL) $(_TEST_ARGS)

## Run only doctests (nextest doesn't run doctests)
test-doc:
	@$(TOVEN) run doctest $(_SEL)

## Run Python repository tooling tests
test-python:
	@echo "==> Running Python tooling tests..."
	@$(PYTHON) -m unittest discover -s scripts/tests -t .
	@echo "✓ Python tooling tests passed"

## Run tests for crates changed against CHANGED_BASE plus their dependents,
## with all features and with default features
test-affected:
	@$(TOVEN) run test,test-default --base $(CHANGED_BASE) --merge-base $(if $(JOBS),--jobs $(JOBS)) $(_TEST_ARGS)

## Run per-package coverage and gate against the thresholds in toven.toml
## (THRESHOLD=<pct> overrides the line floor, JOBS=<n> caps parallelism)
coverage:
	$(call run_coverage_target,)

## Alias for coverage
test-coverage: coverage

## Run coverage for crates changed against CHANGED_BASE (default origin/main)
coverage-changed:
	@$(TOVEN) coverage --base $(CHANGED_BASE) --merge-base \
		$(if $(THRESHOLD),--line $(THRESHOLD)) \
		$(if $(JOBS),--jobs $(JOBS))

## Run clippy with all features and targets; warnings fail (C=, W=, G=)
lint:
	@$(TOVEN) lint $(_SEL)

## Format code (W=core|contrib|examples)
fmt:
	@$(TOVEN) run format $(_SEL)

## Check formatting without modifying files
fmt-check:
	@$(TOVEN) run format-check $(_SEL)

## Declare-only aggregator guard (lib.rs / mod.rs) + crowded-module advisory
## Both gates run through Toven's `command` ecosystem (structure = ast-grep,
## crowded-modules = rskit_tool); ensure-ast-grep provisions the ast-grep tool.
structure:
	@echo "==> Checking declare-only aggregators (lib.rs / mod.rs)..."
	@./scripts/ensure-ast-grep.sh
	@$(TOVEN) run structure
	@echo "==> Scanning for crowded modules (advisory)..."
	@$(TOVEN) run crowded-modules

## Build documentation with private items; warnings fail (C=, W=, G=)
doc:
	@$(TOVEN) doc $(_SEL)

## Run L7 dependency edge checks
check-l7-edges:
	@echo "==> Checking L7 dependency edges..."
	@$(TOVEN) run l7-edges
	@echo "✓ L7 dependency edges OK"

## Check shared core/contrib workspace dependency versions
check-workspace-deps-sync:
	@echo "==> Checking workspace dependency versions..."
	@$(TOVEN) run workspace-deps-sync
	@echo "✓ Workspace dependency versions synced"

## Run module topology checks
check-topology:
	@echo "==> Checking module topology..."
	@$(TOVEN) run topology
	@echo "✓ Module topology OK"

## Check public API guardrails
check-public-api:
	@echo "==> Checking public API guardrails..."
	@$(TOVEN) run public-api
	@echo "✓ Public API guardrails OK"

## Check facade feature combinations
check-facade-features:
	@echo "==> Checking rskit facade feature combinations..."
	@cargo check --manifest-path $(CORE_MANIFEST) -p rskit-suite
	@cargo check --manifest-path $(CORE_MANIFEST) -p rskit-suite --all-features
	@cargo check --manifest-path $(CORE_MANIFEST) -p rskit-suite --features "server grpc http httpclient sse"
	@cargo check --manifest-path $(CORE_MANIFEST) -p rskit-suite --features "auth authz security encryption di observability discovery"
	@cargo check --manifest-path $(CORE_MANIFEST) -p rskit-suite --features "database database-sqlite cache cache-fs cache-redis messaging messaging-kafka messaging-nats messaging-rabbitmq storage storage-s3 storage-gcs storage-supabase vectorstore vectorstore-qdrant"
	@cargo check --manifest-path $(CORE_MANIFEST) -p rskit-suite --features "dag chain process stateful version schema hook"
	@cargo check --manifest-path $(CORE_MANIFEST) -p rskit-suite --features "genai prompt llm llm-openai llm-anthropic llm-ollama llm-gemini embedding inference inference-triton inference-vllm inference-tgi skill tool agent mcp media-full"
	@cargo check --manifest-path $(CORE_MANIFEST) -p rskit-suite --features "cli git dataset bench"
	@echo "✓ Facade feature combinations OK"

## Run cargo-deny, public API, topology, dependency sync, and dependency edge checks
## Requires: cargo-deny, cargo-public-api, and a nightly rustdoc JSON toolchain
deny: check-l7-edges check-workspace-deps-sync check-topology check-public-api
	@echo "==> Running cargo-deny..."
	@if [ -n "$(W)" ]; then \
		case "$(W)" in \
			core) cargo deny --manifest-path core/Cargo.toml --config deny.toml check licenses advisories sources bans ;; \
			contrib) cargo deny --manifest-path contrib/Cargo.toml --config deny.contrib.toml check licenses advisories sources bans ;; \
			examples) cargo deny --manifest-path examples/Cargo.toml --config deny.examples.toml check licenses advisories sources bans ;; \
			*) echo "error: make deny supports W=core, W=contrib, or W=examples" >&2; exit 2 ;; \
		esac; \
	else \
		cargo deny --manifest-path core/Cargo.toml --config deny.toml check licenses advisories sources bans && \
		cargo deny --manifest-path contrib/Cargo.toml --config deny.contrib.toml check licenses advisories sources bans && \
		cargo deny --manifest-path examples/Cargo.toml --config deny.examples.toml check licenses advisories sources bans; \
	fi
	@echo "✓ cargo-deny passed"

## Preview the release plan: crates, versions, tags, and publish order (read-only)
release-plan:
	@$(TOVEN) release plan

## Report release status: declared versions, tags, and published versions (read-only)
release-status:
	@$(TOVEN) release status

## Run the release-readiness supply-chain and API sweep
## Requires: cargo-deny, cargo-audit
## Runs both preflights: the rskit-specific guardrail sweep (panic/unwrap,
## unsafe-without-SAFETY, SHA-pinned Actions, required fuzz targets) via the
## `readiness` command task, then Toven's native gate (clean tree, registry
## idempotency).
release-readiness: check
	@$(TOVEN) run readiness
	@$(TOVEN) release readiness

## Run release coverage gates (per-package line coverage floors from toven.toml)
release-coverage: coverage

## Phase 1 — bump: write per-crate manifest version bumps + dependency floors into
## the working tree and stage them, WITHOUT committing (stage-only is the default;
## `--yes` confirms the mutation). Run it on a CLEAN `main`: the bump gate rejects
## an uncommitted/staged tree, and the prerelease channel is resolved from the
## current branch (`branch_channels`, only `main` maps to `alpha`), so bumping on
## a `release/*` branch would finalize the alpha train to a stable version. Bump on
## main, then rotate the CHANGELOG and cut the release branch carrying both; you
## commit and open the PR yourself. Toven never creates the branch, the commit, or
## the PR, and never tags/pushes/publishes here. Change-aware: only crates with a
## real diff since baseline bump, plus the dependency cascade; the configured
## on-resolved hook re-syncs the README install-snippet version pins in the same
## staged change.
release-bump:
	@$(TOVEN) release bump --yes

## Phase 2 — tag (run only after the release-bump PR merges into main): the
## maintainer cuts the single signed umbrella `vX.Y.Z` tag by hand on the merged
## commit (entrypoint = "maintainer"). This target then VERIFIES that umbrella
## tag exists at HEAD — Toven never creates or moves a maintainer-owned tag, and
## `tag_mode = "umbrella"` means only the one umbrella tag is checked, never
## per-crate tags. The hosted GitHub Release is published separately in Phase 3;
## run release-publish for the crates.io publication.
release-tag:
	@$(TOVEN) release tag --yes

## Phase 3 (dry-run): rehearse the crates.io publication in dependency order — the
## per-crate would-publish / already-published verdicts — without mutating anything.
publish-dry-run:
	@$(TOVEN) release publish --dry-run

## Phase 3 — publish: verify the maintainer's umbrella tag from phase 2 points at
## HEAD, then publish each crate to crates.io in dependency order (idempotent).
## Does not create commits or tags; CI runs this against the checked-out release
## tag. Run after the release-tag phase.
## Requires: CARGO_REGISTRY_TOKEN
release-publish:
	@$(TOVEN) release publish --yes

## Generate CycloneDX SBOMs under target/sbom across the all-features surface
## rskit_tool owns the all-features rendering (Toven orchestrates the `sbom` task).
## Requires: cargo-cyclonedx
release-sbom:
	@$(TOVEN) run sbom

## Regenerate workspace dependency-graph SVGs in docs/depgraphs (embedded in docs/DESIGN.md)
## The layer diagram is derived from `toven groups`, so rskit_tool owns the
## rendering (Toven orchestrates it as the `depgraphs` command task).
## Requires: cargo-depgraph, Graphviz (dot)
depgraphs:
	@$(TOVEN) run depgraphs

## Read-only Toven self-hosting canary: discover modules and the dependency
## graph, then render the mutation-free release previews (status + plan) with the
## candidate Toven binary. TOVEN selects the binary (see the TOVEN var).
## The release previews query crates.io for published-version idempotency, so a
## transient registry fault (SSL reset, dropped connection) is retried with
## backoff via scripts/retry.sh rather than failing the canary; the local
## discovery steps (modules, graph) need no network and run directly.
toven-canary:
	@$(TOVEN) modules
	@$(TOVEN) graph
	@./scripts/retry.sh $(TOVEN) release status
	@./scripts/retry.sh $(TOVEN) release plan

## Fast check: format + lint + build only (no tests) — for rapid iteration
check-fast: fmt-check lint build

## Run all checks (fmt + structure + lint + build + test + doctest + Python tooling)
check: fmt-check structure lint build test test-doc test-python

## Lint and test one module group (make check-<group>; see `toven groups`)
check-%:
	@$(TOVEN) run lint,test --group $* $(if $(JOBS),--jobs $(JOBS))

## Clean build artifacts
clean:
	@cd core && cargo clean
	@cd contrib && cargo clean
	@cd examples && cargo clean
	@rm -f coverage.lcov
	@rm -rf core/target/llvm-cov contrib/target/llvm-cov examples/target/llvm-cov
	@echo "✓ Cleaned"

## Ensure act is installed (for local CI)
ensure-act:
	@command -v act >/dev/null 2>&1 || { \
		echo "==> act not found. Install from https://github.com/nektos/act"; \
		exit 1; \
	}
	@command -v docker >/dev/null 2>&1 || { echo "Error: Docker is required but not installed." && exit 1; }

## Run full CI pipeline locally (mirrors GitHub Actions)
ci: ensure-act
	@act --secret GITHUB_TOKEN=$$(gh auth token 2>/dev/null) $(ACT_ARGS)

## Run only the test job from CI
ci-test: ensure-act
	@act -j test --secret GITHUB_TOKEN=$$(gh auth token 2>/dev/null) $(ACT_ARGS)

## Run only the lint job from CI
ci-lint: ensure-act
	@act -j clippy --secret GITHUB_TOKEN=$$(gh auth token 2>/dev/null) $(ACT_ARGS)

## Run only the fmt job from CI
ci-fmt: ensure-act
	@act -j fmt --secret GITHUB_TOKEN=$$(gh auth token 2>/dev/null) $(ACT_ARGS)

## Show help
help:
	@echo "Usage: make <target> [C=<crate>] [W=core|contrib|examples] [G=<group>] [T=<test>] [PROFILE=<profile>] [JOBS=<n>]"
	@echo ""
	@echo "Development:"
	@echo "  make help                                  Show this help"
	@echo "  make setup                                Install or verify local tooling"
	@echo "  make build              [C=] [W=] [G=]     Build"
	@echo "  make lock               [OFFLINE=1]        Refresh every workspace Cargo.lock (workspace members only)"
	@echo "  make test               [C=] [W=] [G=] [T=] [PROFILE=]  Run tests with nextest (all features)"
	@echo "  make test-default       [C=] [W=] [G=] [T=] [PROFILE=]  Run tests with default features"
	@echo "  make test-doc           [C=] [W=] [G=]     Run doctests only (all features)"
	@echo "  make test-python                          Run Python tooling tests"
	@echo "  make test-affected      [CHANGED_BASE=] [T=] [PROFILE=]  Test changed crates and dependents (all + default features)"
	@echo "  make coverage           [JOBS=] [THRESHOLD=]                            Run per-package coverage and gate via toven.toml"
	@echo "  make coverage-changed   [JOBS=] [THRESHOLD=] [CHANGED_BASE=]            Run coverage for crates changed vs the base ref"
	@echo "  make test-coverage                                                     Alias for coverage"
	@echo "  make lint               [C=] [W=] [G=]     Run clippy (all features and targets, -D warnings)"
	@echo "  make fmt                [C=] [W=] [G=]     Format code"
	@echo "  make fmt-check          [C=] [W=] [G=]     Check formatting"
	@echo "  make structure                            Declare-only aggregator guard + crowded-module advisory (CROWDED_MODULE_FILES=15)"
	@echo "  make doc                [C=] [W=] [G=]     Build documentation"
	@echo "  make check-l7-edges                       Check L7 dependency edges"
	@echo "  make check-workspace-deps-sync            Check shared core/contrib/examples dependency versions"
	@echo "  make check-topology                       Check module topology"
	@echo "  make check-public-api                     Check public API guardrails"
	@echo "  make check-facade-features                Check rskit facade feature combinations"
	@echo "  make deny               [W=]               Run cargo-deny + L7 edge checks"
	@echo "  make release-plan                         Preview the release plan (read-only)"
	@echo "  make release-status                       Report release status (read-only)"
	@echo "  make release-readiness                    Run final release-readiness sweep"
	@echo "  make release-coverage                     Run per-package release coverage thresholds"
	@echo "  make release-bump                         Phase 1: stage manifest bumps + README pins on clean main (branch after the bump) for a PR into main (no commit)"
	@echo "  make release-tag                          Phase 2 (after the bump PR merges): confirm the maintainer's umbrella tag exists at HEAD (existence only, not signature)"
	@echo "  make publish-dry-run                      Rehearse the crates.io publication in dependency order (read-only)"
	@echo "  make release-publish                      Publish crates to crates.io (idempotent, rate-aware)"
	@echo "  make release-sbom                         Generate CycloneDX SBOMs"
	@echo "  make depgraphs                            Regenerate docs dependency graphs (docs/depgraphs)"
	@echo "  make check-fast                           fmt + lint + build"
	@echo "  make check                                fmt + structure + lint + build + test + doctest + Python tests"
	@echo "  make check-<group>       [JOBS=]           Lint and test one module group (list: toven groups)"
	@echo "  make clean                                Remove build artifacts"
	@echo ""
	@echo "Local CI (GitHub Actions via act + Docker):"
	@echo "  make ci                                   Run full CI pipeline"
	@echo "  make ci-test                              Run only test job"
	@echo "  make ci-lint                              Run only lint job"
	@echo "  make ci-fmt                               Run only fmt job"
	@echo ""
	@echo "Selection (C=, W=, G=):"
	@echo "  C=rskit-errors        Target core crate"
	@echo "  C=rskit-storage-s3    Target contrib crate"
	@echo "  C=agent-demo          Target example crate"
	@echo "  W=contrib             Target one workspace"
	@echo "  G=transport           Target one module group"
	@echo ""
	@echo "Examples:"
	@echo "  make build                               Build every workspace"
	@echo "  make build W=examples                    Build examples workspace"
	@echo "  make test PROFILE=ci                     Run nextest with CI profile"
	@echo "  make test-affected                       Test only changed crates"
	@echo "  make test C=rskit-auth                   Test auth crate"
	@echo "  make test C=rskit-storage-s3             Test S3 contrib crate"
	@echo "  make lint C=rskit-errors                 Lint errors crate"
	@echo "  make check-transport                     Lint and test the transport group"
	@echo "  make coverage                            Coverage + gate for every workspace in one combined run"
	@echo "  make coverage-changed                    Coverage for crates changed vs origin/main"
