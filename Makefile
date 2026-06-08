.PHONY: all build test test-nextest test-doc test-affected test-coverage test-coverage-html lint fmt fmt-check check check-fast check-facade-features \
       check-core check-patterns check-crosscutting check-composition check-transport check-auth check-data check-ai \
       check-media check-infra doc deny check-l7-edges check-topology check-public-api release-readiness release-coverage \
       publish-dry-run release-sbom clean help ci ci-test ci-lint ci-fmt ensure-act

CORE_MANIFEST = core/Cargo.toml
CONTRIB_MANIFEST = contrib/Cargo.toml
EXAMPLES_MANIFEST = examples/Cargo.toml
WORKSPACE_MANIFESTS = $(CORE_MANIFEST) $(CONTRIB_MANIFEST)
FORMAT_MANIFESTS = $(WORKSPACE_MANIFESTS) $(EXAMPLES_MANIFEST)
TEST_THREADS ?= 1

# Test filter: pass -- $(T) when T is set
_T = $(if $(T),-- $(T))

## Default target
all: check

define run_cargo_target
	@if [ -n "$(C)" ]; then \
		cargo $(1) --manifest-path $(CORE_MANIFEST) -p $(C) $(2) 2>/dev/null || \
		cargo $(1) --manifest-path $(CONTRIB_MANIFEST) -p $(C) $(2) 2>/dev/null || \
		cargo $(1) --manifest-path $(EXAMPLES_MANIFEST) -p $(C) $(2); \
	elif [ -n "$(W)" ]; then \
		cargo $(1) --manifest-path $(W)/Cargo.toml --workspace $(2); \
	else \
		set -e; \
		for manifest in $(3); do \
			cargo $(1) --manifest-path $$manifest --workspace $(2); \
		done; \
	fi
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
	$(call run_cargo_target,build,,$(WORKSPACE_MANIFESTS))
	@echo "✓ Build succeeded"

## Run tests (C=<crate>, T=<test pattern>, W=core|contrib|examples)
test:
	@echo "==> Testing..."
	$(call run_cargo_target,test,$(_T),$(WORKSPACE_MANIFESTS))
	@echo "✓ Tests passed"

## Run tests using nextest (parallel, with retries in CI)
test-nextest:
	@echo "==> Running tests with nextest..."
ifeq ($(TEST_RUNNER),cargo-test)
	$(call run_cargo_target,test,-- --test-threads=$(TEST_THREADS),$(WORKSPACE_MANIFESTS))
else
	$(call run_cargo_target,nextest run,$(if $(PROFILE),--profile $(PROFILE)),$(WORKSPACE_MANIFESTS))
endif

## Run only doctests (nextest doesn't support doctests)
test-doc:
	@echo "==> Running doctests..."
	$(call run_cargo_target,test,--doc,$(WORKSPACE_MANIFESTS))

## Run tests only for crates affected by current changes
test-affected:
	@echo "==> Detecting affected crates..."
	@CHANGED=$$(git diff --name-only origin/main...HEAD 2>/dev/null || git diff --name-only HEAD~1 2>/dev/null); \
	if [ -z "$$CHANGED" ]; then \
		echo "No changes detected, running all tests"; \
		cargo nextest run --manifest-path $(CORE_MANIFEST) --workspace; \
		cargo nextest run --manifest-path $(CONTRIB_MANIFEST) --workspace; \
	elif echo "$$CHANGED" | grep -qE '^(Cargo\.(toml|lock)|rust-toolchain\.toml|\.cargo/|\.config/|core/Cargo\.toml|contrib/Cargo\.toml|examples/Cargo\.toml)'; then \
		echo "Root/workspace config changed, running all tests"; \
		cargo nextest run --manifest-path $(CORE_MANIFEST) --workspace; \
		cargo nextest run --manifest-path $(CONTRIB_MANIFEST) --workspace; \
	else \
		CRATES=$$(echo "$$CHANGED" | grep -E '\.(rs|toml)$$' | xargs -I{} dirname {} | sort -u | while read dir; do \
			d="$$dir"; \
			while [ "$$d" != "." ] && [ ! -f "$$d/Cargo.toml" ]; do d=$$(dirname "$$d"); done; \
			if [ -f "$$d/Cargo.toml" ] && [ "$$d" != "." ] && [ "$$d" != "core" ] && [ "$$d" != "contrib" ] && [ "$$d" != "examples" ]; then \
				grep -m1 '^name\s*=\s*"' "$$d/Cargo.toml" | sed 's/.*= *"\(.*\)"/\1/'; \
			fi; \
		done | sort -u); \
		if [ -z "$$CRATES" ]; then \
			echo "No crate changes detected, running all tests"; \
			cargo nextest run --manifest-path $(CORE_MANIFEST) --workspace; \
			cargo nextest run --manifest-path $(CONTRIB_MANIFEST) --workspace; \
		else \
			echo "Affected crates: $$CRATES"; \
			echo "$$CRATES" | while IFS= read -r crate; do \
				[ -n "$$crate" ] || continue; \
				cargo nextest run --manifest-path $(CORE_MANIFEST) -p "$$crate" 2>/dev/null || \
				cargo nextest run --manifest-path $(CONTRIB_MANIFEST) -p "$$crate" 2>/dev/null || \
				cargo nextest run --manifest-path $(EXAMPLES_MANIFEST) -p "$$crate"; \
			done; \
		fi; \
	fi

## Run tests with coverage (C=<crate>, T=<test pattern>)
## Requires: cargo install cargo-llvm-cov
test-coverage:
	@echo "==> Testing with coverage..."
	$(call run_cargo_target,llvm-cov,--lcov --output-path coverage.lcov $(_T),$(WORKSPACE_MANIFESTS))
	@echo "✓ Coverage report: coverage.lcov"

## Run tests with coverage HTML report
test-coverage-html:
	@echo "==> Testing with coverage (HTML)..."
	$(call run_cargo_target,llvm-cov,--html $(_T),$(WORKSPACE_MANIFESTS))
	@echo "✓ Coverage report generated"

## Run clippy linter (C=<crate>)
lint:
	@echo "==> Linting..."
	$(call run_cargo_target,clippy,--all-targets -- -D warnings,$(WORKSPACE_MANIFESTS))
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
	@./scripts/check-l7-edges.sh
	@echo "✓ L7 dependency edges OK"

## Run module topology checks
check-topology:
	@echo "==> Checking module topology..."
	@./scripts/check-topology.sh
	@echo "✓ Module topology OK"

## Check public API guardrails
check-public-api:
	@echo "==> Checking public API guardrails..."
	@./scripts/check-public-api.sh
	@echo "✓ Public API guardrails OK"

## Check facade feature combinations
check-facade-features:
	@echo "==> Checking rskit facade feature combinations..."
	@cargo check --manifest-path $(CORE_MANIFEST) -p rskit
	@cargo check --manifest-path $(CORE_MANIFEST) -p rskit --all-features
	@cargo check --manifest-path $(CORE_MANIFEST) -p rskit --features "server grpc http httpclient sse"
	@cargo check --manifest-path $(CORE_MANIFEST) -p rskit --features "auth authz security encryption di observability discovery"
	@cargo check --manifest-path $(CORE_MANIFEST) -p rskit --features "database cache cache-fs cache-redis messaging messaging-kafka messaging-nats messaging-rabbitmq storage storage-s3 storage-gcs vectorstore vectorstore-qdrant"
	@cargo check --manifest-path $(CORE_MANIFEST) -p rskit --features "dag chain process stateful version schema hook"
	@cargo check --manifest-path $(CORE_MANIFEST) -p rskit --features "genai prompt llm llm-openai llm-anthropic llm-ollama llm-gemini embedding inference inference-triton inference-vllm inference-tgi skill tool agent mcp media-full"
	@cargo check --manifest-path $(CORE_MANIFEST) -p rskit --features "cli git dataset bench"
	@echo "✓ Facade feature combinations OK"

## Run cargo-deny, public API, topology, and dependency edge checks
## Requires: cargo-deny, cargo-public-api, and a nightly rustdoc JSON toolchain
deny: check-l7-edges check-topology check-public-api
	@echo "==> Running cargo-deny..."
	@if [ -n "$(W)" ]; then \
		cargo deny --manifest-path $(W)/Cargo.toml check licenses advisories sources bans; \
	else \
		set -e; \
		for manifest in $(WORKSPACE_MANIFESTS); do \
			cargo deny --manifest-path $$manifest check licenses advisories sources bans; \
		done; \
	fi
	@echo "✓ cargo-deny passed"

## Run the release-readiness supply-chain and API sweep
## Requires: cargo-deny, cargo-audit
release-readiness: check
	@./scripts/check-release-readiness.sh

## Run release coverage gates (overall >=85%, per-crate >=80%, security crates >=85%)
## Requires: cargo-llvm-cov
release-coverage:
	@./scripts/check-coverage-thresholds.sh release

## Dry-run publishing all publishable crates in dependency order
publish-dry-run:
	@./scripts/publish-dry-run.sh --dry-run

## Generate CycloneDX SBOMs under target/sbom
## Requires: cargo-cyclonedx
release-sbom:
	@./scripts/generate-sbom.sh

## Fast check: format + lint + build only (no tests) — for rapid iteration
check-fast: fmt-check lint build

## Run all checks (fmt + lint + build + test)
check: fmt-check lint build test-nextest test-doc

## Check only core domain modules
check-core:
	@./scripts/check-domain.sh core

## Check only patterns domain modules
check-patterns:
	@./scripts/check-domain.sh patterns

## Check only crosscutting domain modules
check-crosscutting:
	@./scripts/check-domain.sh crosscutting

## Check only composition domain modules
check-composition:
	@./scripts/check-domain.sh composition

## Check only transport domain modules
check-transport:
	@./scripts/check-domain.sh transport

## Check only auth domain modules
check-auth:
	@./scripts/check-domain.sh auth

## Check only data domain modules
check-data:
	@./scripts/check-domain.sh data

## Check only ai domain modules
check-ai:
	@./scripts/check-domain.sh ai

## Check only media domain modules
check-media:
	@./scripts/check-domain.sh media

## Check only infra domain modules
check-infra:
	@./scripts/check-domain.sh infra

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
	@echo "Usage: make <target> [C=<crate>] [T=<test>] [PROFILE=<profile>] [W=core|contrib|examples]"
	@echo ""
	@echo "Development:"
	@echo "  make help                                  Show this help"
	@echo "  make build              [C=] [W=]          Build workspace(s)"
	@echo "  make test               [C=] [T=] [W=]     Run tests"
	@echo "  make test-nextest       [PROFILE=] [W=]    Run tests with nextest"
	@echo "  make test-doc           [C=] [W=]          Run doctests only"
	@echo "  make test-affected                        Run tests for changed crates"
	@echo "  make test-coverage      [C=] [T=] [W=]     Run tests with coverage (LCOV)"
	@echo "  make test-coverage-html [C=] [T=] [W=]     Run tests with coverage (HTML)"
	@echo "  make lint               [C=] [W=]          Run clippy"
	@echo "  make fmt                [W=]               Format code"
	@echo "  make fmt-check          [W=]               Check formatting"
	@echo "  make doc                [C=] [W=]          Build documentation"
	@echo "  make check-l7-edges                       Check L7 dependency edges"
	@echo "  make check-topology                       Check module topology"
	@echo "  make check-public-api                     Check public API guardrails"
	@echo "  make check-facade-features                Check rskit facade feature combinations"
	@echo "  make deny               [W=]               Run cargo-deny + L7 edge checks"
	@echo "  make release-readiness                    Run final release-readiness sweep"
	@echo "  make release-coverage                     Run release coverage thresholds"
	@echo "  make publish-dry-run                      Dry-run publishing in dependency order"
	@echo "  make release-sbom                         Generate CycloneDX SBOMs"
	@echo "  make check-fast                           fmt + lint + build"
	@echo "  make check              [C=] [W=]          fmt + lint + build + test"
	@echo "  make check-core                           Check only core domain modules"
	@echo "  make check-patterns                       Check only patterns domain modules"
	@echo "  make check-crosscutting                   Check only crosscutting domain modules"
	@echo "  make check-composition                    Check only composition domain modules"
	@echo "  make check-transport                      Check only transport domain modules"
	@echo "  make check-auth                           Check only auth domain modules"
	@echo "  make check-data                           Check only data domain modules"
	@echo "  make check-ai                             Check only ai domain modules"
	@echo "  make check-media                          Check only media domain modules"
	@echo "  make check-infra                          Check only infra domain modules"
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
	@echo "  make test-coverage-html                  Coverage report in browser"
