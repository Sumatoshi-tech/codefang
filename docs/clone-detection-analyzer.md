# Clone Detection Analyzer

The `internal/analyzers/clones/` package implements a static code clone
detection analyzer using MinHash signatures and Locality-Sensitive
Hashing. It identifies duplicate and near-duplicate functions in UAST
output, classifying them into three standard clone types.

## Overview

The analyzer walks the UAST, extracts k-gram shingles from function
subtrees, computes MinHash signatures for each function, builds an LSH
index, and retrieves clone pairs in sublinear time.

Clone types:
- **Type-1 (Exact)**: Identical AST structure and tokens (similarity = 1.0)
- **Type-2 (Renamed)**: Identical structure, different variable names (similarity > 0.8)
- **Type-3 (Near-miss)**: Similar but modified structure (similarity > 0.5)

## API

```go
// Create and use the analyzer.
a := clones.NewAnalyzer()
report, err := a.Analyze(uastRoot)

// Format output.
err = a.FormatReport(report, os.Stdout)        // terminal text
err = a.FormatReportJSON(report, os.Stdout)     // JSON
err = a.FormatReportYAML(report, os.Stdout)     // YAML
err = a.FormatReportBinary(report, os.Stdout)   // binary envelope
err = a.FormatReportPlot(report, os.Stdout)     // HTML plot

// Metadata.
a.Name()        // "clones"
a.Flag()        // "clone-detection"
a.Descriptor()  // ID: "static/clones", Mode: ModeStatic

// Thresholds for color-coded reporting.
thresholds := a.Thresholds()
// "clone_ratio":       green=0.0, yellow=0.1, red=0.3
// "total_clone_pairs": green=0, yellow=5, red=20
```

## Architecture

### Shingling

The `Shingler` extracts k-gram shingles (k=5) from function subtrees
by performing a pre-order traversal and collecting consecutive node
type sequences:

```
Function -> Block -> Assignment -> Identifier -> Call
                     ^-- shingle 1 --^
           Block -> Assignment -> Identifier -> Call -> Return
                    ^-- shingle 2 --^
```

Each shingle is a byte slice of pipe-separated node types:
`Function|Block|Assignment|Identifier|Call`.

### MinHash Signatures

Each function's shingle set is compressed into a 128-hash MinHash
signature. This fixed-size fingerprint enables Jaccard similarity
estimation in O(1) time per pair.

### LSH Index

Signatures are indexed with 16 bands of 8 rows. This creates a
natural similarity threshold around 0.5-0.8, ensuring Type-2 and
Type-3 clones are efficiently retrieved without O(n^2) comparison.

### Visitor Pattern

The analyzer implements `VisitorProvider` for integration with the
`MultiAnalyzerTraverser`. The visitor collects function nodes during
a single-pass traversal, then performs clone detection in `GetReport()`:

```go
v := clones.NewVisitor()
// During traversal:
v.OnEnter(node, depth)  // collects function nodes
// After traversal:
report := v.GetReport() // runs detection, returns report
```

## Report Structure

The report map contains:

| Key | Type | Description |
|---|---|---|
| `analyzer_name` | `string` | `"clones"` |
| `total_functions` | `int` | Number of functions analyzed |
| `total_clone_pairs` | `int` | Number of detected clone pairs |
| `clone_ratio` | `float64` | `total_clone_pairs / total_functions` |
| `clone_pairs` | `[]map[string]any` | List of clone pairs with `func_a`, `func_b`, `similarity`, `clone_type` |
| `message` | `string` | Human-readable summary |

## Report Section

The `ReportSection` provides:
- **Score**: `1.0 - clone_ratio` (lower duplication = higher score)
- **Key Metrics**: Total functions, clone pairs, clone ratio
- **Distribution**: Type-1/Type-2/Type-3 breakdown with percentages
- **Issues**: Clone pairs sorted by severity (similarity > 0.8 = Poor)

## Aggregation

The `Aggregator` merges results across files using `common.Aggregator`:
- Numeric averages: `clone_ratio`
- Counts summed: `total_functions`, `total_clone_pairs`
- Collections merged: `clone_pairs`

## Plot Output

`FormatReportPlot` generates an HTML page with a pie chart showing
clone type distribution using go-echarts. Register with:

```go
clones.RegisterPlotSections()
```

## Performance

Benchmarks on AMD Ryzen AI 9 HX 370 (k=5, 128 hashes, 16 bands x 8 rows):

| Benchmark | ns/op | B/op | allocs/op |
|---|---|---|---|
| `CloneDetection_100Functions` | ~2.7M (~2.7 ms) | 1.6M | 20K |
| `Shingling` (single function) | ~2,635 (~2.6 us) | 4,152 | 85 |
| `Visitor_100Functions` | ~2.7M (~2.7 ms) | 1.6M | 20K |

## Design Decisions

1. **k=5 shingle size**: Balances sensitivity and specificity. Smaller
   k values produce too many false positives; larger k values miss
   near-miss clones.

2. **128 MinHash hashes**: Standard choice for code similarity. Provides
   ~8% standard error on Jaccard estimation.

3. **16 bands x 8 rows**: Creates threshold around similarity 0.5,
   ensuring Type-3 clones are captured while minimizing false candidates.

4. **Pre-order traversal for shingling**: Captures hierarchical structure
   of AST subtrees. Post-order or level-order would lose parent-child
   relationships.

5. **Function-level granularity**: Functions and methods are the natural
   unit for clone detection. Block-level would generate too many pairs;
   file-level would miss intra-file clones.

6. **Shared `findClonePairs` function**: Both the standalone `Analyze`
   path and the visitor path share the same LSH query and pair matching
   logic to avoid code duplication.
