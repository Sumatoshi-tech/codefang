# Couples + Clones Synergy Signal

The `ComputeSynergy` function in `internal/analyzers/clones/synergy.go`
cross-references file coupling data from the couples analyzer with
clone detection results to produce high-confidence refactoring signals.

## Overview

When two files frequently change together (high coupling) AND contain
near-duplicate functions (high MinHash similarity), this is a strong
indicator that shared code should be extracted into a common module.

The synergy function combines these independent signals:
- **Coupling strength** > 0.3 (from couples analyzer co-change matrix)
- **Clone similarity** > 0.8 (from MinHash-based clone detection)

When both conditions are met, a `RefactoringSignal` is emitted.

## API

```go
import (
    "github.com/Sumatoshi-tech/codefang/internal/analyzers/clones"
    "github.com/Sumatoshi-tech/codefang/internal/analyzers/couples"
)

// Get coupling data from couples analysis.
couplingData := couplesMetrics.FileCoupling

// Get clone pairs from clone analysis.
clonePairs := extractedClonePairs

// Compute synergy signals.
signals := clones.ComputeSynergy(couplingData, clonePairs)

for _, signal := range signals {
    fmt.Printf("%s <-> %s (coupling=%.2f, similarity=%.2f)\n",
        signal.FileA, signal.FileB,
        signal.CouplingStrength, signal.CloneSimilarity)
    fmt.Println(signal.Recommendation)
}
```

## RefactoringSignal

```go
type RefactoringSignal struct {
    FileA            string  // First file in the pair
    FileB            string  // Second file in the pair
    CouplingStrength float64 // From couples co-change analysis (0-1)
    CloneSimilarity  float64 // From MinHash clone detection (0-1)
    Recommendation   string  // Human-readable refactoring advice
}
```

## Thresholds

| Signal | Threshold | Rationale |
|---|---|---|
| Coupling strength | > 0.3 | Files change together in >30% of commits |
| Clone similarity | > 0.8 | Functions are near-identical (Type-1 or Type-2 clones) |

Both thresholds use strict greater-than comparison. Values exactly at
the threshold do not produce signals.

## How It Works

1. **Build clone lookup**: Clone pairs are indexed by canonical file
   pair key (alphabetically ordered) with the maximum similarity stored
   for each pair.

2. **Filter coupling data**: Only file pairs with coupling strength
   above the threshold are considered.

3. **Cross-reference**: For each high-coupling pair, check if a
   matching clone pair exists with similarity above the threshold.

4. **Sort signals**: Results are sorted by combined strength
   (`coupling * similarity`) in descending order.

## File Pair Matching

The synergy function uses `clonePairKey` to create order-independent
canonical keys. This means the pair (A, B) matches regardless of
whether the coupling data lists them as (A, B) or (B, A), and
similarly for clone pairs.

When multiple clone pairs exist between the same files (e.g., multiple
functions are duplicated), the maximum similarity is used for the
signal.

## Design Decisions

1. **Pure function**: `ComputeSynergy` is stateless with no side
   effects. It takes structured data and returns signals.

2. **Strict thresholds**: Using `>` (not `>=`) to avoid false signals
   at boundary values.

3. **Located in clones package**: The synergy function lives in the
   clones package because it produces `RefactoringSignal` types that
   are clone-detection concepts. It imports from couples but not vice
   versa, avoiding import cycles.

4. **Map-based lookup**: Clone pairs are indexed by file pair key for
   O(1) lookup, making the overall complexity O(N + M) where N is
   coupling pairs and M is clone pairs.
