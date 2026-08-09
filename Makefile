.PHONY: all setup build test test-nextest test-doc test-python test-affected coverage coverage-changed test-coverage test-coverage-html lint fmt fmt-check check check-fast check-facade-features \
       check-core check-patterns check-crosscutting check-composition check-transport check-auth check-data check-ai \
       check-media check-infra doc deny check-l7-edges check-workspace-deps-sync check-topology check-public-api release-plan release-status release-readiness release-coverage \
       release-bump release-tag publish-dry-run release-publish release-sbom depgraphs clean help ci ci-test ci-lint ci-fmt ensure-act structure toven-canary lock

CORE_MANIFEST = core/Cargo.toml
CONTRIB_MANIFEST = contrib/Cargo.toml
EXAMPLES_MANIFEST = examples/Cargo.toml
FUZZ_MANIFEST = fuzz/Cargo.toml
PYTHON ?= python3
RSKIT_TOOL = $(PYTHON) ./scripts/rskit_tool.py
# Candidate Toven binary for the read-only self-hosting canary. Defaults to a
# `toven` on PATH; point it at a freshly built binary to dogfood a candidate,
# e.g. `make TOVEN=../toven/target/release/toven toven-canary`.
TOVEN ?= toven
WORKSPACE_MANIFESTS = $(CORE_MANIFEST) $(CONTRIB_MANIFEST) $(EXAMPLES_MANIFEST)
# Every workspace that owns a Cargo.lock, including the fuzz harness excluded
# from the build/test train.
LOCK_MANIFESTS = $(WORKSPACE_MANIFESTS) $(FUZZ_MANIFEST)
FORMAT_MANIFESTS = $(WORKSPACE_MANIFESTS)
TEST_THREADS ?= 1
THRESHOLD ?=
FEATURES ?=
TEST_FEATURES ?= --all-features

# Test filter: pass -- $(T) when T is set
_T = $(if $(T),-- $(T))

## Default target
all: check

## Install or verify local development tooling
setup:
	@PYTHON="$(PYTHON)" ./scripts/setup.sh

define run_cargo_target
	@if [ -n "$(C)" ]; then \
		manifest=$$($(RSKIT_TOOL) ci package-manifest "$(C)") || exit $$?; \
		cargo $(1) --manifest-path "$$manifest" -p $(C) $(4) $(2); \
	elif [ -n "$(W)" ]; then \
		cargo $(1) --manifest-path $(W)/Cargo.toml --workspace $(4) $(2); \
	else \
		set -e; \
		for manifest in $(3); do \
			cargo $(1) --manifest-path $$manifest --workspace $(4) $(2); \
		done; \
	fi
endef

define run_coverage_target
	@$(RSKIT_TOOL) coverage $(1) \
		$(if $(W),--workspace $(W)) \
		$(if $(C),--package $(C)) \
		$(if $(PACKAGES),--packages "$(PACKAGES)") \
		$(if $(JOBS),--jobs $(JOBS)) \
		$(if $(CLEAN),--clean $(CLEAN)) \
		$(if $(EXCLUDE_PACKAGES),--exclude-packages "$(EXCLUDE_PACKAGES)") \
		$(if $(THRESHOLD),--line-threshold $(THRESHOLD)) \
		$(if $(FUNCTION_THRESHOLD),--function-threshold $(FUNCTION_THRESHOLD)) \
		$(if $(REGION_THRESHOLD),--region-threshold $(REGION_THRESHOLD)) \
		$(if $(SECURITY_THRESHOLD),--security-line-threshold $(SECURITY_THRESHOLD)) \
		$(if $(PROGRESS_INTERVAL),--progress-interval $(PROGRESS_INTERVAL)) \
		$(if $(PROGRESS_STYLE),--progress-style $(PROGRESS_STYLE)) \
		$(if $(PROGRESS_WIDTH),--progress-width $(PROGRESS_WIDTH)) \
		$(if $(T),--test-filter "$(T)")
endef

define run_domain_target
	@$(RSKIT_TOOL) check domain $(1) \
		$(if $(JOBS),--jobs $(JOBS))
endef

define run_fmt_target
	@if [ -n "$(W)" ]; then \
		cargo fmt --manifest-path $(W)/Cargo.toml --all $(1); \
	else \
		set -e; \
		for manifest in $(FORMAT_MANIFESTS); do \
			cargo fmt --manifest-path $$manifest --all $(1); \
		done; \
	fi
endef

## Build workspace (C=<crate> for specific crate, W=core|contrib|examples for specific workspace)
build:
	@echo "==> Building..."
	$(call run_cargo_target,build,,$(WORKSPACE_MANIFESTS),$(FEATURES))
	@echo "✓ Build succeeded"

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

## Run tests (C=<crate>, T=<test pattern>, W=core|contrib|examples)
test:
	@echo "==> Testing..."
	$(call run_cargo_target,test,$(_T),$(WORKSPACE_MANIFESTS),$(or $(FEATURES),$(TEST_FEATURES)))
	@echo "✓ Tests passed"

## Run tests using nextest (parallel, with retries in CI)
test-nextest:
	@echo "==> Running tests with nextest..."
ifeq ($(TEST_RUNNER),cargo-test)
	$(call run_cargo_target,test,-- --test-threads=$(TEST_THREADS),$(WORKSPACE_MANIFESTS),$(or $(FEATURES),$(TEST_FEATURES)))
else
	$(call run_cargo_target,nextest run,$(if $(PROFILE),--profile $(PROFILE)),$(WORKSPACE_MANIFESTS),$(or $(FEATURES),$(TEST_FEATURES)))
endif

## Run only doctests (nextest doesn't support doctests)
test-doc:
	@echo "==> Running doctests..."
	$(call run_cargo_target,test,--doc,$(WORKSPACE_MANIFESTS),$(or $(FEATURES),$(TEST_FEATURES)))

## Run Python repository tooling tests
test-python:
	@echo "==> Running Python tooling tests..."
	@$(PYTHON) -m unittest discover -s scripts/tests -t .
	@echo "✓ Python tooling tests passed"

## Run tests only for crates affected by current changes
test-affected:
	@$(RSKIT_TOOL) ci test --scope changed --changed-base origin/main...HEAD --feature-mode both $(if $(PROFILE),--profile $(PROFILE),--profile ci)

## Run workspace-level coverage with per-package reporting (C=<crate>, PACKAGES=<list>, W=core|contrib|examples, JOBS=<n>, THRESHOLD=<pct>, T=<test pattern>)
## Requires: cargo install cargo-nextest cargo-llvm-cov
coverage:
	$(call run_coverage_target,)

## Alias for coverage
test-coverage: coverage

## Run workspace-level coverage for changed crates
coverage-changed:
	$(call run_coverage_target,--changed)

## Run tests with coverage HTML report
test-coverage-html:
	$(call run_coverage_target,--html)

## Run clippy linter (C=<crate>)
lint:
	@echo "==> Linting..."
	$(call run_cargo_target,clippy,--all-targets -- -D warnings,$(WORKSPACE_MANIFESTS),$(or $(FEATURES),--all-features))
	@echo "✓ Lint passed"

## Format code (W=core|contrib|examples)
fmt:
	@echo "==> Formatting..."
	$(call run_fmt_target,)
	@echo "✓ Formatted"

## Check formatting without modifying files
fmt-check:
	@echo "==> Checking format..."
	$(call run_fmt_target,-- --check)
	@echo "✓ Format OK"

## Declare-only aggregator guard (lib.rs / mod.rs) + crowded-module advisory
## Both gates run through Toven's `command` ecosystem (structure = ast-grep,
## crowded-modules = rskit_tool); ensure-ast-grep provisions the ast-grep tool.
structure:
	@echo "==> Checking declare-only aggregators (lib.rs / mod.rs)..."
	@./scripts/ensure-ast-grep.sh
	@$(TOVEN) run structure
	@echo "==> Scanning for crowded modules (advisory)..."
	@$(TOVEN) run crowded-modules

## Build documentation
doc:
	@echo "==> Building docs..."
	@if [ -n "$(C)" ]; then \
		RUSTDOCFLAGS="-D warnings" cargo doc --manifest-path $(CORE_MANIFEST) -p $(C) --no-deps --document-private-items 2>/dev/null || \
		RUSTDOCFLAGS="-D warnings" cargo doc --manifest-path $(CONTRIB_MANIFEST) -p $(C) --no-deps --document-private-items 2>/dev/null || \
		RUSTDOCFLAGS="-D warnings" cargo doc --manifest-path $(EXAMPLES_MANIFEST) -p $(C) --no-deps --document-private-items; \
	elif [ -n "$(W)" ]; then \
		RUSTDOCFLAGS="-D warnings" cargo doc --manifest-path $(W)/Cargo.toml --workspace --no-deps --document-private-items; \
	else \
		set -e; \
		for manifest in $(FORMAT_MANIFESTS); do \
			RUSTDOCFLAGS="-D warnings" cargo doc --manifest-path $$manifest --workspace --no-deps --document-private-items; \
		done; \
	fi
	@echo "✓ Docs built"

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

## Run release coverage gates (default per-package line coverage >=90%)
## Requires: cargo-nextest, cargo-llvm-cov
release-coverage:
	$(call run_coverage_target,--mode release)

## Phase 1 — bump: write per-crate manifest version bumps + dependency floors into
## the working tree and stage them, WITHOUT committing (stage-only is the default;
## `--yes` confirms the mutation). Run it on a release branch you created; you then
## commit and open the PR yourself. Toven never creates the branch, the commit, or
## the PR, and never tags/pushes/publishes here. Change-aware: only crates with a
## real diff since baseline bump, plus the dependency cascade; the configured
## on-resolved hook re-syncs the README install-snippet version pins in the same
## staged change.
release-bump:
	@$(TOVEN) release bump --yes

## Phase 2 — tag (run only after the release-bump PR merges into main): create and
## push the signed per-crate + umbrella tags on the merged commit. With
## `push_branch = false` (toven.toml) it pushes only tags, never a branch commit.
## Toven owns the tag; run release-publish for the crates.io publication.
release-tag:
	@$(TOVEN) release tag --yes

## Phase 3 (dry-run): rehearse the crates.io publication in dependency order — the
## per-crate would-publish / already-published verdicts — without mutating anything.
publish-dry-run:
	@$(TOVEN) release publish --dry-run

## Phase 3 — publish: verify the tags pushed in phase 2, then publish each crate to
## crates.io in dependency order (idempotent). Does not create commits or tags; CI
## runs this against the checked-out release tag. Run after the release-tag phase.
## Requires: CARGO_REGISTRY_TOKEN
release-publish:
	@$(TOVEN) release publish --yes

## Generate CycloneDX SBOMs under target/sbom across the all-features surface
## rskit_tool owns the all-features rendering (Toven orchestrates the `sbom` task).
## Requires: cargo-cyclonedx
release-sbom:
	@$(TOVEN) run sbom

## Regenerate workspace dependency-graph SVGs in docs/depgraphs (embedded in docs/DESIGN.md)
## The domain-layer diagram is derived from domains.toml, so rskit_tool owns the
## rendering (Toven orchestrates it as the `depgraphs` command task).
## Requires: cargo-depgraph, Graphviz (dot)
depgraphs:
	@$(TOVEN) run depgraphs

## Read-only Toven self-hosting canary: discover modules and the dependency
## graph, then render the mutation-free release previews (status + plan) with the
## candidate Toven binary. The native RSKIT_TOOL release path above stays
## authoritative until parity is proven; `release readiness` stays on that native
## path because its go/no-go verdict is a release gate (clean tree, registry
## idempotency), not a binary-contract signal. TOVEN selects the binary (see the
## TOVEN var).
toven-canary:
	@$(TOVEN) modules
	@$(TOVEN) graph
	@$(TOVEN) release status
	@$(TOVEN) release plan

## Fast check: format + lint + build only (no tests) — for rapid iteration
check-fast: fmt-check lint build

## Run all checks (fmt + lint + build + test)
check: fmt-check structure lint build test-nextest test-doc test-python

## Check only core domain modules
check-core:
	$(call run_domain_target,core)

## Check only patterns domain modules
check-patterns:
	$(call run_domain_target,patterns)

## Check only crosscutting domain modules
check-crosscutting:
	$(call run_domain_target,crosscutting)

## Check only composition domain modules
check-composition:
	$(call run_domain_target,composition)

## Check only transport domain modules
check-transport:
	$(call run_domain_target,transport)

## Check only auth domain modules
check-auth:
	$(call run_domain_target,auth)

## Check only data domain modules
check-data:
	$(call run_domain_target,data)

## Check only ai domain modules
check-ai:
	$(call run_domain_target,ai)

## Check only media domain modules
check-media:
	$(call run_domain_target,media)

## Check only infra domain modules
check-infra:
	$(call run_domain_target,infra)

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
	@echo "Usage: make <target> [C=<crate>] [T=<test>] [PROFILE=<profile>] [W=core|contrib|examples] [FEATURES=...]"
	@echo ""
	@echo "Development:"
	@echo "  make help                                  Show this help"
	@echo "  make setup                                Install or verify local tooling"
	@echo "  make build              [C=] [W=] [FEATURES=]                Build workspace(s)"
	@echo "  make lock               [OFFLINE=1]        Refresh every workspace Cargo.lock (workspace members only)"
	@echo "  make test               [C=] [T=] [W=] [FEATURES=]            Run tests; defaults to TEST_FEATURES=--all-features"
	@echo "  make test-nextest       [PROFILE=] [W=] [FEATURES=]           Run tests with nextest; defaults to TEST_FEATURES=--all-features"
	@echo "  make test-doc           [C=] [W=] [FEATURES=]                 Run doctests only; defaults to TEST_FEATURES=--all-features"
	@echo "  make test-python                          Run Python tooling tests"
	@echo "  make test-affected                        Run tests for changed crates"
	@echo "  make coverage           [C=] [PACKAGES=] [T=] [W=] [JOBS=] [CLEAN=] [THRESHOLD=] [PROGRESS_INTERVAL=] [PROGRESS_STYLE=]  Run workspace coverage with per-package reporting"
	@echo "  make coverage-changed   [T=] [JOBS=] [THRESHOLD=]                       Run coverage for changed crates"
	@echo "  make test-coverage      [C=] [T=] [W=]                                  Alias for coverage"
	@echo "  make test-coverage-html [C=] [T=] [W=]                                  Run coverage with HTML reports"
	@echo "  make lint               [C=] [W=] [FEATURES=]                 Run clippy; defaults to --all-features"
	@echo "  make fmt                [W=]               Format code"
	@echo "  make fmt-check          [W=]               Check formatting"
	@echo "  make structure                            Declare-only aggregator guard + crowded-module advisory (CROWDED_MODULE_FILES=15)"
	@echo "  make doc                [C=] [W=]          Build documentation"
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
	@echo "  make release-bump                         Phase 1: stage manifest bumps + README pins on a branch for a PR into main (no commit)"
	@echo "  make release-tag                          Phase 2 (after the bump PR merges): create + push signed tags on main"
	@echo "  make publish-dry-run                      Rehearse the crates.io publication in dependency order (read-only)"
	@echo "  make release-publish                      Publish crates to crates.io (idempotent, rate-aware)"
	@echo "  make release-sbom                         Generate CycloneDX SBOMs"
	@echo "  make depgraphs                            Regenerate docs dependency graphs (docs/depgraphs)"
	@echo "  make check-fast                           fmt + lint + build"
	@echo "  make check              [C=] [W=]          fmt + lint + build + test"
	@echo "  make check-core         [JOBS=]            Check only core domain modules"
	@echo "  make check-patterns     [JOBS=]            Check only patterns domain modules"
	@echo "  make check-crosscutting [JOBS=]            Check only crosscutting domain modules"
	@echo "  make check-composition  [JOBS=]            Check only composition domain modules"
	@echo "  make check-transport    [JOBS=]            Check only transport domain modules"
	@echo "  make check-auth         [JOBS=]            Check only auth domain modules"
	@echo "  make check-data         [JOBS=]            Check only data domain modules"
	@echo "  make check-ai           [JOBS=]            Check only ai domain modules"
	@echo "  make check-media        [JOBS=]            Check only media domain modules"
	@echo "  make check-infra        [JOBS=]            Check only infra domain modules"
	@echo "  make clean                                Remove build artifacts"
	@echo ""
	@echo "Local CI (GitHub Actions via act + Docker):"
	@echo "  make ci                                   Run full CI pipeline"
	@echo "  make ci-test                              Run only test job"
	@echo "  make ci-lint                              Run only lint job"
	@echo "  make ci-fmt                               Run only fmt job"
	@echo ""
	@echo "Crate targeting (C=):"
	@echo "  C=rskit-errors        Target core crate"
	@echo "  C=rskit-storage-s3    Target contrib crate"
	@echo "  C=agent-demo          Target example crate"
	@echo ""
	@echo "Examples:"
	@echo "  make build                               Build core + contrib workspaces"
	@echo "  make build W=examples                    Build examples workspace"
	@echo "  make test-nextest PROFILE=ci             Run nextest with CI profile"
	@echo "  make test-affected                       Test only changed crates"
	@echo "  make test C=rskit-auth                   Test auth crate"
	@echo "  make test C=rskit-storage-s3             Test S3 contrib crate"
	@echo "  make lint C=rskit-errors                 Lint errors crate"
	@echo "  make check W=core                        Full check on core workspace"
	@echo "  make coverage C=rskit-errors             Coverage for one crate"
	@echo "  make coverage W=core JOBS=4              Coverage for core crates"
	@echo "  make coverage-changed                    Coverage for changed crates"
