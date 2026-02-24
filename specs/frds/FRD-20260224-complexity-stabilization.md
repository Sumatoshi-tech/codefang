# FRD: Complexity Analyzer Stabilization

**Date:** 2026-02-24  
**Status:** Implemented  
**Analyzer:** `static/complexity`

## Problem Statement

After rapid feature growth, complexity metrics drifted from industry methodology and produced inconsistent output quality:

1. Visitor path (used by CLI) omitted cognitive complexity entirely.
2. Cyclomatic complexity undercounted logical operators and switch cases in Go.
3. Default `case` handling diverged from battle-tested Go tooling.
4. Cognitive complexity nesting/else-if handling diverged from Sonar/gocognit behavior.
5. Terminal issue ordering used lexical sort (`CC=9` above `CC=12`) and issue payload lacked context.

These errors are high risk because this analyzer is used for team and individual assessment decisions.

## Methodology Research (Phase 1)

### Industry baseline
- Cyclomatic complexity (McCabe): baseline 1 + decision-point increments.
- Sonar cognitive complexity: structural flow breaks + nesting penalty, with special handling for `else if` and logical operator sequences.

### Golden references selected
- **Cyclomatic:** `gocyclo` v0.6.0
- **Cognitive:** `gocognit` v1.2.1

### Golden rules encoded
- Cyclomatic increments for `if`, `for/range`, non-default `case`, and each `&&` / `||`.
- Default `case` does not increment.
- Cognitive increments:
  - `if`, `else if`, `for`, `switch` (+ nesting penalty except `else if`)
  - `else` branch (+1)
  - logical operator sequences (+1 for first op, +1 on operator change)
  - direct recursion (+1)

## Discrepancy Findings

Controlled sample comparison before fixes showed:
- CLI reported `Cognitive Total = 0` while golden cognitive sum was non-zero.
- `BoolChain` and `SwitchBranches` were undercounted in cyclomatic output.
- Sorting by string value in terminal issue list produced unstable/non-numeric ordering.

## Implemented Fixes

1. **Unified visitor and analyzer logic**
   - Visitor now collects function nodes and delegates metric computation to the same analyzer logic as direct analysis.

2. **Cyclomatic parity fix**
   - Decision points updated to match golden behavior:
     - `if`, `loop`, `catch`
     - non-default `case`
     - logical binary operators (`&&`, `||`)
   - Added operator extraction fallback using source offsets when UAST omits operator tokens.

3. **Cognitive parity fix**
   - Replaced previous stack-based implementation with structured traversal aligned to gocognit/Sonar semantics:
     - nesting-aware increments
     - explicit `else if` treatment
     - logical operator sequence complexity
     - direct recursion increment

4. **Nesting depth + LOC quality fixes**
   - Nesting depth now tracks control-flow nesting (not block/function noise).
   - LOC now uses source positions when available for realistic per-function LOC.

5. **Terminal UX improvements (Phase 2)**
   - Top issues now include all key context in one line: `CC`, `Cog`, `Nest`.
   - Issue sorting now numeric and deterministic: cyclomatic, then cognitive, then nesting, then name.

## Documentation Updates

- Updated `site/analyzers/complexity.md` to reflect corrected scoring semantics and output interpretation.

## Acceptance Criteria

- [x] Golden comparison parity for methodology sample:
  - Cyclomatic parity with `gocyclo` v0.6.0
  - Cognitive parity with `gocognit` v1.2.1
- [x] Visitor and analyzer paths produce consistent metrics.
- [x] `make lint` passes.
- [x] `make test` passes.
- [x] Coverage for complexity package >= 80% (achieved: 84.2%).
