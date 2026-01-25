# Development Guide

This guide covers everything you need to know to contribute to Codefang development.

## Prerequisites

### Required Software

- **Go 1.21+**: [Download](https://golang.org/dl/)
- **Git**: [Download](https://git-scm.com/downloads)
- **Make**: Usually pre-installed on Unix systems

### Recommended Tools

- **VS Code** with Go extension
- **GoLand** or **Vim/Emacs** with Go support
- **Delve** debugger: `go install github.com/go-delve/delve/cmd/dlv@latest`

## Development Setup

### 1. Clone the Repository

```bash
git clone https://github.com/Sumatoshi-tech/codefang.git
cd codefang
```

### 2. Install Dependencies

```bash
go mod download
go install github.com/golangci/golangci-lint/cmd/golangci-lint@latest
```

### 3. Build the Project

```bash
make build
```

### 4. Verify Setup

```bash
# Run tests
make test

# Check builds
./codefang --help
./uast --help
```

## Project Structure

```
codefang/
├── cmd/
│   ├── codefang/              # Main analyzer CLI
│   │   ├── main.go        # Entry point
│   │   └── commands/      # CLI commands
│   │       ├── analyze.go # Static analysis command
│   │       └── history.go # History analysis command
│   └── uast/              # UAST parser CLI
├── pkg/
│   ├── analyzers/         # All analyzers
│   │   ├── analyze/       # Analyzer factory and interfaces
│   │   ├── common/        # Shared utilities
│   │   │   └── terminal/  # Terminal rendering (box drawing, colors, progress bars)
│   │   ├── plumbing/      # Git plumbing for history analyzers
│   │   │
│   │   │ # Static analyzers (UAST-based)
│   │   ├── complexity/    # Cyclomatic complexity
│   │   ├── cohesion/      # Code cohesion
│   │   ├── comments/      # Comment quality
│   │   ├── halstead/      # Halstead metrics
│   │   │
│   │   │ # History analyzers (Git-based)
│   │   ├── burndown/      # Line burndown
│   │   ├── couples/       # File/dev coupling
│   │   ├── devs/          # Developer stats
│   │   ├── file_history/  # File history
│   │   ├── imports/       # Import analysis (static & history)
│   │   ├── sentiment/     # Comment sentiment
│   │   ├── shotness/      # Structural hotspots
│   │   └── typos/         # Typo detection
│   │
│   ├── uast/              # UAST parsing (60+ languages)
│   │   ├── uastmaps/      # Language mapping files
│   │   └── pkg/           # UAST utilities
│   │
│   ├── framework/         # Analysis pipeline framework
│   ├── pipeline/          # Pipeline configuration
│   └── ...
├── docs/                  # Documentation
└── test/                  # Test data
```

## Building

```bash
# Build all binaries
make build

# Build specific binary
go build ./cmd/codefang
go build ./cmd/uast

# Cross-compilation
GOOS=linux GOARCH=amd64 go build ./cmd/codefang
```

## Testing

```bash
# Run all tests
make test

# Run specific package tests
go test ./pkg/analyzers/complexity/...
go test ./pkg/uast/...

# Run with coverage
go test -cover ./pkg/analyzers/...

# Run benchmarks
go test -bench=. ./pkg/uast/...
```

## CLI Tools

### codefang - Main Analyzer

```bash
# Static analysis (from UAST)
uast parse main.go | codefang analyze -a complexity,halstead

# History analysis (from git repo)
codefang history -a burndown,devs .
```

### uast - UAST Parser

```bash
# Parse source file
uast parse main.go

# Validate language mapping
uast validate go

# Start dev server
uast server --port 8080
```

## Adding New Analyzers

### Static Analyzer

1. Create package in `pkg/analyzers/<name>/`
2. Implement `analyze.StaticAnalyzer` interface:

```go
type MyAnalyzer struct{}

func (a *MyAnalyzer) Name() string { return "my-analyzer" }
func (a *MyAnalyzer) Flag() string { return "my-analyzer" }
func (a *MyAnalyzer) Description() string { return "..." }
func (a *MyAnalyzer) Analyze(root *node.Node) (analyze.Report, error) { ... }
func (a *MyAnalyzer) Thresholds() analyze.Thresholds { ... }
func (a *MyAnalyzer) CreateAggregator() analyze.ResultAggregator { ... }
func (a *MyAnalyzer) FormatReport(report analyze.Report, w io.Writer) error { ... }
func (a *MyAnalyzer) FormatReportJSON(report analyze.Report, w io.Writer) error { ... }
func (a *MyAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption { ... }
func (a *MyAnalyzer) Configure(facts map[string]interface{}) error { ... }
```

3. Register in `cmd/codefang/commands/analyze.go`

### History Analyzer

1. Create package in `pkg/analyzers/<name>/`
2. Implement `analyze.HistoryAnalyzer` interface
3. Register in `cmd/codefang/commands/history.go`

## Code Style

- Follow [Effective Go](https://golang.org/doc/effective_go.html)
- Run `golangci-lint run` before committing
- Use descriptive names
- Write tests for new code

## Debugging

```bash
# Debug with Delve
dlv debug ./cmd/codefang -- analyze -a complexity

# CPU profiling
go test -cpuprofile=cpu.prof -bench=.
go tool pprof cpu.prof
```

## Contributing

1. Fork the repository
2. Create feature branch: `git checkout -b feature/my-feature`
3. Write tests
4. Submit pull request

## Resources

- [Go Documentation](https://golang.org/doc/)
- [Tree-sitter](https://tree-sitter.github.io/tree-sitter/)
