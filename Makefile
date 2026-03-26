# Geo-stream — common Cargo and Docker workflows from the repo root.
# Run `make` or `make help` for targets.

.DEFAULT_GOAL := help

CLI_PKG   := cli
CLI_BIN   := geo-stream
HTTP_BIN  := geo-stream-http
NAPI_DIR  := crates/adapters/napi
SAMPLE    := examples/sample-input.ndjson
IMAGE     := geo-stream

.PHONY: help
help: ## List targets
	@echo "Geo-stream"
	@echo ""
	@grep -E '^[a-zA-Z0-9_.-]+:.*?## ' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "  %-22s %s\n", $$1, $$2}'

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

.PHONY: build-http
build-http: ## Build HTTP adapter binary (debug)
	cargo build -p $(CLI_PKG) --features http --bin $(HTTP_BIN)

.PHONY: run-http
run-http: build-http ## Run HTTP server on 0.0.0.0:8080 (debug build)
	./target/debug/$(HTTP_BIN) --listen 0.0.0.0:8080

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

.PHONY: clean
clean: ## Remove target/ and build artifacts
	cargo clean

.PHONY: docker-build
docker-build: ## docker build -f docker/Dockerfile -t geo-stream .
	docker build -f docker/Dockerfile -t $(IMAGE) .

.PHONY: docker-run
docker-run: ## Run image with sample NDJSON on stdin (requires docker-build)
	docker run --rm -i $(IMAGE) < $(SAMPLE)
