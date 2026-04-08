# FRD-20260408: Flatten developers[].languages to array

## Roadmap Link
- Source roadmap: specs/analytics-readiness/roadmap.md
- Feature: Feature 5 — Flatten nested dicts to arrays

## Problem

`DeveloperData.Languages` is `map[string]LineStats` — a JSON object where keys are language names (variable, high cardinality). DWH systems cannot UNNEST this without custom ETL. Array format is directly loadable.

## Context

Other "dict-like" fields (`z_scores`, `metrics`, `stats`) are actually typed structs with fixed schemas — they don't need flattening. `composition.breakdown` has only 8 stable categories — also fine as-is.

Only `developers[].languages` is a true variable-key map that benefits from array conversion.

## Goal

Change `DeveloperData.Languages` from `map[string]LineStats` to `[]LanguageStatsEntry` where each entry has a `language` field.

## Functional Requirements

### MUST
- `LanguageStatsEntry` struct: `{Language, Added, Removed, Changed}`
- `DeveloperData.Languages` changes from `map[string]pkgplumbing.LineStats` to `[]LanguageStatsEntry`
- Entries sorted by language name for deterministic output
- All compute paths updated

## Implementation

### Changes
- `DeveloperData.Languages` changed from `map[string]pkgplumbing.LineStats` to `[]LanguageStatsEntry`
- Added `LanguageStatsEntry` struct with `Language`, `Added`, `Removed`, `Changed` fields
- Internal accumulation uses `langMap map[string]LineStats` (unexported), converted to sorted array via `finalizeLanguages()` in `collectDevResults`
- `LanguagesMetric.Compute()` updated to iterate slice instead of map
- Dashboard files updated with `devLanguageMap()` helper for lookup-by-name
- Anomaly `z_scores`/`metrics` and quality `stats` are typed structs (NOT maps) — no flattening needed

### Files modified
- `internal/analyzers/devs/metrics.go` — `DeveloperData`, `LanguageStatsEntry`, `finalizeLanguages`, compute functions
- `internal/analyzers/devs/metrics_test.go` — updated test literals, added `findLang` helper
- `internal/analyzers/devs/dashboard_workload.go` — updated language iteration
- `internal/analyzers/devs/dashboard_languages.go` — added `devLanguageMap` helper

## Affected Files
- `internal/analyzers/devs/metrics.go` — `DeveloperData`, compute functions
