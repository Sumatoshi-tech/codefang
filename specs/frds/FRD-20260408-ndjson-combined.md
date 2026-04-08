# FRD-20260408: NDJSON output for combined mode

## Roadmap Link
- Source roadmap: specs/analytics-readiness/roadmap.md
- Feature: Feature 10

## Problem
The monolithic JSON output must be fully parsed to extract any single analyzer. NDJSON enables streaming ingestion.

## Goal
Add `FormatNDJSON` support to `WriteConvertedOutput` — one JSON line per analyzer result.

## Approach
Add a `case FormatNDJSON` to `WriteConvertedOutput` that iterates `model.Analyzers` and writes each as a compact JSON line. Optionally prepend a metadata line if metadata is present.
