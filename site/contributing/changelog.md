# Changelog

All notable changes to the Codefang project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Temporal Anomaly Detection** analyzer (`history/anomaly`) using Z-score
  statistical analysis over sliding windows for detecting sudden quality
  degradation in commit history.
- **TimeSeries output format** (`--format timeseries`) that merges all
  analyzer outputs into a single chronologically-ordered JSON array.
- **Plot output format** (`--format plot`) generating interactive HTML charts
  via go-echarts for visual analysis.
- **MCP Server** (`codefang mcp`) exposing analysis capabilities as tools
  for AI agents via the Model Context Protocol (stdio transport, JSON-RPC 2.0).
  Three tools: `codefang_analyze`, `uast_parse`, `codefang_history`.
- **Docker support** with multi-stage Dockerfile and `debian:bookworm-slim`
  runtime image. Non-root `codefang` user by default.
- **GitHub Actions** integration (`action.yml`) for automated code quality
  checks in CI pipelines with configurable analyzers, formats, and quality gates.
- **OpenTelemetry observability** with distributed tracing, RED metrics
  (Rate, Errors, Duration), and structured logging with trace context injection.
  Supports Jaeger, Prometheus, and OTLP collectors.
- **Streaming pipeline** for bounded-memory processing of large repositories
  via chunk-based processing with hibernate/boot cycles.
- **Double-buffered chunk pipelining** overlapping pipeline prefetch with
  analyzer consumption for improved throughput on multi-chunk workloads.
- **Checkpointing** with automatic save after each processed chunk and
  crash recovery via `--resume` flag. Checkpoint format v2 preserves
  aggregator spill state so resumed runs produce identical output.
- **Configuration system** (`.codefang.yaml`) with file, environment variable,
  and CLI flag support. Merge priority: CLI > env > file > defaults.
- **Large-scale scanning** support for fleet analysis of thousands of
  repositories with bare repo support, GNU Parallel and Kubernetes orchestration
  patterns, and DWH loading guides (Athena, Snowflake, Spark).
- **Deep context propagation** eliminating all `context.Background()` calls
  in production hot paths for end-to-end tracing.
- **Attribute filter** span processor enforcing allow-list of attribute key
  prefixes to prevent PII leakage to collectors.
- **Health check endpoints** (`/healthz`, `/readyz`, `/metrics`) for server
  mode deployments.
- **Incremental scanning** with `--since` flag supporting Go durations,
  date strings, and RFC3339 timestamps.
- **Memory budget auto-tuning** via `--memory-budget` flag with automatic
  chunk size calculation.
- **Watchdog** stall detection with configurable `worker_timeout` for
  identifying hung workers.

### Changed

- **Complete rewrite** from the original `src-d/hercules` project. Modern
  Go 1.24+ codebase with idiomatic patterns, clean architecture, and
  comprehensive test coverage.
- **Split architecture** into two binaries: `uast` (Universal AST parser
  using Tree-sitter for 60+ languages) and `codefang` (analysis engine
  for static and history analysis). Unix philosophy: small tools joined by pipes.
- **Tree-sitter based UAST** replacing the original Babelfish/bblfsh parser
  with a faster, more reliable, and locally-compiled alternative supporting
  60+ programming languages.
- **DSL-based UAST mappings** with a custom domain-specific language for
  transforming Tree-sitter ASTs into standardized UAST nodes.
- **Vendored libgit2** (`third_party/libgit2`) compiled as a static library
  for reproducible builds without external dependencies.
- **Structured logging** with `log/slog` replacing all `log.Printf` and
  `fmt.Printf` calls. Instance loggers enforced by `sloglint`.

### Analyzers

#### Static Analyzers (UAST-based)
- `static/complexity` - Cyclomatic, cognitive complexity, and nesting depth
- `static/cohesion` - LCOM4 metrics for class cohesion analysis
- `static/halstead` - Halstead complexity metrics (volume, difficulty, effort)
- `static/comments` - Documentation coverage, placement quality
- `static/imports` - Import/dependency analysis from source code

#### History Analyzers (Git-based)
- `history/burndown` - Code survival over time with line ownership tracking
- `history/devs` - Developer activity, language breakdown, bus factor risk
- `history/couples` - File coupling and co-change pattern detection
- `history/file-history` - Per-file lifecycle with rename tracking
- `history/sentiment` - Comment sentiment analysis over commit history
- `history/shotness` - Structural hotspots (function-level change frequency)
- `history/typos` - Typo detection dataset builder via Levenshtein distance
- `history/imports` - Import/dependency evolution over time
- `history/anomaly` - Z-score temporal anomaly detection

---

*For detailed documentation, visit the [Codefang Documentation](../index.md).*
