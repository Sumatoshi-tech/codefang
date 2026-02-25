# FRD: Kubernetes Codebase Performance Benchmarks

**ID:** FRD-20260225-kubernetes-benchmarks
**Date:** 2026-02-25
**Status:** Completed
**Author:** Benchmark Agent

## Overview

Comprehensive performance benchmarking of codefang/uast against competitive
source code analysis tools, measured on the Kubernetes codebase.

## Motivation

Users evaluating code analysis tools want objective, reproducible performance
data. The Kubernetes repository (~28k files, ~5M lines of Go) is an ideal
benchmark target: large, real-world, well-known, and publicly accessible.

## Scope

### In Scope
- Code counting tools (scc, tokei, cloc, gocloc)
- Cyclomatic complexity tools (gocyclo, lizard, codefang)
- AST parsing tools (uast, ast-grep)
- Wall-clock time, peak RSS memory, CPU utilization
- Reproducible benchmark scripts
- Plot generation for visual comparison

### Out of Scope
- Git history analysis (requires deep clone, separate benchmark)
- SonarQube/PMD (require server infrastructure)
- IDE integrations and language server benchmarks

## Deliverables

1. `tools/benchmark/kubernetes_benchmark.py` — Main benchmark runner
2. `tools/benchmark/kubernetes_benchmark_fixup.py` — Fix-up runner for retries
3. `tools/benchmark/kubernetes_benchmark_plots.py` — Plot generator
4. `docs/benchmarks/kubernetes_benchmark_results.json` — Raw JSON results
5. `docs/benchmarks/benchmark_*.png` — 9 performance comparison charts
6. `docs/benchmarks/KUBERNETES_BENCHMARK.md` — Full documentation with analysis

## Acceptance Criteria

- [x] All tools benchmarked successfully (13 benchmark configurations)
- [x] Results include wall time, peak RSS, CPU utilization
- [x] Multiple runs per tool (1 warmup + 3 measured) for statistical validity
- [x] Plots generated with multiple tools on same charts
- [x] Fair methodology documented (same machine, same codebase, warm caches)
- [x] Reproducibility instructions provided
- [x] Trade-off analysis between speed and depth of analysis

## Key Results

| Tool       | Category    | Time (s) | RSS (MB) | CPU % |
|------------|-------------|----------|----------|-------|
| scc        | Counting    | 0.56     | 209      | 333%  |
| tokei      | Counting    | 0.56     | 40       | 332%  |
| gocyclo    | Complexity  | 2.79     | 51       | 123%  |
| codefang   | Complexity  | 39.32    | 870      | 376%  |
| ast-grep   | AST Batch   | 5.38     | 143      | 387%  |
| uast       | AST Batch   | 55.48    | 2,779    | 381%  |

Codefang is 2.1x faster than lizard for complexity analysis while providing
deeper multi-language AST-aware analysis.
