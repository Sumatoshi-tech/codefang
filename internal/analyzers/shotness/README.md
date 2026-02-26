# Shotness (Structural Hotness) Analysis

## Preface
Knowing *that* a file changed is good. Knowing *what part* of the file changed is better.

## Problem
File-level statistics are too coarse. A "utils.go" file might be huge and change constantly, but are those changes in the same function or scattered everywhere? We need fine-grained resolution.

## How analyzer solves it
"Shotness" (Structural Hotness) tracks changes to specific structural elements—like functions or classes—defined by a User DSL. It tells you which functions are modified most frequently and which ones change together.

## Historical context
This is an evolution of "hotspot" analysis (Adam Tornhill, *Your Code as a Crime Scene*), moving from file-granularity to logical-unit-granularity. The concept originated in Hercules (src-d/hercules) and has been refined here with normalized coupling strength metrics.

## Real world examples
- **Testing Strategy:** If `ProcessPayment()` changes in 50% of commits, it needs extremely robust tests.
- **Volatility Analysis:** Identifying unstable functions that might need refactoring to adhere to the Open/Closed Principle.
- **Team Assessment:** Functions with high coupling strength (> 0.8) are candidates for extraction into shared modules.
- **Risk Prioritization:** HIGH risk nodes (≥ 20 changes) should be reviewed for design flaws, not just bugs.

## How analyzer works here
1.  **Configuration:** User defines a DSL query (e.g., `filter(.roles has "Function")`) to select nodes of interest.
2.  **Node Tracking:** As files change, the analyzer tracks these specific named nodes via diff hunk mapping.
3.  **Renames:** It handles function renames (if supported by UAST diffing) to maintain history.
4.  **Co-occurrence:** It also tracks which functions change together (Structural Coupling).
5.  **Normalization:** Coupling strength is normalized to [0, 1] using the formula: `co_changes / max(co_changes, changes_a, changes_b)`.

## Output Formats
- **JSON/YAML:** Structured metrics with `node_hotness`, `node_coupling`, `hotspot_nodes`, and `aggregate` sections.
- **Text:** Terminal-friendly output with colored progress bars, risk classification, and coupling arrows.
- **Plot:** Interactive HTML dashboard with TreeMap, HeatMap, and Bar Chart visualizations.

## Metrics
- **Hotness Score:** Normalized [0, 1] relative to the most changed function.
- **Coupling Strength:** Normalized [0, 1] confidence metric for co-change pairs.
- **Risk Level:** HIGH (≥ 20), MEDIUM (≥ 10), LOW (< 10) change count thresholds.
- **Aggregate:** Summary statistics including average coupling strength across all pairs.

## Limitations
- **Performance:** Fine-grained UAST diffing is more expensive than file-level diffing.
- **DSL Complexity:** Requires understanding the UAST structure to write effective queries.
- **Large Functions:** Any change within a function's line range counts as a change to that function.

## Further plans
- Pre-defined queries for common languages.
- Temporal decay: weight recent changes higher than old ones.
- Cross-repository coupling analysis.
