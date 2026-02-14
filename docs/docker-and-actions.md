# Docker & GitHub Actions

Codefang ships a multi-stage Dockerfile and a GitHub Actions `action.yml`
for automated code quality checks in CI pipelines.

## Docker

### Building the Image

```bash
make docker-build                        # default tag: codefang:latest
make docker-build DOCKER_TAG=v1.0.0      # custom tag
```

Or directly with Docker:

```bash
docker build -t codefang .
```

Build arguments for version metadata:

```bash
docker build \
  --build-arg VERSION=v1.0.0 \
  --build-arg COMMIT=$(git rev-parse --short HEAD) \
  --build-arg BUILD_DATE=$(date -u +%Y-%m-%dT%H:%M:%SZ) \
  -t codefang:v1.0.0 .
```

### Running in a Container

Run static analysis on a local directory:

```bash
docker run --rm -v "$(pwd):/workspace:ro" codefang run -a static/* --format json --silent /workspace
```

Run history analysis on a Git repository:

```bash
docker run --rm -v "/path/to/repo:/workspace:ro" codefang run -a history/burndown --format json --silent /workspace
```

Run with a custom config file:

```bash
docker run --rm \
  -v "$(pwd):/workspace:ro" \
  codefang run -a static/* --config /workspace/.codefang.yaml --format json --silent /workspace
```

### Testing the Docker Image

```bash
make docker-test
```

This builds the image and runs a static complexity analysis on the Codefang
source tree inside the container.

### Image Details

- **Base image**: `debian:bookworm-slim` (glibc-compatible for CGO binaries)
- **Size**: ~950 MB (60+ tree-sitter language parsers compiled into binaries)
- **Binaries**: `/usr/local/bin/codefang`, `/usr/local/bin/uast`
- **Runtime deps**: `git`, `libssl3`, `zlib1g`, `libgomp1`, `ca-certificates`
- **Default user**: `codefang` (non-root)
- **Entrypoint**: `codefang`

## GitHub Actions

### Quick Start

Add Codefang to your workflow:

```yaml
name: Code Quality
on: [push, pull_request]

jobs:
  analyze:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

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

### Action Inputs

| Input | Description | Default |
|-------|-------------|---------|
| `analyzers` | Comma-separated analyzer IDs or glob patterns | `static/*` |
| `path` | Path to analyze | `.` |
| `config-path` | Path to `.codefang.yaml` config file | (auto-detect) |
| `format` | Output format: `json`, `text`, `compact`, `yaml` | `json` |
| `fail-on-error` | Fail workflow if analysis detects issues | `false` |

### Action Outputs

| Output | Description |
|--------|-------------|
| `report` | Full analysis report content |
| `pass` | `true` if analysis completed without errors, `false` otherwise |

### Examples

#### Static Analysis with Quality Gate

```yaml
- name: Run Codefang
  uses: Sumatoshi-tech/codefang@main
  with:
    analyzers: "static/complexity,static/comments"
    fail-on-error: "true"
```

#### History Analysis

```yaml
- name: Run Codefang History
  uses: Sumatoshi-tech/codefang@main
  with:
    analyzers: "history/burndown"
    format: "json"
```

#### All Analyzers with Custom Config

```yaml
- name: Run Codefang
  uses: Sumatoshi-tech/codefang@main
  with:
    analyzers: "*"
    config-path: ".codefang.yaml"
    format: "json"
```

#### Post Results as PR Comment

```yaml
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
        body: '## Codefang Analysis\n\n```\n' + `${{ steps.codefang.outputs.report }}` + '\n```'
      })
```

### Self-Test Workflow

The repository includes `.github/workflows/action-test.yml` which validates
the Docker build and action functionality on every change to the Dockerfile,
`action.yml`, or `entrypoint.sh`.
