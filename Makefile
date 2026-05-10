.PHONY: all build test test-nextest test-doc test-affected test-coverage test-coverage-html lint fmt fmt-check check check-fast \
       check-core check-patterns check-crosscutting check-composition check-transport check-auth check-data check-ai \
       check-media check-infra doc deny hakari-verify check-l7-edges clean help ci ci-test ci-lint ci-fmt ensure-act

# Crate flag: pass -p $(C) to cargo when C is set
_C = $(if $(C),-p $(C))

# Test filter: pass -- $(T) when T is set
_T = $(if $(T),-- $(T))

## Default target
all: check

## Build workspace (C=<crate> for specific)
build:
	@echo "==> Building..."
	@cargo build --workspace $(_C)
	@echo "✓ Build succeeded"

## Run tests (C=<crate>, T=<test pattern>)
test:
	@echo "==> Testing..."
	@cargo test --workspace $(_C) $(_T)
	@echo "✓ Tests passed"

## Run tests using nextest (parallel, with retries in CI)
test-nextest:
	@echo "==> Running tests with nextest..."
	@cargo nextest run --workspace $(if $(PROFILE),--profile $(PROFILE))

## Run only doctests (nextest doesn't support doctests)
test-doc:
	@echo "==> Running doctests..."
	@cargo test --workspace --doc

## Run tests only for crates affected by current changes
test-affected:
	@echo "==> Detecting affected crates..."
	@CHANGED=$$(git diff --name-only origin/main...HEAD 2>/dev/null || git diff --name-only HEAD~1 2>/dev/null); \
	if [ -z "$$CHANGED" ]; then \
		echo "No changes detected, running all tests"; \
		cargo nextest run --workspace; \
	elif echo "$$CHANGED" | grep -qE '^(Cargo\.(toml|lock)|rust-toolchain\.toml|\.cargo/|\.config/)'; then \
		echo "Root/workspace config changed, running all tests"; \
		cargo nextest run --workspace; \
	else \
		CRATES=$$(echo "$$CHANGED" | grep -E '\.(rs|toml)$$' | xargs -I{} dirname {} | sort -u | while read dir; do \
			d="$$dir"; \
			while [ "$$d" != "." ] && [ ! -f "$$d/Cargo.toml" ]; do d=$$(dirname "$$d"); done; \
			if [ -f "$$d/Cargo.toml" ] && [ "$$d" != "." ]; then \
				grep -q '^\[package\]' "$$d/Cargo.toml" 2>/dev/null && grep -m1 'name' "$$d/Cargo.toml" | sed 's/.*= *"\(.*\)"/\1/'; \
			fi; \
		done | sort -u); \
		if [ -z "$$CRATES" ]; then \
			echo "No crate changes detected, running all tests"; \
			cargo nextest run --workspace; \
		else \
			echo "Affected crates: $$CRATES"; \
			PKGS=$$(echo "$$CRATES" | sed 's/^/-p /' | tr '\n' ' '); \
			cargo nextest run $$PKGS; \
		fi; \
	fi

## Run tests with coverage (C=<crate>, T=<test pattern>)
## Requires: cargo install cargo-llvm-cov
test-coverage:
	@echo "==> Testing with coverage..."
	@cargo llvm-cov --workspace --lcov --output-path coverage.lcov $(_C) $(_T)
	@echo "✓ Coverage report: coverage.lcov"

## Run tests with coverage HTML report
test-coverage-html:
	@echo "==> Testing with coverage (HTML)..."
	@cargo llvm-cov --workspace --html $(_C) $(_T)
	@echo "✓ Coverage report: target/llvm-cov/html/index.html"

## Run clippy linter (C=<crate>)
lint:
	@echo "==> Linting..."
	@cargo clippy --workspace --all-targets $(_C) -- -D warnings
	@echo "✓ Lint passed"

## Format code (C=<crate>)
fmt:
	@echo "==> Formatting..."
	@cargo fmt --all
	@echo "✓ Formatted"

## Check formatting without modifying files
fmt-check:
	@echo "==> Checking format..."
	@cargo fmt --all -- --check
	@echo "✓ Format OK"

## Build documentation
doc:
	@echo "==> Building docs..."
	@RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --document-private-items
	@echo "✓ Docs built"

## Run L7 dependency edge checks
check-l7-edges:
	@echo "==> Checking L7 dependency edges..."
	@./scripts/check-l7-edges.sh
	@echo "✓ L7 dependency edges OK"

## Run cargo-deny checks (licenses, advisories, sources, bans) and L7 edge checks
## Requires: cargo install cargo-deny
deny: check-l7-edges
	@echo "==> Running cargo-deny..."
	@cargo deny check licenses advisories sources bans
	@echo "✓ cargo-deny passed"

## Verify workspace-hack is up-to-date (requires cargo-hakari)
hakari-verify:
	@echo "==> Verifying workspace-hack..."
	@cargo hakari generate --diff
	@cargo hakari manage-deps --dry-run
	@echo "✓ workspace-hack is up-to-date"

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
	@cargo clean
	@rm -f coverage.lcov
	@rm -rf target/llvm-cov
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
	@echo "Usage: make <target> [C=<crate>] [T=<test>] [PROFILE=<profile>]"
	@echo ""
	@echo "Development:"
	@echo "  make help                                  Show this help"
	@echo "  make build              [C=]               Build workspace"
	@echo "  make test               [C=] [T=]          Run tests"
	@echo "  make test-nextest       [PROFILE=]         Run tests with nextest"
	@echo "  make test-doc                             Run doctests only"
	@echo "  make test-affected                        Run tests for changed crates"
	@echo "  make test-coverage      [C=] [T=]          Run tests with coverage (LCOV)"
	@echo "  make test-coverage-html [C=] [T=]          Run tests with coverage (HTML)"
	@echo "  make lint               [C=]               Run clippy"
	@echo "  make fmt                                  Format code"
	@echo "  make fmt-check                            Check formatting"
	@echo "  make doc                                  Build documentation"
	@echo "  make check-l7-edges                       Check L7 dependency edges"
	@echo "  make deny                                 Run cargo-deny + L7 edge checks"
	@echo "  make hakari-verify                        Verify workspace-hack is current"
	@echo "  make check-fast                           fmt + lint + build"
	@echo "  make check              [C=]               fmt + lint + build + test"
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
	@echo "  make ci                             Run full CI pipeline"
	@echo "  make ci-test                        Run only test job"
	@echo "  make ci-lint                        Run only lint job"
	@echo "  make ci-fmt                         Run only fmt job"
	@echo ""
	@echo "Crate targeting (C=):"
	@echo "  C=rskit-auth          Target auth crate"
	@echo "  C=rskit-http          Target http crate"
	@echo "  C=rskit-database      Target database crate"
	@echo "  C=rskit-messaging     Target messaging crate"
	@echo ""
	@echo "Examples:"
	@echo "  make test                              Test everything"
	@echo "  make test-nextest PROFILE=ci           Run nextest with CI profile"
	@echo "  make test-affected                     Test only changed crates"
	@echo "  make test C=rskit-auth                 Test auth crate"
	@echo "  make test C=rskit-auth T=jwt           Test matching tests in auth"
	@echo "  make lint C=rskit-http                 Lint http crate"
	@echo "  make check C=rskit-database            Full check on database crate"
	@echo "  make test-coverage-html                Coverage report in browser"
