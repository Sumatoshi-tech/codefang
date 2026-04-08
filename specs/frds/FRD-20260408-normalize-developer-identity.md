# FRD-20260408: Normalize developer identity in output

## Roadmap Link
- Source roadmap: specs/analytics-readiness/roadmap.md
- Feature: Feature 3 — Normalize developer identity in output

## Problem

Developer identity in JSON output uses pipe-delimited strings (`"daniel smith|dbsmith@google.com"`) from `ReversedPeopleDict`. This blocks clean DWH dimension table creation. Developer IDs are also inconsistently represented: integers in `developers[]` but JSON-serialized string keys in `activity[].by_developer` and `file_contributors[].contributors`.

## Context

The `ReversedPeopleDict` stores identities as `"name1|name2|email1|email2"` (loose mode) or `"name <email>"` (exact mode). The pipe-delimited format is used directly in output via `devName()` and `getDevName()` helper functions. No existing function splits them back.

## Goal

Split pipe-delimited developer identity strings into separate `name` and `email` fields in all output structs.

## In Scope

- Create a shared `SplitIdentity(pipeStr) (name, email)` helper
- Split `DeveloperData.Name` into `Name` + `Email`
- Split `BusFactorData.PrimaryDevName`/`SecondaryDevName` into name + email pairs
- Split `DeveloperCouplingData.Developer1`/`Developer2` into name + email pairs

## Out of Scope

- Changing `activity[].by_developer` from `map[int]int` to array (Feature 5 — flatten nested dicts)
- Changing `file_contributors[].contributors` from `map[int]LineStats` to array (Feature 5)
- Changing the internal `ReversedPeopleDict` format
- Identity deduplication or merging

## Functional Requirements

### MUST
- `SplitIdentity(s string) (name, email string)` in a shared package
  - For pipe-delimited: first element is name, last element containing `@` is email
  - For exact format `"name <email>"`: parse name and email
  - For single element: name = element, email = ""
- `DeveloperData` gains `Email string json:"email,omitempty"`; `Name` becomes plain (no pipe)
- `BusFactorData` gains `PrimaryDevEmail`, `SecondaryDevEmail` fields
- `DeveloperCouplingData` gains `Developer1Email`, `Developer2Email` fields; `Developer1`/`Developer2` become plain names

### SHOULD
- No pipe characters in any developer name/email field in JSON output

## Affected Structs & Compute Functions

| Struct | Field | File:Line | Action |
|--------|-------|-----------|--------|
| `DeveloperData` | `Name` | devs/metrics.go:315 | Split into Name + Email |
| `BusFactorData` | `PrimaryDevName` | devs/metrics.go:344 | Split + add PrimaryDevEmail |
| `BusFactorData` | `SecondaryDevName` | devs/metrics.go:348 | Split + add SecondaryDevEmail |
| `DeveloperCouplingData` | `Developer1` | couples/metrics.go:76 | Split + add Developer1Email |
| `DeveloperCouplingData` | `Developer2` | couples/metrics.go:77 | Split + add Developer2Email |

## Implementation

### Shared helper
- `internal/identity/split.go` — `SplitIdentity(s string) (name, email string)`: handles pipe-delimited, exact `"name <email>"`, and plain name formats
- `internal/identity/split_test.go` — 6 test cases covering all formats

### Struct changes
- `DeveloperData` — added `Email string json:"email,omitempty"`
- `BusFactorData` — added `PrimaryDevEmail`, `SecondaryDevEmail` fields
- `DeveloperCouplingData` — added `Developer1Email`, `Developer2Email` fields

### Logic changes
- `devName()` → `devNameAndEmail()` in devs/metrics.go — returns split name+email via `SplitIdentity`
- `getDevName()` → `getDevNameAndEmail()` in couples/metrics.go — same pattern
- `getOrCreateDev()` — sets both `Name` and `Email` on `DeveloperData`
- `BusFactorMetric.ComputeWithOptions()` — uses tuple assignment for name+email
- `computeDevCouplings()` / `buildCouplingData()` — passes name+email through

### Files modified
- `internal/identity/split.go` (new)
- `internal/identity/split_test.go` (new)
- `internal/analyzers/devs/metrics.go`
- `internal/analyzers/devs/metrics_test.go`
- `internal/analyzers/couples/metrics.go`
