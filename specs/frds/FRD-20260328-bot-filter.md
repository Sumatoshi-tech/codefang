# FRD-20260328: Bot Author Filter

**Date:** 2026-03-28
**Author:** Agent
**Status:** Complete
**Roadmap:** specs/filestats/ROADMAP.md — Step 3.2
**Spec:** specs/filestats/SPEC.md — FR-3.2

## Problem

Bot accounts (Dependabot, GitHub Actions, Renovate) inflate contributor statistics and pollute workload charts. Users need `--exclude-bots` to automatically filter known bots and `--exclude-author` for custom patterns.

## Solution

Create `BotFilter` in `internal/plumbing/` with built-in patterns for common CI bots and support for custom patterns.

### Type

```go
type BotFilter struct {
    customPatterns []string
}
```

### Built-in Patterns

A name or email is considered a bot if it matches any of:
- Contains `[bot]` (case-insensitive)
- Contains `github-actions` (case-insensitive)
- Contains `dependabot` (case-insensitive)
- Contains `renovate` (case-insensitive)
- Contains `noreply@` (case-insensitive)

### API

```go
func NewBotFilter(customPatterns ...string) *BotFilter
func (f *BotFilter) IsBot(name, email string) bool
```

`IsBot` returns true if either name or email matches a built-in pattern or any custom pattern (substring match, case-insensitive).

## Test Plan

- Known bots: dependabot[bot], github-actions[bot], renovate[bot] detected.
- Humans: alice@example.com NOT detected.
- Custom patterns: match works.
- Case insensitivity.
- Empty filter: no bots detected.

## Implementation

- `internal/plumbing/bot_filter.go`
- `internal/plumbing/bot_filter_test.go`
