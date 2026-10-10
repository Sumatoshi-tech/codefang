<!-- Authored by /documenter release-notes (Good Docs Project release-notes template). Driver: /march Item 12 (R-RELEASE-04). -->
<!-- Content is anchored to the single [Unreleased] block in CHANGELOG.md.proposed.md. No tagged release exists yet, -->
<!-- so this page is framed as the upcoming first release line. Do not add a version number or release date until the -->
<!-- first v* tag is cut by the GoReleaser pipeline (.github/workflows/ci.yml). -->

# Upcoming release

These notes describe the upcoming first release line of Codefang. No `v*` tag has been cut yet, so this page tracks everything staged in the `[Unreleased]` section of the [changelog](../contributing/changelog.md). When the first tag ships, the GoReleaser pipeline in the [CI/CD workflow](https://github.com/Sumatoshi-tech/codefang/blob/main/.github/workflows/ci.yml) publishes the matching [GitHub Release](https://github.com/Sumatoshi-tech/codefang/releases), and these notes are promoted to a versioned page.

## Highlights

- Codefang is a complete rewrite of the original `src-d/hercules` project into a modern, idiomatic Go codebase with clean architecture and comprehensive tests.
- The tool ships as two composable binaries: `uast` (a Tree-sitter-based Universal AST parser for 60+ languages) and `codefang` (the static and history analysis engine).
- Several output formats target both humans and data warehouses: `timeseries`, `plot` (interactive HTML), and `ndjson` (streaming, warehouse-friendly).
- An MCP server (`codefang mcp`) exposes analysis to AI agents over the Model Context Protocol.
- One shared history walk per run: every history analyzer reduces the same commit window in chronological order, and the window flags (`--limit`, `--since`, `--first-parent`, `--max-changes-per-commit`, `--languages`) bound what a run costs.
- Every `codefang` flag and subcommand listed by `--help` has an effect; the configuration file (`--config`, `./.codefang.yaml`, `$HOME/.codefang.yaml`) supplies the defaults of the `run` flags.

## Upgrade notes

This is the first release line, so there is no prior version to upgrade from. If you tracked an unreleased build, note these behavior changes before adopting the release:

- **Default file filtering changed.** Analysis now excludes vendored and generated files by default, matching mature multi-language analyzers. To restore the previous behavior, pass `--include-vendored --include-generated`.
- **`--languages` is now cross-phase.** It narrows both static and history analysis through a single source of truth and fails fast on unknown language tokens. Pipelines that relied on static analysis ignoring this flag must drop it or supply valid tokens.
- **Output schema changes.** Several fields moved from maps to sorted arrays so they can be UNNEST'd in columnar warehouses: `developers[].languages`, `activity[].by_developer`, and `file_contributors[].contributors`. Clone-pair `func_a` / `func_b` paths are now relative. Update any consumers that read these fields.
- **Removed flags.** Flags that were accepted but had no effect are gone and now fail as unknown arguments: `--checkpoint`, `--checkpoint-dir`, `--resume`, `--clear-checkpoint`, `--cache-dir`, `--no-cache`, `--debug-trace`, `--skip-blacklist`, `--blacklisted-prefixes` (use `--extra-excluded-prefixes`), and the Go-era tuning flags (`--memory-budget`, `--workers`, `--static-workers`, `--gogc`, `--ballast-size`, the cache / arena / batch sizes, `--tmp-dir`, `--keep-store`, `--input`, `--input-format`, …). Drop them from scripts. The `codefang render` subcommand is removed; use `codefang run --format plot -o <dir>`.
- **Configuration keys.** Only the keys that map to a live `run` flag are read (`analyzers`, `pipeline.max_changes_per_commit`, `history.tick_size`, burndown granularity / sampling / tracking, devs empty-commits / anonymize, sentiment minimum length, shotness DSL, typos distance, anomaly threshold / window). Every other key — the Go-era `pipeline:` tuning tree, `checkpoint:`, hibernation and analyzer threshold keys — is ignored with one `warning: ignoring unsupported config keys: …` line.

## Added

- Temporal anomaly detection analyzer (`history/anomaly`) using Z-score statistical analysis over sliding windows to detect sudden quality degradation in commit history.
- TimeSeries output format (`--format timeseries`) that merges all analyzer outputs into a single chronologically ordered JSON array.
- Plot output format (`--format plot`) generating interactive HTML charts via go-echarts.
- NDJSON output format (`--format ndjson`) emitting one JSON line per analyzer result (with an optional metadata line) for streaming ingestion into warehouses such as ClickHouse.
- MCP server (`codefang mcp`) exposing analysis capabilities as tools for AI agents via the Model Context Protocol (stdio transport, JSON-RPC 2.0), with the `codefang_analyze`, `uast_parse`, and `codefang_history` tools.
- Docker support with a multi-stage Dockerfile and a `debian:bookworm-slim` runtime image that runs as a non-root `codefang` user by default.
- GitHub Actions integration (`action.yml`) for automated code-quality checks in CI pipelines with configurable analyzers, formats, and quality gates.
- Configuration system (`.codefang.yaml`, explicit `--config <file>`, else `./.codefang.yaml`, else `$HOME/.codefang.yaml`) with file, `CODEFANG_*` environment-variable and CLI-flag support. Merge priority: explicit CLI flag > env > file > compiled defaults.
- `-o <file>` writes any non-plot report to a file instead of stdout; `-q` / `--quiet` (alias `--silent`) suppresses the `progress:` lines and `-v` / `--verbose` adds the config file and resolved analyzers to them.
- Large-scale scanning support for fleet analysis of thousands of repositories, with bare-repo support, GNU Parallel and Kubernetes orchestration patterns, and data-warehouse loading guides (Athena, Snowflake, Spark).
- Deep context propagation that eliminates `context.Background()` calls in production hot paths for end-to-end tracing.
- Attribute-filter span processor that enforces an allow-list of attribute key prefixes to prevent PII leakage to collectors.
- Health-check endpoints (`/healthz`, `/readyz`, `/metrics`) for server-mode deployments.
- Incremental scanning with the `--since` flag, supporting Go durations, date strings, and RFC 3339 timestamps.
- `--include-vendored` flag (bool, default `false`) to re-include paths detected as vendored by enry/Linguist (`vendor/`, `node_modules/`, `third_party/`, `testdata/`, `dist/`, minified bundles, and more).
- `--include-generated` flag (bool, default `false`) to re-include auto-generated files (`*.pb.go`, `zz_generated_*.go`, `*_pb2.py`, `*.min.js`, and content-header markers such as `DO NOT EDIT`, `Code generated`, `@generated`).
- `--extra-excluded-prefixes` flag (strings, default `[]`) for additional UNIX path prefixes to exclude for ecosystems enry does not know (for example `.venv/`, `target/`, `.gradle/`).
- `source_file` field on every function record (relative path, for example `pkg/kubelet/kubelet.go`) for `static/complexity`, `static/halstead`, `static/cohesion`, and `static/comments`.
- `language` field on every function record (for example `go`, `bash`) for the same static analyzers.
- `directory` field on every function record (for example `pkg/kubelet`) for the same static analyzers.
- `start_time` and `end_time` fields (RFC 3339) on every history time-series tick for `history/sentiment`, `history/anomaly`, `history/quality`, `history/devs`, and `history/file-history`.
- `email` field on developer records, plus `primary_dev_email` / `secondary_dev_email` on bus-factor records and `developer1_email` / `developer2_email` on developer-coupling records, via a new `SplitIdentity` helper.
- Top-level `metadata` section in the report envelope with `repo_path`, `repo_name`, `analyzed_at` (RFC 3339), and `codefang_version`.
- Per-analyzer `schema` manifest in each analyzer result with field `type`, `grain`, and `description` for automated ETL generation across all 17 analyzers.
- `clone_type_distribution` computed from the full pair population rather than the capped 1,000-pair sample, so Type-1/2/3 percentages are accurate on large codebases.

## Changed

- Complete rewrite from the original `src-d/hercules` project into a modern Go codebase with idiomatic patterns, clean architecture, and comprehensive test coverage.
- Split the tool into two binaries: `uast` (Universal AST parser using Tree-sitter for 60+ languages) and `codefang` (analysis engine for static and history analysis). Unix philosophy: small tools joined by pipes.
- Replaced the original Babelfish/bblfsh parser with a Tree-sitter-based UAST that is faster, more reliable, and locally compiled, supporting 60+ programming languages.
- Added a DSL-based UAST mapping layer with a custom domain-specific language for transforming Tree-sitter ASTs into standardized UAST nodes.
- Vendored libgit2 (`third_party/libgit2`), compiled as a static library for reproducible builds without external dependencies.
- Replaced all `log.Printf` and `fmt.Printf` calls with structured logging via `log/slog`. Instance loggers are enforced by `sloglint`.
- Removed all backward-compatibility fallbacks in the quality, anomaly, sentiment, and devs `ParseReportData` paths; formalized shotness shallow extraction; and integrated VADER (GoVader) for real sentiment analysis.
- Default analysis output across both phases now excludes vendor and generated files, matching the convention of mature multi-language analyzers. Pass `--include-vendored --include-generated` to restore the previous behavior. **Breaking change.**
- Made `--languages` cross-phase. Static analysis previously ignored the flag; it now narrows both static and history phases through a single `internal/analyzers/plumbing/langpath` source of truth, with fail-fast errors on unknown language tokens.
- Pushed the `--languages` filter down into libgit2 via a new `cf_tree_diff_v2` C ABI that forwards a pathspec to `git_diff_options.pathspec`, cutting tree-diff work on polyglot repositories with a narrow filter (about 34% wall-time and 36% `cgocall` CPU reduction on a synthetic fixture).
- Replaced the `FileContentAnalyzer` + `WalksAllFiles` marker-interface pattern with explicit `StaticAnalyzer` (UAST) and `RawFileAnalyzer` (raw file) hierarchies sharing a `FormattableAnalyzer` base, driven by explicit pipeline stages. This enabled relative source paths and per-record language stamping.
- Changed `developers[].languages` from a map keyed by language name to a sorted `[]LanguageStatsEntry` array so it can be UNNEST'd in columnar warehouses. Empty language strings become `Other`. **Breaking change to output schema.**
- Changed `activity[].by_developer` from a `map[int]int` to a sorted `[]DeveloperCommits` array of `{dev_id, commits}`. **Breaking change to output schema.**
- Changed `file_contributors[].contributors` from a `map[int]LineStats` to a sorted `[]ContributorEntry` array of `{dev_id, added, removed, changed}`. **Breaking change to output schema.**
- Changed clone-pair `func_a` / `func_b` paths from absolute to relative.

## Removed

- Accepted-but-inert `codefang run` flags: `--checkpoint`, `--checkpoint-dir`, `--resume`, `--clear-checkpoint`, `--cache-dir`, `--no-cache`, `--debug-trace`, the hidden `--skip-blacklist` / `--blacklisted-prefixes`, and the Go-era memory / worker / cache / profiling flags. The `codefang render` subcommand (it only printed "not available").

- Removed `// FRD: specs/frds/FRD-...md` comments from all `.go` files; `specs/` is gitignored, so those references broke for anyone cloning the repo. Traceability now lives in FRDs and PR descriptions.

## Fixed

- Full-report schemas now declare exactly the fields each analyzer emits. `history/typos`, `history/imports`, `history/burndown` (`interactions`), and `static/clones` (`message`) had registry entries that named fields the reports did not contain, or omitted fields they did contain.
- A shallow clone no longer produces empty history reports silently. History analyzers still see no commits in a shallow repository, but the run prints one `warning: shallow clone` line on stderr naming the remedy (`git fetch --unshallow`).
- Fixed a data race in `internal/framework.PipelineSampler`: `t1Captured` was a plain `bool` read by the sampler goroutine and written by the caller, causing intermittent races under `go test -race`. It is now a `sync/atomic.Bool` using `CompareAndSwap`, so at most one t1 heap profile is captured. Removed the unused `t0Captured` field.

## Security

- No security fixes are staged for this release.

## Known issues

- No tagged release exists yet, so the changelog `[Unreleased]` reference link points at the commits view rather than a version compare base. The link is replaced with a compare base once the first `v*` tag ships.
- Several output-schema changes listed under [Changed](#changed) are breaking. Consumers that parsed the previous map-shaped fields must migrate to the sorted-array shapes before adopting this release.
