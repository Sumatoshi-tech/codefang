# FRD: Bounded parser pool (Roadmap perf30/1.4)

**ID**: FRD-20260311-bounded-parser-pool
**Roadmap**: [specs/perf30/ROADMAP.md](../perf30/ROADMAP.md) — Step 1.4
**Date**: 2026-03-11

## Problem

`sync.Pool` in `analyzeFilesParallel` is unbounded and GC-dependent. Parsers (and their
internal tree-sitter state) can accumulate beyond the MaxWorkers count because `sync.Pool`
may grow during GC pauses and items are lazily collected. This means more than `MaxWorkers`
parsers could exist simultaneously, undermining the memory cap from Step 1.1.

## Decision

Replace `sync.Pool` with a channel-based pool `chan *uast.Parser` of size `MaxWorkers`.
This guarantees at most N parsers exist at any time.

### Key design decisions

- **Channel of size MaxWorkers**: Workers block on receive when all parsers are in use,
  providing natural backpressure.
- **Lazy creation**: Channel starts empty. Workers create parsers on demand up to capacity.
- **No separate `getOrCreateParser` method**: The logic simplifies to a non-blocking receive
  from the channel (try `select` with default) + fallback to `uast.NewParser()`.
- **Return via send**: After use, parser is sent back to the channel (non-blocking, since
  channel size >= number of workers).

## Contract

- At most `ResolveMaxWorkers()` parsers exist simultaneously.
- Workers that can't get a parser from the channel create one (up to channel capacity).
- Parser is returned to channel after each file analysis.
- All existing tests pass unchanged.

## Acceptance Criteria

- [x] `sync.Pool` replaced with `chan *uast.Parser` in `analyzeFilesParallel`
- [x] Channel capacity = `ResolveMaxWorkers()`
- [x] `getOrCreateParser` replaced with `acquireParser` using channel
- [x] `go test -race ./internal/analyzers/analyze/...` passes
- [x] Benchmark `BenchmarkStaticParserPool` shows bounded parser count
- [x] `make lint` passes

## Implementation

Files created:
- `specs/frds/FRD-20260311-bounded-parser-pool.md` (this file)

Files modified:
- `internal/analyzers/analyze/static.go` — replaced `sync.Pool` + `getOrCreateParser` with `chan *uast.Parser` + `acquireParser`, removed `sync.Pool` import usage
- `internal/analyzers/analyze/static_bench_test.go` — `BenchmarkStaticParserPool`
- `specs/perf30/ROADMAP.md` — closed Step 1.4, added FRD link and key files
