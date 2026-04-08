# FRD: Cap static worker count (Roadmap perf30/1.1)

**ID**: FRD-20260311-cap-static-workers
**Roadmap**: [specs/perf30/ROADMAP.md](../perf30/ROADMAP.md) — Step 1.1
**Date**: 2026-03-11

## Problem

`StaticService.analyzeFilesParallel` creates a `WorkerPool[string]` with `MaxParallel=0`,
which defaults to `runtime.NumCPU()`. On a 96-core machine this means 96 concurrent UAST
parse trees (100KB–10MB each in tree-sitter native memory) plus 96 Go-side `*node.Node`
trees held simultaneously.

When running `codefang -a static/*` on ~/sources/kubernetes (~25K+ files), this spikes
RSS to 50GB and triggers OOM.

## Decision

Add a `MaxWorkers` field to `StaticService`:
- Default: `min(runtime.NumCPU(), defaultStaticMaxWorkers)` where `defaultStaticMaxWorkers = 8`
- Passed through to `WorkerPool.MaxParallel` in `analyzeFilesParallel`

Add a `--static-workers` CLI flag in `cmd/codefang/commands/run.go`:
- Type: `int`
- Default: `0` (use `StaticService` default)
- When non-zero, sets `StaticService.MaxWorkers`

### Key design decisions

- **Default cap of 8**: Balances throughput with memory. 8 concurrent UAST parse trees ×
  10MB ≈ 80MB peak vs 96 × 10MB ≈ 960MB. Tree-sitter parsing is CPU-bound per-file but
  memory-heavy when parallel.
- **Zero means auto**: Consistent with `WorkerPool` convention where 0 = sensible default.
- **No breaking change**: `NewStaticService` continues to work without setting `MaxWorkers`;
  `resolveMaxWorkers()` applies the cap internally.

## Contract

- `MaxWorkers=0` resolves to `min(runtime.NumCPU(), 8)`.
- `MaxWorkers>0` is used as-is (user override).
- `analyzeFilesParallel` uses the resolved value for `WorkerPool.MaxParallel`.
- All existing tests continue to pass unchanged.

## Acceptance Criteria

- [x] `StaticService.MaxWorkers` field exists (public, int)
- [x] `ResolveMaxWorkers()` method returns effective worker count
- [x] `analyzeFilesParallel` passes resolved value to `WorkerPool.MaxParallel`
- [x] `--static-workers` flag wired in run.go
- [x] Unit tests cover: default resolution, explicit override, cap behavior
- [x] Benchmark `BenchmarkStaticPeakParsers` shows bounded concurrency
- [x] `go test ./internal/analyzers/analyze/...` passes
- [x] `make lint` passes

## Implementation

Files created:
- `specs/frds/FRD-20260311-cap-static-workers.md` (this file)
- `internal/analyzers/analyze/static_bench_test.go` — `BenchmarkStaticPeakParsers`, `TestStaticPeakParsers_BoundedConcurrency`

Files modified:
- `internal/analyzers/analyze/static.go` — `MaxWorkers` field, `DefaultStaticMaxWorkers` constant, `ResolveMaxWorkers()` method, wiring to `WorkerPool.MaxParallel`
- `internal/analyzers/analyze/static_test.go` — unit tests: `TestStaticService_ResolveMaxWorkers_DefaultCapsAtEight`, `TestStaticService_ResolveMaxWorkers_ExplicitOverride`, `TestStaticService_AnalyzeFolder_RespectsMaxWorkers`
- `cmd/codefang/commands/run.go` — `staticWorkers` field, `--static-workers` CLI flag, `staticExecutor` signature updated, `runStaticAnalyzers` wires `MaxWorkers`
- `cmd/codefang/commands/run_test.go` — updated mock `staticExecutor` signatures
- `cmd/codefang/commands/run_plot_test.go` — updated mock `staticExecutor` signatures
- `cmd/codefang/commands/run_config_test.go` — updated mock `staticExecutor` signatures
- `specs/perf30/ROADMAP.md` — closed Step 1.1, added FRD link and key files
