# FRD: ParseSourceFile Helper in pkg/uast (Roadmap 7.2)

**ID**: FRD-20260310-parse-source-file
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 7.2
**Date**: 2026-03-10

## Problem

Six `cmd/uast` commands repeat the same three-step pattern: read a source file
from disk, optionally override the language via filename mangling, then call
`parser.Parse`. The pattern appears in:

- `cmd/uast/parse.go` — `parseOnly`, `parseFileWithParser` (2 sites)
- `cmd/uast/query.go` — `parseFileForQuery`, `loadInteractiveInputFromFile` (2 sites)
- `cmd/uast/explore.go` — `parseExploreFile` (1 site)
- `cmd/uast/diff.go` — `runDiff` (2 sites, no lang override, uses `os.ReadFile`)

Each site independently handles file reading, filename construction for language
override, and error wrapping — duplicating ~10 lines of boilerplate.

## Decision

Add two new functions to `pkg/uast`:

1. **`Parser.ParseFile`** — method on existing `Parser`; reads the file via
   `iosafety.ReadFile`, applies optional language override, delegates to `Parse`.
   Suitable for callers that already hold a `*Parser` (parallel paths, diff).

2. **`ParseSourceFile`** — standalone convenience; creates a `Parser` then calls
   `ParseFile`. Suitable for one-shot callers (explore, query single-file).

```go
// ParseFile reads a source file from disk and returns its UAST.
// If lang is non-empty, it overrides language detection derived from the file extension.
func (p *Parser) ParseFile(ctx context.Context, path, lang string) (*node.Node, error)

// ParseSourceFile creates a parser, reads the source file at path, and returns its UAST.
// If lang is non-empty, it overrides language detection.
func ParseSourceFile(ctx context.Context, path, lang string) (*node.Node, error)
```

## Contract

- `ParseFile` reads via `iosafety.ReadFile` (path validation, NUL rejection, resolve).
- Language override: when `lang != ""`, the resolved filename's extension is replaced
  with `"." + lang` before calling `Parse`, matching the existing pattern in all callers.
- `ParseSourceFile` creates a new `Parser` per call — callers needing parser reuse
  should use `Parser.ParseFile` directly.
- Error wrapping preserves the original cause for `errors.Is` compatibility.

## Scope

### Files modified

| File | Change |
|------|--------|
| `pkg/uast/parsefile.go` | New file: `ParseFile` method and `ParseSourceFile` function |
| `pkg/uast/parsefile_test.go` | New file: tests for both functions |
| `cmd/uast/parse.go` | `parseOnly` and `parseFileWithParser` use `Parser.ParseFile` |
| `cmd/uast/explore.go` | `parseExploreFile` uses `Parser.ParseFile` |
| `cmd/uast/diff.go` | `runDiff` uses `Parser.ParseFile` for both files |

### Out of scope

- `cmd/uast/query.go` — has JSON-fallback logic that makes extraction less clean;
  can be migrated later.
- `cmd/uast/analyze.go` — uses `os.ReadFile` in a parallel hot path; switching to
  `iosafety.ReadFile` is a deliberate behavioral change deferred to a separate step.
- `cmd/uast/server.go` — parses from request body, not from disk.
- Modifying `Parse`, `NewParser`, or any Tree-sitter internals.

## Acceptance Criteria

- [x] `Parser.ParseFile` implemented in `pkg/uast/parsefile.go`
- [x] `ParseSourceFile` implemented as standalone convenience
- [x] Tests cover: success path, language override, empty lang (auto-detect), file-not-found error
- [x] At least 3 cmd/uast commands updated to use the helper
- [x] `go test ./pkg/uast/...` passes
- [x] `go build ./cmd/uast/...` passes
- [x] `make lint` passes — 0 issues, no dead code

## Implementation

### Files Created

| File | Change |
|------|--------|
| `pkg/uast/parsefile.go` | `Parser.ParseFile` method + `ParseSourceFile` standalone convenience |
| `pkg/uast/parsefile_test.go` | 6 tests: ParseFile success, lang override, auto-detect, file-not-found; ParseSourceFile success, file-not-found |

### Files Modified

| File | Change |
|------|--------|
| `cmd/uast/parse.go` | `parseOnly` and `parseFileWithParser` delegate to `Parser.ParseFile`; removed `iosafety` and `strings` imports |
| `cmd/uast/explore.go` | `parseExploreFile` delegates to `Parser.ParseFile`; removed `filepath` import |
| `cmd/uast/diff.go` | `runDiff` uses `Parser.ParseFile` for both files; removed `os.ReadFile` calls |
| `.deadcode-whitelist` | Added `ParseSourceFile` (public API, tested, no current cmd caller) |
| `specs/ref/ROADMAP.md` | Marked 7.2 done, added FRD link |
