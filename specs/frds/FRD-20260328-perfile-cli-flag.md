# FRD-20260328: CLI Flag --per-file / -F

**Date:** 2026-03-28
**Author:** Agent
**Status:** Complete
**Roadmap:** specs/filestats/ROADMAP.md — Step 1.6
**Spec:** specs/filestats/SPEC.md — Feature 1 (FR-1.1)

## Problem

The per-file output pipeline (steps 1.1-1.5) is fully implemented but has no CLI entry point. Users need a `--per-file` flag to activate it.

## Solution

Add `--per-file` / `-F` boolean flag to `codefang run`. Wire through `staticExecutor` to `StaticService.PerFile`.

## Changes

1. `RunCommand.perFile bool` field.
2. `staticExecutor` type gets `perFile bool` parameter.
3. `runStaticAnalyzers` sets `service.PerFile = perFile`.
4. Flag registered with help text.
5. Test verifies flag propagation.

## Implementation

**Status:** Complete

**Files modified:**
- `cmd/codefang/commands/run.go` — `perFile` field, `--per-file` / `-F` flag, `staticExecutor` type, `runStaticAnalyzers` wiring
- `cmd/codefang/commands/run_test.go` — 3 new tests (propagation, short alias, default false), all stubs updated
- `cmd/codefang/commands/run_plot_test.go` — stub signature updated

**Coverage:** 3 new CLI tests, all PASS. All existing 30+ tests continue to pass.
**Lint:** Clean.
