<!-- Proposed by /documenter changelog (Keep a Changelog 1.1.0). Driver: /march Item 4 (R-RELEASE-01). -->
<!-- This is a proposed replacement for CHANGELOG.md. Review and merge; do not treat as canonical until merged. -->
<!-- Maintainer decision applied: no released git tags exist, so the five prior `## [Unreleased]` blocks are -->
<!-- collapsed into ONE `[Unreleased]` block grouped by change type. No per-version blocks are created. The -->
<!-- `[Unreleased]` reference link points at the commits view (no released compare base exists). -->
<!-- Site-copy decision applied: site/contributing/changelog.md becomes a thin pointer to this canonical file -->
<!-- on GitHub and stays in the mkdocs nav. -->

# Changelog

All notable changes to the Codefang project are documented in this file.

The format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Temporal anomaly detection analyzer (`history/anomaly`) using Z-score
  statistical analysis over sliding windows for detecting sudden quality
  degradation in commit history.
- TimeSeries output format (`--format timeseries`) that merges all analyzer
  outputs into a single chronologically ordered JSON array.
- Plot output format (`--format plot`) generating interactive HTML charts via
  go-echarts for visual analysis.
- NDJSON output format (`--format ndjson`) emitting one JSON line per analyzer
  result (with an optional metadata line) for streaming ingestion into
  warehouses such as ClickHouse.
- MCP server (`codefang mcp`) exposing analysis capabilities as tools for AI
  agents via the Model Context Protocol (stdio transport, JSON-RPC 2.0). Three
  tools: `codefang_analyze`, `uast_parse`, `codefang_history`.
- Docker support with a multi-stage Dockerfile and a `debian:bookworm-slim`
  runtime image. Runs as a non-root `codefang` user by default.
- GitHub Actions integration (`action.yml`) for automated code-quality checks
  in CI pipelines with configurable analyzers, formats, and quality gates.
- OpenTelemetry observability with distributed tracing, RED metrics (rate,
  errors, duration), and structured logging with trace-context injection.
  Supports Jaeger, Prometheus, and OTLP collectors.
- Streaming pipeline for bounded-memory processing of large repositories via
  chunk-based processing with hibernate/boot cycles.
- Double-buffered chunk pipelining that overlaps pipeline prefetch with
  analyzer consumption for higher throughput on multi-chunk workloads.
- Checkpointing with automatic save after each processed chunk and crash
  recovery via the `--resume` flag. Checkpoint format v2 preserves aggregator
  spill state so resumed runs produce identical output.
- Configuration system (`.codefang.yaml`) with file, environment-variable, and
  CLI-flag support. Merge priority: CLI > env > file > defaults.
- Large-scale scanning support for fleet analysis of thousands of repositories,
  with bare-repo support, GNU Parallel and Kubernetes orchestration patterns,
  and data-warehouse loading guides (Athena, Snowflake, Spark).
- Deep context propagation that eliminates `context.Background()` calls in
  production hot paths for end-to-end tracing.
- Attribute-filter span processor that enforces an allow-list of attribute key
  prefixes to prevent PII leakage to collectors.
- Health-check endpoints (`/healthz`, `/readyz`, `/metrics`) for server-mode
  deployments.
- Incremental scanning with the `--since` flag, supporting Go durations, date
  strings, and RFC 3339 timestamps.
- Memory-budget auto-tuning via the `--memory-budget` flag with automatic chunk
  size calculation.
- Watchdog stall detection with a configurable `worker_timeout` for identifying
  hung workers.
- `--include-vendored` flag (bool, default `false`) to re-include paths detected
  as vendored by enry/Linguist (`vendor/`, `node_modules/`, `third_party/`,
  `testdata/`, `dist/`, minified bundles, and more).
- `--include-generated` flag (bool, default `false`) to re-include
  auto-generated files (`*.pb.go`, `zz_generated_*.go`, `*_pb2.py`, `*.min.js`,
  and content-header markers such as `DO NOT EDIT`, `Code generated`,
  `@generated`).
- `--extra-excluded-prefixes` flag (strings, default `[]`) for additional UNIX
  path prefixes to exclude for ecosystems enry does not know (for example
  `.venv/`, `target/`, `.gradle/`).
- `source_file` field on every function record (relative path, for example
  `pkg/kubelet/kubelet.go`) for `static/complexity`, `static/halstead`,
  `static/cohesion`, and `static/comments`.
- `language` field on every function record (for example `go`, `bash`) for the
  same static analyzers.
- `directory` field on every function record (for example `pkg/kubelet`) for the
  same static analyzers.
- `start_time` and `end_time` fields (RFC 3339) on every history time-series
  tick for `history/sentiment`, `history/anomaly`, `history/quality`,
  `history/devs`, and `history/file-history`.
- `email` field on developer records, plus `primary_dev_email` /
  `secondary_dev_email` on bus-factor records and `developer1_email` /
  `developer2_email` on developer-coupling records, via a new
  `SplitIdentity` helper.
- Top-level `metadata` section in the report envelope with `repo_path`,
  `repo_name`, `analyzed_at` (RFC 3339), and `codefang_version`.
- Per-analyzer `schema` manifest in each analyzer result with field `type`,
  `grain`, and `description` for automated ETL generation across all 17
  analyzers.
- `clone_type_distribution` computed from the full pair population rather than
  the capped 1,000-pair sample, so Type-1/2/3 percentages are accurate on large
  codebases.

### Changed

- Complete rewrite from the original `src-d/hercules` project, into a modern Go
  codebase with idiomatic patterns, clean architecture, and comprehensive test
  coverage.
- Split the tool into two binaries: `uast` (Universal AST parser using
  Tree-sitter for 60+ languages) and `codefang` (analysis engine for static and
  history analysis). Unix philosophy: small tools joined by pipes.
- Replaced the original Babelfish/bblfsh parser with a Tree-sitter-based UAST
  that is faster, more reliable, and locally compiled, supporting 60+
  programming languages.
- Added a DSL-based UAST mapping layer with a custom domain-specific language
  for transforming Tree-sitter ASTs into standardized UAST nodes.
- Vendored libgit2 (`third_party/libgit2`), compiled as a static library for
  reproducible builds without external dependencies.
- Replaced all `log.Printf` and `fmt.Printf` calls with structured logging via
  `log/slog`. Instance loggers are enforced by `sloglint`.
- Removed all backward-compatibility fallbacks in the quality, anomaly,
  sentiment, and devs `ParseReportData` paths; formalized shotness shallow
  extraction; and integrated VADER (GoVader) for real sentiment analysis.
- Default analysis output across both phases now excludes vendor and generated
  files, matching the convention of mature multi-language analysers. Pass
  `--include-vendored --include-generated` to restore the previous behaviour.
  **Breaking change.**
- Made `--languages` cross-phase. Static analysis previously ignored the flag;
  it now narrows both static and history phases through a single
  `internal/analyzers/plumbing/langpath` source of truth, with fail-fast errors
  on unknown language tokens.
- Pushed the `--languages` filter down into libgit2 via a new `cf_tree_diff_v2`
  C ABI that forwards a pathspec to `git_diff_options.pathspec`, cutting tree-diff
  work on polyglot repositories with a narrow filter (about 34% wall-time and
  36% `cgocall` CPU reduction on a synthetic fixture).
- Replaced the `FileContentAnalyzer` + `WalksAllFiles` marker-interface pattern
  with explicit `StaticAnalyzer` (UAST) and `RawFileAnalyzer` (raw file)
  hierarchies sharing a `FormattableAnalyzer` base, driven by explicit pipeline
  stages. This enabled relative source paths and per-record language stamping.
- Changed `developers[].languages` from a map keyed by language name to a sorted
  `[]LanguageStatsEntry` array so it can be UNNEST'd in columnar warehouses.
  Empty language strings become `Other`. **Breaking change to output schema.**
- Changed `activity[].by_developer` from a `map[int]int` to a sorted
  `[]DeveloperCommits` array of `{dev_id, commits}`. **Breaking change to output
  schema.**
- Changed `file_contributors[].contributors` from a `map[int]LineStats` to a
  sorted `[]ContributorEntry` array of `{dev_id, added, removed, changed}`.
  **Breaking change to output schema.**
- Changed clone-pair `func_a` / `func_b` paths from absolute to relative.

### Deprecated

- `--skip-blacklist` is now a no-op (the new default already excludes vendor and
  generated files); a Cobra deprecation warning fires when it is passed.
- `--blacklisted-prefixes` is superseded by `--extra-excluded-prefixes`
  (identical semantics); a Cobra deprecation warning fires when it is passed.

### Fixed

- Fixed a data race in `internal/framework.PipelineSampler`: `t1Captured` was a
  plain `bool` read by the sampler goroutine and written by the caller, causing
  intermittent races under `go test -race`. It is now a `sync/atomic.Bool` using
  `CompareAndSwap`, so at most one t1 heap profile is captured. Removed the
  unused `t0Captured` field.

### Removed

- Removed `// FRD: specs/frds/FRD-...md` comments from all `.go` files;
  `specs/` is gitignored, so those references broke for anyone cloning the repo.
  Traceability now lives in FRDs and PR descriptions.

[Unreleased]: https://github.com/Sumatoshi-tech/codefang/commits/main
