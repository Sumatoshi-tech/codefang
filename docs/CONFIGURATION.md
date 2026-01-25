# Configuration Guide

## Command-Line Options

### codefang analyze (Static Analysis)

```bash
uast parse <file> | codefang analyze [flags]
```

| Flag | Description | Default |
|------|-------------|---------|
| `-a, --analyzers` | Analyzers to run (comma-separated) | all |
| `-f, --format` | Output format: text or json | text |
| `-o, --output` | Output file (default: stdout) | - |

**Available analyzers:** complexity, cohesion, comments, halstead, imports

### codefang history (History Analysis)

```bash
codefang history [flags] <repository>
```

| Flag | Description | Default |
|------|-------------|---------|
| `-a, --analyzers` | Analyzers to run (comma-separated) | required |
| `-f, --format` | Output format: yaml or json | yaml |
| `--head` | Analyze only latest commit | false |
| `--first-parent` | Follow only first parent | false |

**Available analyzers:** burndown, couples, devs, file-history, imports, sentiment, shotness, typos

### uast parse

```bash
uast parse [flags] <file>
```

| Flag | Description | Default |
|------|-------------|---------|
| `-f, --format` | Output format: json or yaml | json |

### uast server

```bash
uast server [flags]
```

| Flag | Description | Default |
|------|-------------|---------|
| `--port` | Server port | 8080 |
| `--static` | Path to static files for web UI | - |

## Examples

### Static Analysis

```bash
# All analyzers, text output
uast parse main.go | codefang analyze

# Specific analyzers
uast parse main.go | codefang analyze -a complexity,halstead

# JSON output
uast parse main.go | codefang analyze -f json

# Save to file
uast parse main.go | codefang analyze -f json -o report.json

# Multiple files
uast parse *.go | codefang analyze
```

### History Analysis

```bash
# Single analyzer
codefang history -a burndown .

# Multiple analyzers
codefang history -a burndown,couples,devs .

# Latest commit only
codefang history -a devs --head .

# JSON output
codefang history -a burndown -f json .

# Analyze remote repository (clones locally)
codefang history -a burndown /path/to/repo
```

## Analyzer-Specific Options

### Burndown Options

| Option | Description | Default |
|--------|-------------|---------|
| `--granularity` | Time ticks per band | 30 |
| `--sampling` | State recording frequency | 30 |
| `--burndown-files` | Per-file statistics | false |
| `--burndown-people` | Per-developer statistics | false |

### Couples Options

| Option | Description | Default |
|--------|-------------|---------|
| `--tick-size` | Hours per tick | 24 |

### Devs Options

| Option | Description | Default |
|--------|-------------|---------|
| `--tick-size` | Hours per tick | 24 |
| `--empty-commits` | Include empty commits | false |

### Imports Options

| Option | Description | Default |
|--------|-------------|---------|
| `--import-goroutines` | Parallel extraction threads | 4 |
| `--import-max-file-size` | Max file size to process | 1MB |

### Shotness Options

| Option | Description | Default |
|--------|-------------|---------|
| `--shotness-dsl-struct` | UAST DSL query for nodes | filter(.roles has "Function") |
| `--shotness-dsl-name` | UAST DSL query for names | .token |

### Typos Options

| Option | Description | Default |
|--------|-------------|---------|
| `--typos-max-distance` | Max Levenshtein distance | 4 |

### Sentiment Options

| Option | Description | Default |
|--------|-------------|---------|
| `--min-comment-len` | Minimum comment length | 20 |
| `--sentiment-gap` | Sentiment threshold | 0.5 |
