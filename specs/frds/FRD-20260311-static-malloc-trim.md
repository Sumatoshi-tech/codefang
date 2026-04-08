# FRD: Periodic malloc_trim in static worker loop (Roadmap perf30/1.2)

**ID**: FRD-20260311-static-malloc-trim
**Roadmap**: [specs/perf30/ROADMAP.md](../perf30/ROADMAP.md) — Step 1.2
**Date**: 2026-03-11

## Problem

tree-sitter allocates via glibc malloc. Even after Go-side parse tree is freed, glibc retains
arenas. History analyzers call `gitlib.ReleaseNativeMemory()` (= `malloc_trim(0)`) every 10
commits. Static analyzers never call it.

On kubernetes (~25K files), tree-sitter native memory accumulates across all worker iterations.
Without periodic trimming, RSS grows monotonically even though each parse tree is short-lived.

## Decision

Add an atomic file counter in the `analyzeFilesParallel` worker closure. Every N files
(configurable, default 50), call `gitlib.ReleaseNativeMemory()` to release glibc arenas back
to the OS.

### Key design decisions

- **Default interval of 50 files**: Balances trim overhead (~1ms per call) vs memory savings.
  At 8 workers processing ~3K files/sec, this means ~6 trims/sec — negligible CPU cost.
- **Zero means default**: `MallocTrimInterval=0` resolves to `DefaultMallocTrimInterval` (50).
- **Negative disables**: `MallocTrimInterval<0` disables trimming entirely (for benchmarking).
- **Testable via function field**: `NativeMemoryReleaseFn` on `StaticService` allows tests to
  inject a counter instead of calling the real `malloc_trim`.
- **No breaking change**: `NewStaticService` works without setting `MallocTrimInterval`.

## Contract

- `MallocTrimInterval=0` resolves to `DefaultMallocTrimInterval` (50).
- `MallocTrimInterval>0` is used as-is (user override).
- `MallocTrimInterval<0` disables periodic trimming.
- `NativeMemoryReleaseFn` defaults to `gitlib.ReleaseNativeMemory` when nil.
- Trim is called when `fileCounter % interval == 0`, where `fileCounter` is a shared atomic.
- All existing tests continue to pass unchanged.

## Acceptance Criteria

- [x] `DefaultMallocTrimInterval` constant (50)
- [x] `MallocTrimInterval` field on `StaticService`
- [x] `ResolveMallocTrimInterval()` method
- [x] `NativeMemoryReleaseFn` field for testability
- [x] Atomic counter in worker closure triggers trim every N files
- [x] Unit tests cover: default resolution, explicit override, disable, trim invocation
- [x] Benchmark `BenchmarkStaticMallocTrim` shows RSS reduction
- [x] `go test ./internal/analyzers/analyze/...` passes
- [x] `make lint` passes

## Implementation

Files created:
- `specs/frds/FRD-20260311-static-malloc-trim.md` (this file)

Files modified:
- `internal/analyzers/analyze/static.go` — `DefaultMallocTrimInterval` constant, `MallocTrimInterval` and `NativeMemoryReleaseFn` fields, `ResolveMallocTrimInterval()` and `resolveReleaseFn()` methods, atomic file counter + trim call in `analyzeFilesParallel`
- `internal/analyzers/analyze/static_test.go` — `TestStaticService_ResolveMallocTrimInterval_Default`, `_ExplicitOverride`, `_Disabled`, `TestStaticService_AnalyzeFolder_CallsMallocTrim`, `_NoTrimWhenDisabled`
- `internal/analyzers/analyze/static_bench_test.go` — `BenchmarkStaticMallocTrim` with trim-enabled/trim-disabled sub-benchmarks
- `specs/perf30/ROADMAP.md` — closed Step 1.2, added FRD link and key files
