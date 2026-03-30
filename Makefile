# Geo-stream — common Cargo and Docker workflows from the repo root.
# Run `make` or `make help` for targets.

.DEFAULT_GOAL := help

CLI_PKG   := cli
CLI_BIN   := geo-stream
NAPI_DIR  := crates/adapters/napi
SAMPLE    := examples/sample-input.ndjson
IMAGE     := geo-stream

.PHONY: help
help: ## List targets
	@echo "Geo-stream"
	@echo ""
	@grep -E '^[a-zA-Z0-9_.-]+:.*?## ' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "  %-22s %s\n", $$1, $$2}'

.PHONY: init
init: install-hooks napi-install ## Bootstrap dev environment (hooks + npm deps)
	@rustup component add rustfmt clippy 2>/dev/null || true
	@cargo install cargo-edit 2>/dev/null || true
	@echo "Dev environment ready. Run 'make build' to compile."

.PHONY: check-prereqs
check-prereqs: ## Check that required tools are installed
	@command -v cargo >/dev/null || (echo "error: cargo not found"; exit 1)
	@command -v node >/dev/null || (echo "error: node not found"; exit 1)
	@cargo set-version --version >/dev/null 2>&1 || (echo "error: cargo-edit not installed — run: cargo install cargo-edit"; exit 1)

.PHONY: build
build: ## Build the workspace (debug)
	cargo build

.PHONY: build-release
build-release: ## Release build of the CLI binary (matches Dockerfile)
	cargo build --release -p $(CLI_PKG) --bin $(CLI_BIN)

.PHONY: check
check: ## Typecheck without producing binaries
	cargo check --workspace

.PHONY: test
test: ## Run all workspace tests
	cargo test

.PHONY: test-cli
test-cli: ## Run CLI crate tests (NDJSON integration / fixtures)
	cargo test -p $(CLI_PKG)

.PHONY: bench
bench: ## Criterion benchmarks for the engine (`process_batch` hot path)
	cargo bench -p engine

.PHONY: fmt
fmt: ## Format with rustfmt
	cargo fmt --all

.PHONY: fmt-check
fmt-check: ## Fail if code is not formatted
	cargo fmt --all -- --check

.PHONY: clippy
clippy: ## Lint with Clippy (workspace, all targets)
	cargo clippy --workspace --all-targets

.PHONY: run
run: ## Run CLI on examples/sample-input.ndjson
	cargo run -p $(CLI_PKG) --bin $(CLI_BIN) -- < $(SAMPLE)

.PHONY: run-batch
run-batch: ## Run CLI with --batch-size 0 on sample input
	cargo run -p $(CLI_PKG) --bin $(CLI_BIN) -- --batch-size 0 -- < $(SAMPLE)

.PHONY: napi-install
napi-install: ## Install npm dependencies for the NAPI adapter
	cd $(NAPI_DIR) && npm install

.PHONY: napi-build
napi-build: napi-install ## Build the NAPI native module (debug)
	cd $(NAPI_DIR) && npm run build:debug

.PHONY: napi-build-release
napi-build-release: napi-install ## Build the NAPI native module (release)
	cd $(NAPI_DIR) && npm run build

.PHONY: napi-typecheck
napi-typecheck: ## Type-check types.ts against the generated index.d.ts
	cd $(NAPI_DIR) && npm run typecheck

.PHONY: install-hooks
install-hooks: ## Install git pre-commit hook (auto-formats with cargo fmt)
	@cp scripts/hooks/pre-commit .git/hooks/pre-commit
	@chmod +x .git/hooks/pre-commit
	@echo "pre-commit hook installed"

.PHONY: bump-patch
bump-patch: check-prereqs ## Bump patch version (0.1.1 → 0.1.2), commit and tag locally
	@scripts/bump.sh patch

.PHONY: bump-minor
bump-minor: check-prereqs ## Bump minor version (0.1.1 → 0.2.0), commit and tag locally
	@scripts/bump.sh minor

.PHONY: bump-major
bump-major: check-prereqs ## Bump major version (0.1.1 → 2.0.0), commit and tag locally
	@scripts/bump.sh major

.PHONY: clean
clean: ## Remove target/ and build artifacts
	cargo clean

.PHONY: docker-build
docker-build: ## docker build -f docker/Dockerfile -t geo-stream .
	docker build -f docker/Dockerfile -t $(IMAGE) .

.PHONY: docker-run
docker-run: ## Run image with sample NDJSON on stdin (requires docker-build)
	docker run --rm -i $(IMAGE) < $(SAMPLE)
