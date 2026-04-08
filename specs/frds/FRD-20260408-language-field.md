# FRD-20260408: Add language field to function records

## Roadmap Link
- Source roadmap: specs/analytics-readiness/roadmap.md
- Feature: Feature 8

## Problem
Function-level records have no language field. Analysts must infer from file extension.

## Goal
Add `language` field to all function-level output structs, populated from parser.GetLanguage.

## Approach
- Add `LanguageKey = "_language"` constant in analyze package
- Stamp language in `analyzeFilesParallel` alongside StampSourceFile, using `parser.GetLanguage(filePath)`
- Add `Language` field to `FunctionData` (input) in complexity, halstead, cohesion, comments
- Parse `_language` in each `parseFunctionData`
- Add `Language string json:"language,omitempty"` to all output data structs
- Propagate in each `Compute()`
