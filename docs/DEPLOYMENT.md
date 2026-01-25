# Installation & Deployment

## Installation

### From Source

```bash
git clone https://github.com/Sumatoshi-tech/codefang.git
cd codefang
make build

# Binaries are created in current directory
./codefang --help
./uast --help
```

### Go Install

```bash
go install github.com/Sumatoshi-tech/codefang/cmd/codefang@latest
go install github.com/Sumatoshi-tech/codefang/cmd/uast@latest
```

### Docker

```bash
# Build
docker build -t codefang .

# Run static analysis
docker run -v $(pwd):/code codefang codefang analyze

# Run history analysis
docker run -v $(pwd):/code codefang codefang history -a burndown /repo
```

## Binaries

The project produces two CLI tools:

| Binary | Purpose |
|--------|---------|
| `codefang` | Code analysis (static and history) |
| `uast` | UAST parsing and utilities |

## Usage Examples

### Static Analysis

```bash
# Parse and analyze
uast parse main.go | codefang analyze

# Specific analyzers
uast parse main.go | codefang analyze -a complexity,halstead

# JSON output
uast parse main.go | codefang analyze -f json
```

### History Analysis

```bash
# Analyze git repository
codefang history -a burndown .
codefang history -a burndown,couples,devs .
codefang history -a devs --head .
```

### UAST Operations

```bash
# Parse source file
uast parse main.go

# Validate language mapping
uast validate go

# Start development server
uast server --port 8080
```

## Build Options

```bash
# Build all
make build

# Build with race detector
make build-race

# Run tests
make test

# Run linter
make lint
```
