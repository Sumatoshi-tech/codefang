# FRD: Replace joinTypes with strings.Join (Phase 1.2)

**ID**: FRD-20260317-jointypes-stdlib
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Phase 1.2
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Section 1 Stdlib Replacements

## Problem

`joinTypes` in internal/analyzers/clones/shingler.go duplicates stdlib functionality. `strings.Join(types, shingleSeparator)` provides identical semantics.

## Goal

Remove `joinTypes` and use `strings.Join` in `buildShingle`.

## In Scope

- Replace `joinTypes(types)` with `strings.Join(types, shingleSeparator)` in buildShingle
- Remove `joinTypes` from shingler.go
- Update TestJoinTypes to TestBuildShingle (test buildShingle output, which uses strings.Join)

## Out of Scope

- Changing ExtractShingles or collectNodeTypes logic

## Acceptance Criteria

- [x] joinTypes removed from shingler.go
- [x] buildShingle uses strings.Join
- [x] `go test ./internal/analyzers/clones/...` passes
- [x] `make lint` passes
- [x] Clone detection produces identical results (behavioral equivalence)

## Implementation

- Modified: internal/analyzers/clones/shingler.go (buildShingle uses strings.Join; joinTypes removed)
- Modified: internal/analyzers/clones/analyzer_test.go (TestJoinTypes → TestBuildShingle)
