---
title: Docker & GitHub Actions
description: Running Codefang in Docker containers and GitHub Actions CI pipelines for automated code quality checks.
---

# Docker & GitHub Actions

Codefang ships a multi-stage Dockerfile and a GitHub Actions `action.yml`
for automated code quality checks in CI pipelines.

---

## Docker

### Building the Image

Using the Makefile:

```bash
make docker-build                        # default tag: codefang:latest
make docker-build DOCKER_TAG=v1.0.0      # custom tag
```

Or directly with Docker:

```bash
docker build -t codefang .
```

#### Build Arguments

Embed version metadata into the binary at build time:

```bash
docker build \
  --build-arg VERSION=v1.0.0 \
  --build-arg COMMIT=$(git rev-parse --short HEAD) \
  --build-arg BUILD_DATE=$(date -u +%Y-%m-%dT%H:%M:%SZ) \
  -t codefang:v1.0.0 .
```

### Running in a Container

#### Static Analysis

Analyze a local directory (read-only mount):

```bash
docker run --rm \
  -v "$(pwd):/workspace:ro" \
  codefang run -a static/* --format json --silent /workspace
```

#### History Analysis

Analyze a Git repository's commit history:

```bash
docker run --rm \
  -v "/path/to/repo:/workspace:ro" \
  codefang run -a history/burndown --format json --silent /workspace
```

#### Combined Analysis

Run both static and history analyzers in one pass:

```bash
docker run --rm \
  -v "/path/to/repo:/workspace:ro" \
  codefang run -a static/*,history/devs --format json --silent /workspace
```

#### Custom Configuration

Mount a config file into the container:

```bash
docker run --rm \
  -v "$(pwd):/workspace:ro" \
  codefang run -a static/* \
    --config /workspace/.codefang.yaml \
    --format json --silent /workspace
```

#### With Observability

Export traces to an OTLP collector:

```bash
docker run --rm \
  -e OTEL_EXPORTER_OTLP_ENDPOINT=host.docker.internal:4317 \
  -e OTEL_EXPORTER_OTLP_INSECURE=true \
  -v "/path/to/repo:/workspace:ro" \
  codefang run -a history/* --format json --silent /workspace
```

#### Using the UAST CLI

The Docker image also includes the `uast` binary:

```bash
docker run --rm \
  -v "$(pwd):/workspace:ro" \
  --entrypoint uast \
  codefang parse /workspace/main.go
```

### Testing the Docker Image

```bash
make docker-test
```

This builds the image and runs a static complexity analysis on the Codefang
source tree inside the container.

### Image Details

| Property | Value |
|----------|-------|
| **Base image** | `debian:bookworm-slim` (glibc-compatible for CGO binaries) |
| **Image size** | ~950 MB (60+ Tree-sitter language parsers compiled in) |
| **Binaries** | `/usr/local/bin/codefang`, `/usr/local/bin/uast` |
| **Runtime deps** | `git`, `libssl3`, `zlib1g`, `libgomp1`, `ca-certificates` |
| **Default user** | `codefang` (non-root, UID 1000) |
| **Entrypoint** | `codefang` |
| **Working dir** | `/workspace` |

!!! note "Image Size"
    The image is larger than typical Go binaries because Codefang statically
    links 60+ Tree-sitter language grammars and the libgit2 C library. The
    multi-stage build ensures only runtime dependencies are included in the
    final image.

---

## GitHub Actions

### Quick Start

Add Codefang to any workflow:

```yaml
name: Code Quality
on: [push, pull_request]

jobs:
  analyze:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0  # Full history for history analyzers

      - name: Run Codefang
        id: codefang
        uses: Sumatoshi-tech/codefang@main
        with:
          analyzers: "static/*"
          format: "json"

      - name: Check results
        run: |
          echo "Pass: ${{ steps.codefang.outputs.pass }}"
          echo "${{ steps.codefang.outputs.report }}"
```

!!! warning "fetch-depth: 0"
    History analyzers require full Git history. Set `fetch-depth: 0` in the
    checkout step, or history analysis will only see the shallow clone commits.

### Action Inputs

| Input | Description | Default |
|-------|-------------|---------|
| `analyzers` | Comma-separated analyzer IDs or glob patterns | `static/*` |
| `path` | Path to analyze (relative to repo root) | `.` |
| `config-path` | Path to `.codefang.yaml` config file | (auto-detect) |
| `format` | Output format: `json`, `text`, `compact`, `yaml`, `timeseries` | `json` |
| `fail-on-error` | Fail the workflow step if analysis detects issues | `false` |

### Action Outputs

| Output | Description |
|--------|-------------|
| `report` | Full analysis report content |
| `pass` | `true` if analysis completed without errors, `false` otherwise |

---

### Example Workflows

#### Static Analysis with Quality Gate

Fail the CI pipeline if static analysis detects issues:

```yaml
name: Quality Gate
on: [pull_request]

jobs:
  quality:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Run Codefang
        uses: Sumatoshi-tech/codefang@main
        with:
          analyzers: "static/complexity,static/comments"
          fail-on-error: "true"
```

#### History Analysis on Push

Run history analysis on every push to main:

```yaml
name: History Analysis
on:
  push:
    branches: [main]

jobs:
  history:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Run Codefang History
        id: codefang
        uses: Sumatoshi-tech/codefang@main
        with:
          analyzers: "history/burndown,history/devs,history/couples"
          format: "json"

      - name: Upload report
        uses: actions/upload-artifact@v4
        with:
          name: codefang-history
          path: report.json
```

#### Post Results as PR Comment

Annotate pull requests with analysis results:

```yaml
name: PR Analysis
on: [pull_request]

jobs:
  analyze:
    runs-on: ubuntu-latest
    permissions:
      pull-requests: write
    steps:
      - uses: actions/checkout@v4

      - name: Run Codefang
        id: codefang
        uses: Sumatoshi-tech/codefang@main
        with:
          analyzers: "static/*"
          format: "text"

      - name: Comment on PR
        if: github.event_name == 'pull_request'
        uses: actions/github-script@v7
        with:
          script: |
            github.rest.issues.createComment({
              issue_number: context.issue.number,
              owner: context.repo.owner,
              repo: context.repo.repo,
              body: '## Codefang Analysis\n\n```\n' +
                `${{ steps.codefang.outputs.report }}` +
                '\n```'
            })
```

#### All Analyzers with Custom Config

Run the full analyzer suite with a project config file:

```yaml
name: Full Analysis
on:
  push:
    branches: [main]

jobs:
  analyze:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Run Codefang
        uses: Sumatoshi-tech/codefang@main
        with:
          analyzers: "*"
          config-path: ".codefang.yaml"
          format: "json"
```

#### Complexity Trend Tracking

Track complexity over time by uploading JSON reports as artifacts:

```yaml
name: Complexity Trend
on:
  push:
    branches: [main]

jobs:
  track:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Run Complexity Analysis
        id: codefang
        uses: Sumatoshi-tech/codefang@main
        with:
          analyzers: "static/complexity"
          format: "json"

      - name: Save report
        run: echo '${{ steps.codefang.outputs.report }}' > complexity-report.json

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: complexity-${{ github.sha }}
          path: complexity-report.json
          retention-days: 90
```

#### Scheduled Fleet Scan

Run nightly analysis across multiple paths:

```yaml
name: Nightly Scan
on:
  schedule:
    - cron: '0 2 * * *'  # 2 AM UTC

jobs:
  scan:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        path: [services/api, services/worker, libs/core]
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Run Codefang
        uses: Sumatoshi-tech/codefang@main
        with:
          analyzers: "static/*,history/devs"
          path: ${{ matrix.path }}
          format: "json"
```

---

## Troubleshooting

### Common Issues

!!! failure "Error: repository path does not exist"
    The checkout step did not run, or the `path` input points to a
    non-existent directory. Verify your `actions/checkout` configuration.

!!! failure "Error: no commits found"
    Your checkout used the default `fetch-depth: 1` (shallow clone).
    Set `fetch-depth: 0` for full history or use `--since` to limit the range.

!!! failure "Error: out of memory"
    For large repositories in CI, add `--memory-budget 2GiB` to constrain
    memory usage. The streaming pipeline will automatically chunk the
    commit history.

### Self-Test Workflow

The repository includes `.github/workflows/action-test.yml` which validates
the Docker build and action functionality on every change to the Dockerfile,
`action.yml`, or `entrypoint.sh`.
