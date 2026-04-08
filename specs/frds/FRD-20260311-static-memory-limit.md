# FRD: Set debug.SetMemoryLimit for static phase (Roadmap perf30/1.3)

**ID**: FRD-20260311-static-memory-limit
**Roadmap**: [specs/perf30/ROADMAP.md](../perf30/ROADMAP.md) — Step 1.3
**Date**: 2026-03-11

## Problem

History analyzers set `debug.SetMemoryLimit(budget)` so Go GC self-regulates as heap approaches
the limit. Static analyzers have no memory limit — GC only triggers at default GOGC thresholds.
This means the Go runtime doesn't know to collect aggressively during the static phase, allowing
heap to grow unchecked until system OOM.

## Decision

Reuse the existing `--memory-budget` flag. When set, call `debug.SetMemoryLimit` before the
static phase in `runStaticPhase()`. After the static phase completes, restore the previous value
so the history phase can set its own limit independently.

### Key design decisions

- **90% of budget**: Use `budget * 90 / 100` as the soft limit, matching the pattern in
  `framework/coordinator.go:applyMemoryLimitFromBudget`.
- **System RAM cap**: If budget exceeds available system RAM, cap at 90% of system RAM.
- **Restore after phase**: `debug.SetMemoryLimit` returns the previous limit. Restore it
  after the static phase so the history phase can set its own.
- **No budget = no action**: When `--memory-budget` is empty, skip entirely (current behavior).
- **Reuse parseMemoryBudgetBytes**: Extract a helper to parse the budget string to int64,
  reusable by both static and libgit2 config paths.

## Contract

- `--memory-budget` empty → no `debug.SetMemoryLimit` call for static phase.
- `--memory-budget` set → `debug.SetMemoryLimit(budget * 90%)` before static exec.
- Previous limit restored after static phase completes.
- `debug.SetMemoryLimit` is safe to call multiple times (idempotent within a process).

## Acceptance Criteria

- [x] `parseMemoryBudgetBytes` helper extracted in `run.go`
- [x] `applyStaticMemoryLimit` called in `runStaticPhase` when budget is set
- [x] Previous limit restored after static phase
- [x] Unit test verifies limit is applied and restored
- [x] `go test ./cmd/codefang/commands/...` passes
- [x] `make lint` passes

## Implementation

Files created:
- `specs/frds/FRD-20260311-static-memory-limit.md` (this file)

Files modified:
- `cmd/codefang/commands/run.go` — `parseMemoryBudgetBytes`, `applyStaticMemoryLimit`, `staticMemoryLimitRatio`/`staticMemoryLimitDivisor` constants, `runtime/debug` import, wiring in `runStaticPhase`
- `cmd/codefang/commands/run_test.go` — `TestParseMemoryBudgetBytes_Valid`, `_Empty`, `_Invalid`, `TestApplyStaticMemoryLimit_ZeroBudget`, `_SetsAndRestores`
- `specs/perf30/ROADMAP.md` — closed Step 1.3, added FRD link and key files
