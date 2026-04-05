.PHONY: all build test test-coverage lint fmt check doc deny clean help \
       ci ci-test ci-lint ci-fmt ensure-act

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

## Run cargo-deny checks (licenses, advisories, sources, bans)
## Requires: cargo install cargo-deny
deny:
	@echo "==> Running cargo-deny..."
	@cargo deny check licenses advisories sources bans
	@echo "✓ cargo-deny passed"

## Run all checks (fmt + lint + build + test)
check: fmt-check lint build test

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
	@echo "Usage: make <target> [C=<crate>] [T=<test>]"
	@echo ""
	@echo "Development:"
	@echo "  make build              [C=]       Build workspace"
	@echo "  make test               [C=] [T=]  Run tests"
	@echo "  make test-coverage      [C=] [T=]  Run tests with coverage (LCOV)"
	@echo "  make test-coverage-html [C=] [T=]  Run tests with coverage (HTML)"
	@echo "  make lint               [C=]       Run clippy"
	@echo "  make fmt                            Format code"
	@echo "  make fmt-check                      Check formatting"
	@echo "  make doc                            Build documentation"
	@echo "  make deny                           Run cargo-deny checks"
	@echo "  make check              [C=]       fmt + lint + build + test"
	@echo "  make clean                          Remove build artifacts"
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
	@echo "  make test                            Test everything"
	@echo "  make test C=rskit-auth               Test auth crate"
	@echo "  make test C=rskit-auth T=jwt         Test matching tests in auth"
	@echo "  make lint C=rskit-http               Lint http crate"
	@echo "  make check C=rskit-database          Full check on database crate"
	@echo "  make test-coverage-html              Coverage report in browser"
