# Kubernetes Codebase Performance Benchmark

A comprehensive, reproducible performance comparison of codefang/uast against
competitive source code analysis tools, measured on the **Kubernetes** codebase
(one of the largest open-source Go projects).

## Target Repository

| Metric         | Value          |
|----------------|----------------|
| Repository     | kubernetes/kubernetes |
| Total files    | 28,275         |
| Go files       | 16,620         |
| Go lines       | 4,939,520      |
| Disk size      | ~383 MB        |

## Test Environment

| Component       | Value                          |
|-----------------|--------------------------------|
| CPU             | Intel Xeon (4 cores, 2.4 GHz)  |
| Memory          | 15.6 GB                        |
| OS              | Linux 6.1.147 x86_64           |
| Go              | 1.26.0                         |
| Python          | 3.12.3                         |
| Methodology     | 1 warmup + 3 measured runs     |
| Instrumentation | GNU time (`/usr/bin/time -v`)   |

Measurements: wall-clock time, peak resident set size (RSS), CPU utilization %.

All tool outputs were redirected to `/dev/null` so I/O write costs are excluded.
Filesystem caches were warm after the warmup run.

---

## Tools Compared

| Tool      | Version   | Language | Focus                             |
|-----------|-----------|----------|-----------------------------------|
| **codefang** | dev (9319ff5) | Go+C | Cyclomatic + cognitive complexity via UAST |
| **uast**     | dev (9319ff5) | Go+C | Full AST parsing + UAST transformation |
| **scc**      | 3.6.0     | Go       | Lines of code + complexity counting |
| **tokei**    | 12.1.2    | Rust     | Lines of code counting             |
| **cloc**     | 1.98      | Perl     | Lines of code counting (classic)   |
| **gocloc**   | latest    | Go       | Lines of code counting             |
| **lizard**   | 1.21.0    | Python   | Cyclomatic complexity analysis     |
| **gocyclo**  | latest    | Go       | Cyclomatic complexity (Go only)    |
| **ast-grep** | 0.41.0    | Rust     | AST pattern search via tree-sitter |

---

## Results Summary

### Overall Performance Table

| Tool       | Category             | Avg Time (s) | Peak RSS (MB) | CPU % | Notes |
|------------|----------------------|--------------|---------------|-------|-------|
| **tokei**  | Code Counting        | **0.56**     | **40**        | 332%  | Fastest counter, lowest memory |
| **scc**    | Code Counting        | **0.56**     | 209           | 333%  | Tied fastest, adds complexity metrics |
| **gocloc** | Code Counting        | 1.58         | 47            | 121%  | 3x slower, low memory |
| **cloc**   | Code Counting        | 64.18        | 116           | 99%   | 115x slower (single-threaded Perl) |
| **gocyclo**| Complexity           | **2.79**     | **51**        | 123%  | Fastest Go-only complexity |
| **codefang**| Complexity          | 39.32        | 870           | 376%  | Multi-lang UAST-based, deepest analysis |
| **lizard** | Complexity           | 83.89        | 260           | 99%   | Single-threaded Python |
| **scc**    | Complexity (detail)  | **2.58**     | 288           | 150%  | Line-level counting complexity |
| **codefang**| Complexity (detail) | 38.87        | 913           | 375%  | Full AST-aware function-level metrics |
| **ast-grep**| AST Parse (single)  | **0.18**     | **37**        | 101%  | Pattern match only, 30k-line file |
| **uast**   | AST Parse (single)   | 0.99         | 389           | 131%  | Full UAST transformation, 30k-line file |
| **ast-grep**| AST Parse (batch)   | **5.38**     | **143**       | 387%  | Pattern search across 16k+ files |
| **uast**   | AST Parse (batch)    | 55.48        | 2,779         | 381%  | Full UAST transform, 16k+ Go files |

---

## Category 1: Code Counting & Metrics

Counting lines of code, comments, and blank lines across the entire Kubernetes
codebase (~28k files, ~5M lines of Go).

![Code Counting Benchmark](benchmark_code_counting.png)

| Tool     | Time (s) | Peak RSS (MB) | CPU % | Speedup vs cloc |
|----------|----------|---------------|-------|-----------------|
| scc      | 0.56     | 209           | 333%  | **115x**        |
| tokei    | 0.56     | 40            | 332%  | **115x**        |
| gocloc   | 1.58     | 47            | 121%  | **41x**         |
| cloc     | 64.18    | 116           | 99%   | 1x (baseline)   |

**Key findings:**
- **scc** and **tokei** are tied at ~0.56s, both leveraging multi-core parallelism (330%+ CPU).
- **tokei** achieves this with only **40 MB** of RAM (5x less than scc).
- **cloc** is 115x slower due to single-threaded Perl execution.
- **gocloc** is a solid middle ground: 3x slower than scc/tokei but very lean on memory.

---

## Category 2: Cyclomatic Complexity Analysis

Measuring function-level cyclomatic complexity across all Go files.

![Complexity Benchmark](benchmark_complexity.png)

| Tool      | Time (s) | Peak RSS (MB) | CPU % | Depth of Analysis |
|-----------|----------|---------------|-------|-------------------|
| gocyclo   | 2.79     | 51            | 123%  | Go-only cyclomatic |
| codefang  | 39.32    | 870           | 376%  | Multi-lang cyclomatic + cognitive via UAST |
| lizard    | 83.89    | 260           | 99%   | Multi-lang cyclomatic |

**Key findings:**
- **gocyclo** is the fastest for Go-only cyclomatic complexity (2.8s) but provides
  no cross-language support and no cognitive complexity metrics.
- **codefang** is 14x slower than gocyclo but provides:
  - Multi-language support (60+ languages via UAST)
  - Both cyclomatic AND cognitive complexity
  - Function-level AST-aware analysis (not regex-based)
  - Full utilization of all CPU cores (376%)
- **codefang** is **2.1x faster** than lizard while providing deeper analysis.
- **lizard** is bottlenecked by single-threaded Python (99% CPU, single core).

### Complexity Detail: scc vs codefang

![Complexity Detail](benchmark_complexity_detail.png)

| Tool      | Time (s) | Peak RSS (MB) | Analysis Depth |
|-----------|----------|---------------|----------------|
| scc       | 2.58     | 288           | Line-based complexity estimate |
| codefang  | 38.87    | 913           | AST-aware function-level metrics |

scc's "complexity" is a line-based heuristic (counting branches/keywords), while
codefang performs full AST parsing through UAST and computes precise cyclomatic +
cognitive complexity per function. The ~15x time difference reflects the depth
difference: line scanning vs. full semantic analysis.

---

## Category 3: AST Parsing

### Single File (30,478 lines)

Parsing a single large Go file (`validation_test.go`).

![AST Parse Single](benchmark_ast_parse_single.png)

| Tool      | Time (s) | Peak RSS (MB) | Operation |
|-----------|----------|---------------|-----------|
| ast-grep  | 0.18     | 37            | Pattern match (tree-sitter parse + search) |
| uast      | 0.99     | 389           | Full UAST transformation (tree-sitter → UAST) |

**Key finding:** uast performs a full semantic transformation from raw tree-sitter
AST to Universal AST with DSL-based mappings, which is ~5x slower but produces a
language-independent representation that codefang analyzers consume.

### Batch (16,620 Go files)

Processing all Go files in the Kubernetes repository.

![AST Parse Batch](benchmark_ast_parse_batch.png)

| Tool      | Time (s) | Peak RSS (MB) | CPU % | Operation |
|-----------|----------|---------------|-------|-----------|
| ast-grep  | 5.38     | 143           | 387%  | Pattern search across all files |
| uast      | 55.48    | 2,779         | 381%  | Full UAST transformation |

**Key findings:**
- ast-grep is ~10x faster because it only performs pattern matching on raw
  tree-sitter AST nodes, while uast does full semantic transformation.
- uast's higher memory usage reflects loading the DSL mapping engine and
  producing rich UAST output for each file.
- Both tools utilize all CPU cores effectively (380%+ on 4 cores).

---

## Overall Comparison

### Wall-Clock Time

![Overall Time](benchmark_overall_time.png)

### Memory Consumption

![Overall Memory](benchmark_overall_memory.png)

### Time vs Memory Trade-off

![Time vs Memory](benchmark_time_vs_memory.png)

### Dashboard

![Dashboard](benchmark_dashboard.png)

---

## Analysis & Conclusions

### Where codefang/uast excel

1. **Depth of analysis**: codefang provides AST-aware, function-level cyclomatic AND
   cognitive complexity across 60+ languages. No other tested tool matches this.

2. **Multi-core utilization**: codefang/uast use ~375% CPU on 4 cores, while
   single-threaded competitors (cloc, lizard) are stuck at 99%.

3. **vs lizard**: codefang is **2.1x faster** while providing deeper analysis
   (cognitive complexity, UAST-based, multi-language). Lizard's Python runtime is
   the bottleneck.

4. **Unified pipeline**: `uast parse | codefang analyze` provides a complete code
   intelligence pipeline that no single competitor offers.

### Where competitors excel

1. **scc/tokei**: For simple line counting, these tools are unbeatable at ~0.5s.
   They don't parse ASTs, which is both their strength (speed) and limitation (no
   semantic understanding).

2. **gocyclo**: For Go-only cyclomatic complexity, gocyclo is ~14x faster than
   codefang. The trade-off: no multi-language support, no cognitive complexity,
   no UAST transformation.

3. **ast-grep**: For AST pattern matching, ast-grep is ~10x faster than uast batch
   parsing. ast-grep works directly on tree-sitter ASTs without UAST transformation.

### The trade-off

| Dimension          | Fast tools (scc, tokei, gocyclo) | Deep tools (codefang, uast) |
|--------------------|----------------------------------|-----------------------------|
| Speed              | Sub-second to seconds            | 30-60 seconds               |
| Memory             | 40-290 MB                        | 870-2800 MB                 |
| Languages          | Limited or single                | 60+ via UAST                |
| Analysis depth     | Line/regex heuristics            | Full AST semantic analysis  |
| Cognitive metrics  | No                               | Yes                         |
| History analysis   | No                               | Yes (burndown, couples, etc)|

---

## Reproducibility

### Prerequisites

```bash
# Install competitive tools
go install github.com/boyter/scc/v3@latest
go install github.com/hhatto/gocloc/cmd/gocloc@latest
go install github.com/fzipp/gocyclo/cmd/gocyclo@latest
pip install lizard ast-grep-cli matplotlib
sudo apt-get install -y cloc time
# tokei: download binary from https://github.com/XAMPPRocky/tokei/releases

# Clone Kubernetes
git clone --depth 1 https://github.com/kubernetes/kubernetes.git /tmp/benchmark-repos/kubernetes

# Build codefang/uast
cd /path/to/codefang
make build
```

### Running Benchmarks

```bash
# Full benchmark suite
python3 tools/benchmark/kubernetes_benchmark.py

# Fix-up for any failures
python3 tools/benchmark/kubernetes_benchmark_fixup.py

# Generate plots
python3 tools/benchmark/kubernetes_benchmark_plots.py
```

### Output

- `docs/benchmarks/kubernetes_benchmark_results.json` — Raw results (JSON)
- `docs/benchmarks/benchmark_*.png` — Performance comparison plots
- `docs/benchmarks/KUBERNETES_BENCHMARK.md` — This document

---

## Methodology Notes

1. **Warmup**: Each tool runs once before measurement to warm filesystem caches.
2. **Measurements**: 3 runs per tool; results show average, min, max, and stddev.
3. **Isolation**: All output redirected to `/dev/null` to remove I/O write variance.
4. **Fairness**: Same machine, same codebase, same filesystem state for all tools.
5. **Instrumentation**: GNU `time` (`/usr/bin/time -v`) captures peak RSS and CPU%.
6. **Comparison caveat**: Tools perform different depths of analysis. The table above
   captures what each tool does. Comparing raw speed between a line counter and an
   AST analyzer is apples-to-oranges — the charts show the trade-off, not a ranking.

---

*Generated: 2026-02-25 | System: Intel Xeon 4-core, 15.6 GB RAM, Linux 6.1.147*
