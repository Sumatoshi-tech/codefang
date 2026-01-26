GOBIN ?= build/bin
GO111MODULE=on
ifneq ($(OS),Windows_NT)
EXE =
else
EXE = .exe
endif
PKG = $(shell go env GOOS)_$(shell go env GOARCH)
TAGS ?=

all: precompile ${GOBIN}/uast${EXE} ${GOBIN}/codefang${EXE}

# Build all binaries (alias for all)
.PHONY: build
build: all

# Show help
.PHONY: help
help:
	@echo "Available targets:"
	@echo "  all              - Build all binaries"
	@echo "  build            - Build all binaries (alias for all)"
	@echo "  install          - Install binaries to system PATH"
	@echo "  test             - Run all tests"
	@echo "  lint             - Run linters and deadcode analysis"
	@echo "  fmt              - Format code"
	@echo "  deadcode         - Run deadcode analysis with detailed output"
	@echo "  deadcode-prod    - Run deadcode analysis excluding tests"
	@echo "  deadcode-why     - Show why a function is not dead (FUNC=name)"
	@echo "  bench            - Run UAST performance benchmarks"
	@echo "  uast-dev         - Start UAST development environment (frontend + backend)"
	@echo "  uast-dev-stop    - Stop UAST development servers"
	@echo "  uast-dev-status  - Check status of UAST development servers"
	@echo "  uast-test        - Run UI tests for UAST development service"
	@echo "  dev-service      - Start backend only (legacy)"
	@echo "  clean            - Clean build artifacts"

# Pre-compile UAST mappings for faster startup
precompile:
	@echo "Pre-compiling UAST mappings..."
	@mkdir -p ${GOBIN}
	@go run ./build/scripts/precompgen/precompile.go -o pkg/uast/embedded_mappings.gen.go

# Generate UAST mappings for all languages
uastmaps-gen:
	@echo "Generating UAST mappings..."
	@python3 build/scripts/uastmapsgen/gen_uastmaps.py

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

# Run all tests with CGO disabled (for cross-platform compatibility)
test: all
	CGO_ENABLED=1 go test ./...

# Run all tests with CGO disabled and verbose output
testv: all
	CGO_ENABLED=1 go test ./... -v

# Run UAST performance benchmarks (comprehensive suite with organized results)
bench: all
	python3 build/scripts/benchmark/benchmark_runner.py

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
	python3 build/scripts/benchmark/benchmark_plot.py test/benchmarks/benchmark_results.txt

# Run benchmarks with verbose output and generate plots
benchplotv: all
	CGO_ENABLED=1 go test -run="^$$" -bench=. -benchmem -v ./pkg/uast > test/benchmarks/benchmark_results.txt 2>&1
	python3 build/scripts/benchmark/benchmark_plot.py test/benchmarks/benchmark_results.txt

# Run comprehensive benchmark suite with organized results
bench-suite: bench

# Run benchmarks without plots
bench-no-plots: all
	python3 build/scripts/benchmark/benchmark_runner.py --no-plots

# Generate report for latest benchmark run
report:
	python3 build/scripts/benchmark/benchmark_report.py

# Generate report for specific run
report-run:
	python3 build/scripts/benchmark/benchmark_report.py $(RUN_NAME)

# List all benchmark runs
bench-list:
	python3 build/scripts/benchmark/benchmark_report.py --list

# Compare latest run with previous
compare:
	python3 build/scripts/benchmark/benchmark_comparison.py $(shell python3 build/scripts/benchmark/benchmark_report.py --list | head -1)

# Compare specific runs
compare-runs:
	python3 build/scripts/benchmark/benchmark_comparison.py $(CURRENT_RUN) --baseline $(BASELINE_RUN)

# Compare last N benchmark runs (usage: make compare-last N=3)
compare-last:
	@N=$${N:-2}; \
	echo "Comparing last $$N benchmark runs..."; \
	if [ $$N -eq 2 ]; then \
		LATEST=$$(python3 build/scripts/benchmark/benchmark_report.py --list | grep -v "Available benchmark runs:" | head -1 | xargs); \
		PREVIOUS=$$(python3 build/scripts/benchmark/benchmark_report.py --list | grep -v "Available benchmark runs:" | head -2 | tail -1 | xargs); \
		echo "Latest: $$LATEST"; \
		echo "Previous: $$PREVIOUS"; \
		python3 build/scripts/benchmark/benchmark_comparison.py "$$LATEST" --baseline "$$PREVIOUS"; \
	else \
		RUNS=$$(python3 build/scripts/benchmark/benchmark_report.py --list | grep -v "Available benchmark runs:" | head -$$N | tr '\n' ' '); \
		echo "Comparing $$N runs:"; \
		echo "$$RUNS" | nl; \
		python3 build/scripts/benchmark/benchmark_comparison_multi.py "$$RUNS"; \
	fi

# Run benchmarks with profiling and generate plots
benchplotprofile: all
	CGO_ENABLED=1 go test -run="^$$" -bench=. -benchmem -cpuprofile=cpu.prof -memprofile=mem.prof ./pkg/uast > test/benchmarks/benchmark_results.txt 2>&1
	python3 build/scripts/benchmark/benchmark_plot.py test/benchmarks/benchmark_results.txt

# Run specific benchmark and generate plots (usage: make benchplot-simple BENCH=BenchmarkParse)
benchplot-simple: all
	CGO_ENABLED=1 go test -run="^$$" -bench=$(BENCH) -benchmem ./pkg/uast > test/benchmarks/benchmark_results.txt 2>&1
	python3 build/scripts/benchmark/benchmark_plot.py test/benchmarks/benchmark_results.txt

# Run benchmarks with timeout and generate plots (usage: make benchplot-timeout TIMEOUT=30s)
benchplot-timeout: all
	CGO_ENABLED=1 go test -run="^$$" -bench=. -benchmem -timeout=$(TIMEOUT) ./pkg/uast > test/benchmarks/benchmark_results.txt 2>&1
	python3 build/scripts/benchmark/benchmark_plot.py test/benchmarks/benchmark_results.txt

clean:
	rm -f ./uast
	rm -f ./codefang
	rm -rf ${GOBIN}/
	rm -f *.prof
	rm -f test/benchmarks/benchmark_results.txt
	rm -rf benchmark_plots/

# Linting tools (resolve from GOPATH/bin if not on PATH)
GOPATH_BIN=$(shell go env GOPATH)/bin
GOLINT=$(or $(shell command -v golangci-lint 2>/dev/null),$(GOPATH_BIN)/golangci-lint)
DEADCODE=$(or $(shell command -v deadcode 2>/dev/null),$(GOPATH_BIN)/deadcode)

# Package paths for linting
INTERNAL_PKGS=./cmd/... ./pkg/...

## lint: Run linters and deadcode analysis
.PHONY: lint
lint:
	@echo "Running linters..."
	@$(GOLINT) run $(INTERNAL_PKGS)
	@echo "Running deadcode analysis..."
	@./scripts/deadcode-filter.sh -test github.com/Sumatoshi-tech/codefang/cmd/... github.com/Sumatoshi-tech/codefang/pkg/...
	@echo "✓ Linting complete"

## deadcode: Run deadcode analysis with detailed output (requires: go install golang.org/x/tools/cmd/deadcode@latest)
.PHONY: deadcode
deadcode:
	@echo "Running deadcode analysis..."
	@echo "================================================================"
	@$(DEADCODE) -test github.com/Sumatoshi-tech/codefang/cmd/... github.com/Sumatoshi-tech/codefang/pkg/... || echo "Note: Review any unreachable functions listed above"
	@echo "================================================================"
	@echo "Tip: Use 'deadcode -whylive <function>' to understand why a function is considered reachable"

## deadcode-prod: Run deadcode analysis excluding tests (production-only dead code)
.PHONY: deadcode-prod
deadcode-prod:
	@echo "Running deadcode analysis (production only)..."
	@echo "================================================================"
	@./scripts/deadcode-filter.sh github.com/Sumatoshi-tech/codefang/cmd/... github.com/Sumatoshi-tech/codefang/pkg/...

## deadcode-test-only: Find functions used only by tests (requires jq)
.PHONY: deadcode-test-only
deadcode-test-only:
	@echo "Finding functions that are only used by tests..."
	@echo "This compares deadcode results with and without tests..."
	@mkdir -p .deadcode-tmp
	@$(DEADCODE) -json github.com/Sumatoshi-tech/codefang/cmd/... github.com/Sumatoshi-tech/codefang/pkg/... | jq -r '.[] | .Funcs[].Name' | sort > .deadcode-tmp/dead_prod.txt
	@$(DEADCODE) -test -json github.com/Sumatoshi-tech/codefang/cmd/... github.com/Sumatoshi-tech/codefang/pkg/... | jq -r '.[] | .Funcs[].Name' | sort > .deadcode-tmp/dead_with_tests.txt
	@echo "Functions used only by tests:"
	@comm -23 .deadcode-tmp/dead_prod.txt .deadcode-tmp/dead_with_tests.txt || echo "No test-only functions found"
	@rm -rf .deadcode-tmp

## deadcode-json: Run deadcode analysis with JSON output
.PHONY: deadcode-json
deadcode-json:
	@echo "Running deadcode analysis (JSON output)..."
	@$(DEADCODE) -json github.com/Sumatoshi-tech/codefang/cmd/... github.com/Sumatoshi-tech/codefang/pkg/...

## deadcode-why: Show why a function is not dead (usage: make deadcode-why FUNC=functionName)
.PHONY: deadcode-why
deadcode-why:
	@if [ -z "$(FUNC)" ]; then \
		echo "Usage: make deadcode-why FUNC=functionName"; \
		echo "Example: make deadcode-why FUNC=bytes.Buffer.String"; \
		exit 1; \
	fi
	@echo "Analyzing why $(FUNC) is considered live..."
	@$(DEADCODE) -whylive="$(FUNC)" github.com/Sumatoshi-tech/codefang/cmd/... github.com/Sumatoshi-tech/codefang/pkg/...

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
	CGO_ENABLED=1 go build -tags "$(TAGS)" -o ${GOBIN}/uast${EXE} ./cmd/uast

${GOBIN}/codefang${EXE}: cmd/codefang/*.go cmd/codefang/commands/*.go pkg/analyzers/*/*.go
	CGO_ENABLED=1 go build -tags "$(TAGS)" -o ${GOBIN}/codefang${EXE} ./cmd/codefang

# Build the development service
.PHONY: build-dev-service
build-dev-service:
	@echo "Building UAST Development Service..."
	@cd web && go build -o ../build/uast-dev-service main.go

# Start UAST Development Environment (Frontend + Backend)
.PHONY: uast-dev
uast-dev: install
	@cd web && ./start-dev.sh
