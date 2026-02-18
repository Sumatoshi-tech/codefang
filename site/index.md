---
title: Codefang
hide:
  - navigation
  - toc
---

<div class="cf-hero-banner" markdown>

<div class="cf-stars-container">
<div class="cf-stars1"></div>
<div class="cf-stars2"></div>
<div class="cf-stars3"></div>
</div>

<div class="cf-hero-content" markdown>

<p class="cf-hero-title">Codefang</p>
<p class="cf-hero-subtitle">
The heavy lifter for your codebase — deep code analysis through structure and history.
</p>

<div class="cf-hero-badges" markdown>

[![CI](https://github.com/Sumatoshi-tech/codefang/actions/workflows/ci.yml/badge.svg)](https://github.com/Sumatoshi-tech/codefang/actions/workflows/ci.yml)
[![Go Reference](https://pkg.go.dev/badge/github.com/Sumatoshi-tech/codefang.svg)](https://pkg.go.dev/github.com/Sumatoshi-tech/codefang)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://github.com/Sumatoshi-tech/codefang/blob/main/LICENSE)

</div>

<div class="cf-hero-buttons" markdown>

[Get Started](getting-started/installation.md){ .md-button .md-button--primary }
[View on GitHub](https://github.com/Sumatoshi-tech/codefang){ .md-button }

</div>

</div>

</div>

<hr class="cf-separator">

```
┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
┃ Codefang Overview                                              Score: 10/10 ┃
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
```

Codefang ships two binaries — `uast` for universal syntax-tree parsing
across **60+ languages**, and `codefang` for deep static and historical
analysis of entire repositories. One tool, one config, complete understanding.

---

## Key Metrics

<div class="grid" markdown>

<div class="cf-stat" markdown>
<span class="cf-stat-value">60+</span>
<div class="cf-stat-label">Languages Supported</div>
</div>

<div class="cf-stat" markdown>
<span class="cf-stat-value">14</span>
<div class="cf-stat-label">Built-in Analyzers</div>
</div>

<div class="cf-stat" markdown>
<span class="cf-stat-value">6</span>
<div class="cf-stat-label">Output Formats</div>
</div>

<div class="cf-stat" markdown>
<span class="cf-stat-value">MCP</span>
<div class="cf-stat-label">AI-Agent Ready</div>
</div>

</div>

---

## Distribution

```
  Capabilities
  ──────────────────────────────────────────────────────────────────────────────
  UAST Parsing         ████████████████████  60+ languages via Tree-sitter
  Static Analysis      ████████████████████  5 analyzers (complexity, cohesion, …)
  History Analysis     ████████████████████  9 analyzers (burndown, devs, …)
  Output Formats       ████████████████████  JSON, YAML, text, compact, plot, binary
  Streaming Pipeline   ████████████████████  Bounded-memory, checkpointed
  Observability        ████████████████████  OpenTelemetry traces + metrics
```

---

## Quick Install

=== "Go Install"

    ```bash
    go install github.com/Sumatoshi-tech/codefang/cmd/codefang@latest
    go install github.com/Sumatoshi-tech/codefang/cmd/uast@latest
    ```

=== "Build from Source"

    ```bash
    git clone https://github.com/Sumatoshi-tech/codefang.git
    cd codefang
    make build
    make install
    ```

=== "Docker"

    ```bash
    docker build -t codefang .
    docker run --rm -v "$(pwd):/repo" codefang run -a static/complexity /repo
    ```

Verify the installation:

```bash
codefang --version
uast --version
```

---

## Quick Example

```
$ codefang run -a static/complexity,static/halstead --format text .

codefang (v2):
  version: 2
  hash: abc123def456

┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
┃ Complexity                                                     Score: 8/10  ┃
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛

  Summary: Good overall code structure with some complex functions

  Key Metrics
  ──────────────────────────────────────────────────────────────────────────────
  Average Complexity   6.2           Max Complexity       18

  Distribution
  ──────────────────────────────────────────────────────────────────────────────
  Simple              ████████████████░░░░  65%  (42)
  Complex             ████░░░░░░░░░░░░░░░░  20%  (13)
  Critical            ██░░░░░░░░░░░░░░░░░░  15%  (10)

  Top Issues
  ──────────────────────────────────────────────────────────────────────────────
  ProcessPayment       src/payment.go:245              HIGH
  ValidateInput        src/utils.go:89                 MEDIUM
```

---

## Analyzers at a Glance

### Static Analyzers

| Analyzer | Description | Key Metric |
|----------|-------------|------------|
| `complexity` | Cyclomatic and cognitive complexity per function | Score 0–10 |
| `cohesion` | Class/module cohesion metrics (LCOM) | LCOM ratio |
| `halstead` | Halstead software science metrics | Volume, Difficulty |
| `comments` | Comment density and documentation ratios | Density % |
| `imports` | Import/dependency graph analysis | Coupling score |

### History Analyzers

| Analyzer | Description | Key Metric |
|----------|-------------|------------|
| `burndown` | Code age and survival analysis over time | Survival % |
| `devs` | Per-developer contribution metrics | LOC, commits |
| `couples` | File and developer coupling patterns | Coupling matrix |
| `file-history` | Per-file change history and churn | Churn rate |
| `sentiment` | Commit message sentiment trends | Sentiment score |
| `shotness` | Function-level change frequency | Hotspot rank |
| `typos` | Identifier typo detection across history | Typo count |
| `anomaly` | Automated anomaly detection in metrics | Z-score |

---

## Explore the Docs

<div class="grid cards" markdown>

-   :material-rocket-launch:{ .lg .middle } __Getting Started__

    ---

    [Installation](getting-started/installation.md) ·
    [Quick Start](getting-started/quickstart.md) ·
    [First Analysis](getting-started/first-analysis.md)

-   :material-book-open-variant:{ .lg .middle } __User Guide__

    ---

    [CLI Reference](guide/cli-reference.md) ·
    [Configuration](guide/configuration.md) ·
    [Output Formats](guide/output-formats.md)

-   :material-cog-outline:{ .lg .middle } __Architecture__

    ---

    [Overview](architecture/overview.md) ·
    [UAST System](architecture/uast.md) ·
    [Streaming Pipeline](architecture/streaming-pipeline.md)

-   :material-puzzle-outline:{ .lg .middle } __Integrations__

    ---

    [MCP Server](integrations/mcp.md) ·
    [Docker & Actions](integrations/docker-and-actions.md) ·
    [AI Agents](integrations/ai-agents.md)

-   :material-telescope:{ .lg .middle } __Operations__

    ---

    [Observability](operations/observability.md) ·
    [Large-Scale Scanning](operations/large-scale-scanning.md)

-   :material-account-group:{ .lg .middle } __Contributing__

    ---

    [How to Contribute](contributing/index.md) ·
    [Code of Conduct](contributing/code-of-conduct.md) ·
    [Changelog](contributing/changelog.md)

</div>
