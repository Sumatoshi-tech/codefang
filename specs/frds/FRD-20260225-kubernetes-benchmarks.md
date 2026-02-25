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
- Git history analysis (hercules v10.7.2 vs codefang: burndown, couples, devs)
- Wall-clock time, peak RSS memory, CPU utilization
- Reproducible benchmark scripts
- Plot generation for visual comparison

### Out of Scope
- SonarQube/PMD (require server infrastructure)
- IDE integrations and language server benchmarks

## Deliverables

1. `tools/benchmark/kubernetes_benchmark.py` — Static analysis benchmark runner
2. `tools/benchmark/kubernetes_benchmark_plots.py` — Static analysis plot generator
3. `tools/benchmark/kubernetes_benchmark_fixup.py` — Fix-up runner for retries
4. `tools/benchmark/kubernetes_hercules_benchmark.py` — Hercules vs codefang runner
5. `tools/benchmark/kubernetes_hercules_plots.py` — Hercules plot generator
6. `docs/benchmarks/kubernetes_benchmark_results.json` — Static analysis results
7. `docs/benchmarks/kubernetes_hercules_benchmark_results.json` — History results
8. `docs/benchmarks/benchmark_*.png` — 9 static analysis charts
9. `docs/benchmarks/hercules_*.png` — 5 hercules comparison charts
10. `docs/benchmarks/KUBERNETES_BENCHMARK.md` — Full documentation with analysis

## Acceptance Criteria

- [x] All static tools benchmarked (13 configurations)
- [x] Hercules vs codefang benchmarked (12 configurations: 3 analyzers x 2 scales x 2 tools)
- [x] Results include wall time, peak RSS, CPU utilization
- [x] Multiple runs per tool (1 warmup + 3 measured) for statistical validity
- [x] Plots generated with multiple tools on same charts
- [x] Fair methodology documented (same machine, same codebase, warm caches)
- [x] Reproducibility instructions provided
- [x] Trade-off analysis between speed and depth of analysis

## Key Results

### Static Analysis

| Tool       | Category    | Time (s) | RSS (MB) | CPU % |
|------------|-------------|----------|----------|-------|
| scc        | Counting    | 0.56     | 209      | 333%  |
| tokei      | Counting    | 0.56     | 40       | 332%  |
| gocyclo    | Complexity  | 2.79     | 51       | 123%  |
| codefang   | Complexity  | 39.32    | 870      | 376%  |
| ast-grep   | AST Batch   | 5.38     | 143      | 387%  |
| uast       | AST Batch   | 55.48    | 2,779    | 381%  |

### History Analysis (1000 commits)

| Tool      | Analyzer | Time (s) | RSS (MB) | Speedup |
|-----------|----------|----------|----------|---------|
| codefang  | burndown | 1.47     | 323      | 28.7x   |
| hercules  | burndown | 42.16    | 1,576    | —       |
| codefang  | couples  | 3.77     | 1,942    | 19.1x   |
| hercules  | couples  | 72.11    | 1,578    | —       |
| codefang  | devs     | 1.53     | 326      | 28.5x   |
| hercules  | devs     | 43.68    | 1,577    | —       |

**Headline**: Codefang is **19-29x faster** than Hercules on the same analyzers,
with **4-5x less memory** for burndown/devs.
