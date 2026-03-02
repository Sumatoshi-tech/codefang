GOBIN ?= build/bin
GO111MODULE=on
ifneq ($(OS),Windows_NT)
EXE =
else
EXE = .exe
endif
PKG = $(shell go env GOOS)_$(shell go env GOARCH)
TAGS ?=

VERSION_PKG = github.com/Sumatoshi-tech/codefang/pkg/version
GIT_COMMIT  = $(shell git rev-parse --short HEAD 2>/dev/null || echo "none")
GIT_VERSION = $(shell git describe --tags --always --dirty 2>/dev/null || echo "dev")
BUILD_DATE  = $(shell date -u +%Y-%m-%dT%H:%M:%SZ)
LDFLAGS     = -X $(VERSION_PKG).Version=$(GIT_VERSION) -X $(VERSION_PKG).Commit=$(GIT_COMMIT) -X $(VERSION_PKG).Date=$(BUILD_DATE)
ifdef STATIC
LDFLAGS    += -extldflags=-static
endif

# libgit2 vendored build configuration
LIBGIT2_DIR := third_party/libgit2
LIBGIT2_BUILD := $(LIBGIT2_DIR)/build
LIBGIT2_INSTALL := $(LIBGIT2_DIR)/install
LIBGIT2_PKG_CONFIG := $(CURDIR)/$(LIBGIT2_INSTALL)/lib64/pkgconfig:$(CURDIR)/$(LIBGIT2_INSTALL)/lib/pkgconfig

all: precompile ${GOBIN}/uast${EXE} ${GOBIN}/codefang${EXE}

# Build all binaries (alias for all)
.PHONY: build
build: all

# Show help
.PHONY: help
help:
	@echo "Available targets:"
	@echo "  all              - Build all binaries (includes libgit2)"
	@echo "  build            - Build all binaries (alias for all)"
	@echo "  libgit2          - Build vendored libgit2 statically (auto-built by 'all')"
	@echo "  install          - Install binaries to system PATH"
	@echo "  test             - Run all tests"
	@echo "  lint             - Run linters and deadcode analysis"
	@echo "  fmt              - Format code"
	@echo "  schemas          - Generate JSON schemas for all analyzers"
	@echo "  deadcode         - Run deadcode analysis with detailed output"
	@echo "  deadcode-prod    - Run deadcode analysis excluding tests"
	@echo "  deadcode-why     - Show why a function is not dead (FUNC=name)"
	@echo "  bench            - Run UAST performance benchmarks"
	@echo "  perf             - Run burndown perf baseline (1k + 15k, CPU profiles). REPO=path (default: .)"
	@echo "  deps-update-*    - Update libgit2/tree-sitter third-party dependencies"
	@echo "  battle           - Battle test on large repo with CPU+heap profiles. BATTLE_REPO=path BATTLE_ANALYZER=burndown"
	@echo "  uast-dev         - Start UAST development environment (frontend + backend)"
	@echo "  uast-dev-stop    - Stop UAST development servers"
	@echo "  uast-dev-status  - Check status of UAST development servers"
	@echo "  uast-test        - Run UI tests for UAST development service"
	@echo "  dev-service      - Start backend only (legacy)"
	@echo "  compare-burndown - Run burndown and compare with Hercules reference (REPO=~/sources/iortcw, FP=1 for --first-parent)"
	@echo "  clean            - Clean build artifacts"
	@echo ""
	@echo "Variables:"
	@echo "  STATIC=1         - Build fully-static binaries (requires musl/Alpine or static libs)"

# Pre-compile UAST mappings for faster startup
precompile: libgit2
	@echo "Pre-compiling UAST mappings..."
	@mkdir -p ${GOBIN}
	PKG_CONFIG_PATH=$(LIBGIT2_PKG_CONFIG) \
	CGO_CFLAGS="-I$(CURDIR)/$(LIBGIT2_INSTALL)/include" \
	CGO_LDFLAGS="-L$(CURDIR)/$(LIBGIT2_INSTALL)/lib64 -L$(CURDIR)/$(LIBGIT2_INSTALL)/lib -lgit2 -lpthread" \
	CGO_ENABLED=1 go run ./tools/precompgen/precompile.go -o pkg/uast/embedded_mappings.gen.go

# Generate UAST mappings for all languages
uastmaps-gen:
	@echo "Generating UAST mappings..."
	@python3 tools/uastmapsgen/gen_uastmaps.py

# Generate JSON schemas for all analyzers
.PHONY: schemas
schemas: libgit2
	@echo "Generating JSON schemas..."
	@PKG_CONFIG_PATH=$(LIBGIT2_PKG_CONFIG) \
	CGO_CFLAGS="-I$(CURDIR)/$(LIBGIT2_INSTALL)/include" \
	CGO_LDFLAGS="-L$(CURDIR)/$(LIBGIT2_INSTALL)/lib64 -L$(CURDIR)/$(LIBGIT2_INSTALL)/lib -lgit2 -lpthread" \
	CGO_ENABLED=1 go run ./tools/schemagen/schemagen.go -o docs/schemas
	@echo "✓ Schemas generated in docs/schemas/"

# Install binaries to user's local bin directory
install: all
	@echo "Installing uast and codefang binaries..."
	@mkdir -p ~/.local/bin
	@cp ${GOBIN}/uast${EXE} ~/.local/bin/
	@cp ${GOBIN}/codefang${EXE} ~/.local/bin/
	@echo "Installed to ~/.local/bin"
	@if ! echo $$PATH | grep -q "$$HOME/.local/bin"; then \
		echo ""; \
		echo "Note: ~/.local/bin is not in your PATH"; \
		echo "Add it by running: export PATH=\"\$$HOME/.local/bin:\$$PATH\""; \
		echo "Or add the above line to your ~/.bashrc or ~/.zshrc"; \
	fi

# Run all tests
test: all
	PKG_CONFIG_PATH=$(LIBGIT2_PKG_CONFIG) \
	CGO_CFLAGS="-I$(CURDIR)/$(LIBGIT2_INSTALL)/include" \
	CGO_LDFLAGS="-L$(CURDIR)/$(LIBGIT2_INSTALL)/lib64 -L$(CURDIR)/$(LIBGIT2_INSTALL)/lib -lgit2 -lpthread" \
	CGO_ENABLED=1 go test ./...

# Run all tests with verbose output
testv: all
	PKG_CONFIG_PATH=$(LIBGIT2_PKG_CONFIG) \
	CGO_CFLAGS="-I$(CURDIR)/$(LIBGIT2_INSTALL)/include" \
	CGO_LDFLAGS="-L$(CURDIR)/$(LIBGIT2_INSTALL)/lib64 -L$(CURDIR)/$(LIBGIT2_INSTALL)/lib -lgit2 -lpthread" \
	CGO_ENABLED=1 go test ./... -v

# Run UAST performance benchmarks (comprehensive suite with organized results)
bench: all
	python3 tools/benchmark/benchmark_runner.py

# Burndown perf baseline: 1k + 15k commits with CPU profiles. REPO=path (default: .)
# Produces cpu_1k.prof, cpu_15k.prof; run from repo root.
perf: all
	@REPO=$${REPO:-.}; \
	echo "Perf repo: $$REPO"; \
	echo "Running 1k commits..."; \
	time $(GOBIN)/codefang run -a history/burndown --format yaml --limit 1000 --cpuprofile=cpu_1k.prof $$REPO > /tmp/out_1k.yaml 2>&1; \
	echo "Running 15k commits..."; \
	time $(GOBIN)/codefang run -a history/burndown --format yaml --limit 15000 --cpuprofile=cpu_15k.prof $$REPO > /tmp/out_15k.yaml 2>&1; \
	echo "Profiles: cpu_1k.prof, cpu_15k.prof"; \
	go tool pprof -text -diff_base=cpu_1k.prof cpu_15k.prof > pprof_diff.txt 2>/dev/null || true; \
	echo "Diff: pprof_diff.txt"

# Same as perf; writes to cpu_*_treap.prof for backward compatibility (treap is default).
perf-treap: all
	@REPO=$${REPO:-.}; \
	echo "Perf repo: $$REPO"; \
	echo "Running 1k commits..."; \
	time $(GOBIN)/codefang run -a history/burndown --format yaml --limit 1000 --cpuprofile=cpu_1k_treap.prof $$REPO > /tmp/out_1k_treap.yaml 2>&1; \
	echo "Running 15k commits..."; \
	time $(GOBIN)/codefang run -a history/burndown --format yaml --limit 15000 --cpuprofile=cpu_15k_treap.prof $$REPO > /tmp/out_15k_treap.yaml 2>&1; \
	echo "Profiles: cpu_1k_treap.prof, cpu_15k_treap.prof"; \
	go tool pprof -text -diff_base=cpu_1k_treap.prof cpu_15k_treap.prof > pprof_diff_treap.txt 2>/dev/null || true; \
	echo "Diff: pprof_diff_treap.txt"

# Battle test: full run on large repo with CPU+heap profiles and /usr/bin/time metrics.
# Usage: make battle [BATTLE_REPO=~/sources/kubernetes] [BATTLE_ANALYZER=burndown] [BATTLE_LIMIT=0]
.PHONY: battle
battle: all
	@REPO=$${BATTLE_REPO:-$$HOME/sources/kubernetes}; \
	ANALYZER=$${BATTLE_ANALYZER:-burndown}; \
	LIMIT=$${BATTLE_LIMIT:-0}; \
	STAMP=$$(date +%Y%m%d-%H%M%S); \
	DIR=profiles/$$(basename $$REPO)/$$STAMP; \
	mkdir -p $$DIR; \
	echo "Battle test: $$REPO ($$ANALYZER)"; \
	echo "Output dir: $$DIR"; \
	LIMIT_FLAG=""; \
	if [ "$$LIMIT" != "0" ]; then LIMIT_FLAG="--limit $$LIMIT"; fi; \
	/usr/bin/time -v $(GOBIN)/codefang run \
		-a history/$$ANALYZER --format yaml --first-parent \
		--cpuprofile=$$DIR/cpu.prof \
		--heapprofile=$$DIR/heap.prof \
		$$LIMIT_FLAG \
		$$REPO > $$DIR/output.yaml 2> $$DIR/time.txt; \
	echo "--- Profile summaries ---"; \
	go tool pprof -top -cum $$DIR/cpu.prof 2>/dev/null | head -25 > $$DIR/cpu_top.txt || true; \
	go tool pprof -top -inuse_space $$DIR/heap.prof 2>/dev/null | head -25 > $$DIR/heap_top.txt || true; \
	go tool pprof -svg $$DIR/cpu.prof > $$DIR/cpu_flamegraph.svg 2>/dev/null || true; \
	go tool pprof -svg -inuse_space $$DIR/heap.prof > $$DIR/heap_flamegraph.svg 2>/dev/null || true; \
	echo "Done. Artifacts in $$DIR/"; \
	cat $$DIR/time.txt | grep -E "(wall clock|Maximum resident|Percent of CPU)" || true

# Run basic Go benchmarks directly (no organization)
bench-basic: all
	CGO_ENABLED=1 go test -run="^$$" -bench=. -benchmem ./pkg/uast

# Run UAST performance benchmarks with verbose output
benchv: all
	CGO_ENABLED=1 go test -run="^$$" -bench=. -benchmem -v ./pkg/uast

# Run UAST performance benchmarks with CPU profiling
benchcpu: all
	CGO_ENABLED=1 go test -run="^$$" -bench=. -benchmem -cpuprofile=cpu.prof ./pkg/uast

# Run UAST performance benchmarks with memory profiling
benchmem: all
	CGO_ENABLED=1 go test -run="^$$" -bench=. -benchmem -memprofile=mem.prof ./pkg/uast

# Run UAST performance benchmarks with both CPU and memory profiling
benchprofile: all
	CGO_ENABLED=1 go test -run="^$$" -bench=. -benchmem -cpuprofile=cpu.prof -memprofile=mem.prof ./pkg/uast

# Run benchmarks and generate performance plots
benchplot: all
	CGO_ENABLED=1 go test -run="^$$" -bench=. -benchmem ./pkg/uast > test/benchmarks/benchmark_results.txt 2>&1
	python3 tools/benchmark/benchmark_plot.py test/benchmarks/benchmark_results.txt

# Run benchmarks with verbose output and generate plots
benchplotv: all
	CGO_ENABLED=1 go test -run="^$$" -bench=. -benchmem -v ./pkg/uast > test/benchmarks/benchmark_results.txt 2>&1
	python3 tools/benchmark/benchmark_plot.py test/benchmarks/benchmark_results.txt

# Run comprehensive benchmark suite with organized results
bench-suite: bench

# Run benchmarks without plots
bench-no-plots: all
	python3 tools/benchmark/benchmark_runner.py --no-plots

# Generate report for latest benchmark run
report:
	python3 tools/benchmark/benchmark_report.py

# Generate report for specific run
report-run:
	python3 tools/benchmark/benchmark_report.py $(RUN_NAME)

# List all benchmark runs
bench-list:
	python3 tools/benchmark/benchmark_report.py --list

# Compare latest run with previous
compare:
	python3 tools/benchmark/benchmark_comparison.py $(shell python3 tools/benchmark/benchmark_report.py --list | head -1)

# Compare specific runs
compare-runs:
	python3 tools/benchmark/benchmark_comparison.py $(CURRENT_RUN) --baseline $(BASELINE_RUN)

# Run Codefang burndown and compare with Hercules reference.
# Usage: make compare-burndown [REPO=~/sources/iortcw] [FP=1 for --first-parent]
.PHONY: compare-burndown
compare-burndown: build
	@REPO=$${REPO:-$$HOME/sources/iortcw}; \
	FP=$${FP:-0}; \
	$(GOBIN)/codefang run -a history/burndown --format yaml $$([ "$$FP" = "1" ] && echo --first-parent) "$$REPO" 2>/dev/null > /tmp/codefang_burndown.yaml; \
	python3 references/compare_burndown.py /tmp/codefang_burndown.yaml references/iortcw_burndown.yaml

# Compare last N benchmark runs (usage: make compare-last N=3)
compare-last:
	@N=$${N:-2}; \
	echo "Comparing last $$N benchmark runs..."; \
	if [ $$N -eq 2 ]; then \
		LATEST=$$(python3 tools/benchmark/benchmark_report.py --list | grep -v "Available benchmark runs:" | head -1 | xargs); \
		PREVIOUS=$$(python3 tools/benchmark/benchmark_report.py --list | grep -v "Available benchmark runs:" | head -2 | tail -1 | xargs); \
		echo "Latest: $$LATEST"; \
		echo "Previous: $$PREVIOUS"; \
		python3 tools/benchmark/benchmark_comparison.py "$$LATEST" --baseline "$$PREVIOUS"; \
	else \
		RUNS=$$(python3 tools/benchmark/benchmark_report.py --list | grep -v "Available benchmark runs:" | head -$$N | tr '\n' ' '); \
		echo "Comparing $$N runs:"; \
		echo "$$RUNS" | nl; \
		python3 tools/benchmark/benchmark_comparison_multi.py "$$RUNS"; \
	fi

# Run benchmarks with profiling and generate plots
benchplotprofile: all
	CGO_ENABLED=1 go test -run="^$$" -bench=. -benchmem -cpuprofile=cpu.prof -memprofile=mem.prof ./pkg/uast > test/benchmarks/benchmark_results.txt 2>&1
	python3 tools/benchmark/benchmark_plot.py test/benchmarks/benchmark_results.txt

# Run specific benchmark and generate plots (usage: make benchplot-simple BENCH=BenchmarkParse)
benchplot-simple: all
	CGO_ENABLED=1 go test -run="^$$" -bench=$(BENCH) -benchmem ./pkg/uast > test/benchmarks/benchmark_results.txt 2>&1
	python3 tools/benchmark/benchmark_plot.py test/benchmarks/benchmark_results.txt

# Run benchmarks with timeout and generate plots (usage: make benchplot-timeout TIMEOUT=30s)
benchplot-timeout: all
	CGO_ENABLED=1 go test -run="^$$" -bench=. -benchmem -timeout=$(TIMEOUT) ./pkg/uast > test/benchmarks/benchmark_results.txt 2>&1
	python3 tools/benchmark/benchmark_plot.py test/benchmarks/benchmark_results.txt

clean:
	rm -f ./uast
	rm -f ./codefang
	rm -rf ${GOBIN}/
	rm -f *.prof
	rm -f test/benchmarks/benchmark_results.txt
	rm -rf benchmark_plots/
	rm -rf profiles/
	rm -rf $(LIBGIT2_BUILD)
	rm -rf $(LIBGIT2_INSTALL)
	rm -rf bin/

# Linting tools (resolve from GOPATH/bin if not on PATH)
GOPATH_BIN=$(shell go env GOPATH)/bin
GOLINT=$(or $(shell command -v golangci-lint 2>/dev/null),$(GOPATH_BIN)/golangci-lint)
DEADCODE=$(or $(shell command -v deadcode 2>/dev/null),$(GOPATH_BIN)/deadcode)

# Package paths for linting
INTERNAL_PKGS=./cmd/... ./pkg/... ./internal/...
DEADCODE_PKGS=./cmd/... ./pkg/... ./internal/...
LINT_GOCACHE?=/tmp/codefang-go-cache
LINT_GOLANGCI_CACHE?=/tmp/codefang-golangci-lint-cache

## lint: Run linters and deadcode analysis
.PHONY: lint
lint:
	@echo "Running linters..."
	@mkdir -p $(LINT_GOCACHE) $(LINT_GOLANGCI_CACHE)
	@PKG_CONFIG_PATH=$(LIBGIT2_PKG_CONFIG) \
	GOCACHE=$(LINT_GOCACHE) \
	GOLANGCI_LINT_CACHE=$(LINT_GOLANGCI_CACHE) \
	CGO_CFLAGS="-I$(CURDIR)/$(LIBGIT2_INSTALL)/include" \
	CGO_LDFLAGS="-L$(CURDIR)/$(LIBGIT2_INSTALL)/lib64 -L$(CURDIR)/$(LIBGIT2_INSTALL)/lib -lgit2 -lpthread" \
	CGO_ENABLED=1 $(GOLINT) run $(INTERNAL_PKGS)
	@echo "Running deadcode analysis (production)..."
	@GOCACHE=$(LINT_GOCACHE) ./scripts/deadcode-filter.sh $(DEADCODE_PKGS)
	@echo "✓ Linting complete"

## deadcode: Run deadcode analysis with whitelist filter (fails if dead code found)
.PHONY: deadcode
deadcode:
	@echo "Running deadcode analysis with whitelist..."
	@GOCACHE=$(LINT_GOCACHE) ./scripts/deadcode-filter.sh -test $(DEADCODE_PKGS)

## deadcode-prod: Run deadcode analysis excluding tests (production-only dead code)
.PHONY: deadcode-prod
deadcode-prod:
	@echo "Running deadcode analysis (production only)..."
	@echo "================================================================"
	@GOCACHE=$(LINT_GOCACHE) ./scripts/deadcode-filter.sh $(DEADCODE_PKGS)

## deadcode-test-only: Find functions used only by tests (requires jq)
.PHONY: deadcode-test-only
deadcode-test-only:
	@echo "Finding functions that are only used by tests..."
	@echo "This compares deadcode results with and without tests..."
	@mkdir -p .deadcode-tmp
	@GOCACHE=$(LINT_GOCACHE) $(DEADCODE) -json $(DEADCODE_PKGS) | jq -r '.[] | .Funcs[].Name' | sort > .deadcode-tmp/dead_prod.txt
	@GOCACHE=$(LINT_GOCACHE) $(DEADCODE) -test -json $(DEADCODE_PKGS) | jq -r '.[] | .Funcs[].Name' | sort > .deadcode-tmp/dead_with_tests.txt
	@echo "Functions used only by tests:"
	@comm -23 .deadcode-tmp/dead_prod.txt .deadcode-tmp/dead_with_tests.txt || echo "No test-only functions found"
	@rm -rf .deadcode-tmp

## deadcode-json: Run deadcode analysis with JSON output
.PHONY: deadcode-json
deadcode-json:
	@echo "Running deadcode analysis (JSON output)..."
	@GOCACHE=$(LINT_GOCACHE) $(DEADCODE) -json $(DEADCODE_PKGS)

## deadcode-why: Show why a function is not dead (usage: make deadcode-why FUNC=functionName)
.PHONY: deadcode-why
deadcode-why:
	@if [ -z "$(FUNC)" ]; then \
		echo "Usage: make deadcode-why FUNC=functionName"; \
		echo "Example: make deadcode-why FUNC=bytes.Buffer.String"; \
		exit 1; \
	fi
	@echo "Analyzing why $(FUNC) is considered live..."
	@GOCACHE=$(LINT_GOCACHE) $(DEADCODE) -whylive="$(FUNC)" $(DEADCODE_PKGS)

## fmt: Format code
.PHONY: fmt
fmt:
	@echo "Formatting code..."
	@go fmt $(INTERNAL_PKGS)
	@echo "✓ Formatting complete"

# Stop UAST development servers
.PHONY: uast-dev-stop
uast-dev-stop:
	@echo "Stopping UAST development servers..."
	@if [ -f web/.vite.pid ]; then \
		kill $$(cat web/.vite.pid) 2>/dev/null || true; \
		rm -f web/.vite.pid; \
	fi
	@if [ -f web/.backend.pid ]; then \
		kill $$(cat web/.backend.pid) 2>/dev/null || true; \
		rm -f web/.backend.pid; \
	fi
	@echo "Servers stopped."

# Check UAST development server status
.PHONY: uast-dev-status
uast-dev-status:
	@echo "UAST Development Server Status:"
	@if curl -s http://localhost:3000 >/dev/null 2>&1; then \
		echo "✓ Frontend (Vite) running on http://localhost:3000"; \
	else \
		echo "✗ Frontend (Vite) not running"; \
	fi
	@if curl -s http://localhost:8080 >/dev/null 2>&1; then \
		echo "✓ Backend (Go) running on http://localhost:8080"; \
	else \
		echo "✗ Backend (Go) not running"; \
	fi

# Run UI tests
.PHONY: uast-test
uast-test:
	@echo "Running UAST UI Tests..."
	@cd web && npm test -- tests/simple.spec.js tests/basic.spec.js

${GOBIN}/uast${EXE}: cmd/uast/*.go pkg/uast/*.go pkg/uast/*/*.go pkg/uast/*/*/*.go
	PKG_CONFIG_PATH=$(LIBGIT2_PKG_CONFIG) \
	CGO_CFLAGS="-I$(CURDIR)/$(LIBGIT2_INSTALL)/include" \
	CGO_LDFLAGS="-L$(CURDIR)/$(LIBGIT2_INSTALL)/lib64 -L$(CURDIR)/$(LIBGIT2_INSTALL)/lib -lgit2 -lpthread" \
	CGO_ENABLED=1 go build -tags "$(TAGS)" -ldflags "$(LDFLAGS)" -o ${GOBIN}/uast${EXE} ./cmd/uast

${GOBIN}/codefang${EXE}: cmd/codefang/*.go cmd/codefang/commands/*.go internal/analyzers/*/*.go libgit2
	PKG_CONFIG_PATH=$(LIBGIT2_PKG_CONFIG) \
	CGO_CFLAGS="-I$(CURDIR)/$(LIBGIT2_INSTALL)/include" \
	CGO_LDFLAGS="-L$(CURDIR)/$(LIBGIT2_INSTALL)/lib64 -L$(CURDIR)/$(LIBGIT2_INSTALL)/lib -lgit2 -lpthread" \
	CGO_ENABLED=1 go build -tags "$(TAGS)" -ldflags "$(LDFLAGS)" -o ${GOBIN}/codefang${EXE} ./cmd/codefang

# Build the development service
.PHONY: build-dev-service
build-dev-service:
	@echo "Building UAST Development Service..."
	@cd web && go build -o ../build/uast-dev-service main.go

# Start UAST Development Environment (Frontend + Backend)
.PHONY: uast-dev
uast-dev: install
	@cd web && ./start-dev.sh

# =============================================================================
# libgit2 targets for fast history analysis
# =============================================================================

# Third-party update automation.
# Examples:
#   make deps-update-libgit2 LIBGIT2_REF=v1.9.1
#   make deps-update-treesitter TREE_SITTER_BARE_REF=v1.11.0 SITTER_FOREST_REF=v1.9.163 SITTER_FOREST_GO_REF=v1.9.4
#   make deps-update-all LIBGIT2_REF=v1.9.1 TREE_SITTER_BARE_REF=v1.11.0 SITTER_FOREST_REF=v1.9.163 SITTER_FOREST_GO_REF=v1.9.4
.PHONY: deps-update-libgit2
deps-update-libgit2:
	@if [ -z "$(LIBGIT2_REF)" ]; then \
		echo "Usage: make deps-update-libgit2 LIBGIT2_REF=<tag-or-sha>"; \
		exit 1; \
	fi
	@./scripts/update-third-party.sh --mode libgit2 --libgit2-ref "$(LIBGIT2_REF)"

.PHONY: deps-update-treesitter
deps-update-treesitter:
	@if [ -z "$(TREE_SITTER_BARE_REF)" ] || [ -z "$(SITTER_FOREST_REF)" ] || [ -z "$(SITTER_FOREST_GO_REF)" ]; then \
		echo "Usage: make deps-update-treesitter TREE_SITTER_BARE_REF=<ref> SITTER_FOREST_REF=<ref> SITTER_FOREST_GO_REF=<ref>"; \
		exit 1; \
	fi
	@./scripts/update-third-party.sh --mode treesitter \
		--tree-sitter-bare-ref "$(TREE_SITTER_BARE_REF)" \
		--sitter-forest-ref "$(SITTER_FOREST_REF)" \
		--sitter-forest-go-ref "$(SITTER_FOREST_GO_REF)"

.PHONY: deps-update-all
deps-update-all:
	@if [ -z "$(LIBGIT2_REF)" ] || [ -z "$(TREE_SITTER_BARE_REF)" ] || [ -z "$(SITTER_FOREST_REF)" ] || [ -z "$(SITTER_FOREST_GO_REF)" ]; then \
		echo "Usage: make deps-update-all LIBGIT2_REF=<ref> TREE_SITTER_BARE_REF=<ref> SITTER_FOREST_REF=<ref> SITTER_FOREST_GO_REF=<ref>"; \
		exit 1; \
	fi
	@./scripts/update-third-party.sh --mode all \
		--libgit2-ref "$(LIBGIT2_REF)" \
		--tree-sitter-bare-ref "$(TREE_SITTER_BARE_REF)" \
		--sitter-forest-ref "$(SITTER_FOREST_REF)" \
		--sitter-forest-go-ref "$(SITTER_FOREST_GO_REF)"

# =============================================================================
# OpenTelemetry local development stack
# =============================================================================

OTEL_COMPOSE := dev/docker-compose.otel.yml

# Detect compose command: docker compose (plugin) > docker-compose (standalone) > podman compose.
COMPOSE_CMD := $(shell \
	if command -v docker >/dev/null 2>&1 && docker compose version >/dev/null 2>&1; then \
		echo "docker compose"; \
	elif command -v docker-compose >/dev/null 2>&1; then \
		echo "docker-compose"; \
	elif command -v podman-compose >/dev/null 2>&1; then \
		echo "podman-compose"; \
	elif command -v podman >/dev/null 2>&1 && podman compose version >/dev/null 2>&1; then \
		echo "podman compose"; \
	fi \
)

# Start local OTel stack (Jaeger + Collector + Prometheus).
.PHONY: otel-up
otel-up:
ifeq ($(COMPOSE_CMD),)
	$(error No compose tool found. Install one of: docker-compose, podman-compose, or the docker compose plugin)
endif
	@echo "Starting OTel development stack ($(COMPOSE_CMD))..."
	$(COMPOSE_CMD) -f $(OTEL_COMPOSE) up -d
	@echo "Jaeger UI:   http://localhost:16686"
	@echo "Prometheus:  http://localhost:9090"
	@echo "OTLP gRPC:   localhost:4317"

# Stop local OTel stack.
.PHONY: otel-down
otel-down:
ifeq ($(COMPOSE_CMD),)
	$(error No compose tool found. Install one of: docker-compose, podman-compose, or the docker compose plugin)
endif
	$(COMPOSE_CMD) -f $(OTEL_COMPOSE) down

# Run a demo analysis with tracing enabled against the local OTel stack.
# Usage: make demo [DEMO_REPO=.] [DEMO_ANALYZER=static/complexity]
DEMO_REPO ?= .
DEMO_ANALYZER ?= static/complexity
.PHONY: demo
demo: all otel-up
	OTEL_EXPORTER_OTLP_ENDPOINT=localhost:4317 \
	${GOBIN}/codefang run --debug-trace -a $(DEMO_ANALYZER) $(DEMO_REPO)
	@echo "View trace at http://localhost:16686"

# =============================================================================
# libgit2 targets for fast history analysis
# =============================================================================

# Build libgit2 as a static library (vendored in third_party/libgit2)
.PHONY: libgit2
libgit2: $(LIBGIT2_INSTALL)/lib64/libgit2.a

$(LIBGIT2_INSTALL)/lib64/libgit2.a:
	@echo "Building libgit2 statically from third_party/libgit2..."
	@mkdir -p $(LIBGIT2_BUILD)
	cd $(LIBGIT2_BUILD) && cmake .. \
		-DCMAKE_INSTALL_PREFIX=$(CURDIR)/$(LIBGIT2_INSTALL) \
		-DCMAKE_BUILD_TYPE=Release \
		-DBUILD_SHARED_LIBS=OFF \
		-DBUILD_TESTS=OFF \
		-DBUILD_CLI=OFF \
		-DUSE_SSH=OFF \
		-DUSE_HTTPS=OFF \
		-DUSE_BUNDLED_ZLIB=ON
	cd $(LIBGIT2_BUILD) && cmake --build . --parallel
	cd $(LIBGIT2_BUILD) && cmake --install .
	@echo "✓ libgit2 built and installed to $(LIBGIT2_INSTALL)"
