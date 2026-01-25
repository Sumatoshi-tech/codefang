# Shotness (Structural Hotness) Analysis

## Preface
Knowing *that* a file changed is good. Knowing *what part* of the file changed is better.

## Problem
File-level statistics are too coarse. A "utils.go" file might be huge and change constantly, but are those changes in the same function or scattered everywhere? We need fine-grained resolution.

## How analyzer solves it
"Shotness" (Structural Hotness) tracks changes to specific structural elements—like functions or classes—defined by a User DSL. It tells you which functions are modified most frequently.

## Historical context
This is an evolution of "hotspot" analysis, moving from file-granularity to logical-unit-granularity.

## Real world examples
- **Testing Strategy:** If `ProcessPayment()` changes in 50% of commits, it needs extremely robust tests.
- **Volatility Analysis:** Identifying unstable functions that might need refactoring to adhere to the Open/Closed Principle.

## How analyzer works here
1.  **Configuration:** User defines a DSL query (e.g., `filter(.roles has "Function")`) to select nodes of interest.
2.  **Node Tracking:** As files change, the analyzer tracks these specific named nodes.
3.  **Renames:** It handles function renames (if supported by UAST diffing) to maintain history.
4.  **Co-occurrence:** It also tracks which functions change together (Structural Coupling).

## Limitations
- **Performance:** Fine-grained UAST diffing is more expensive than file-level diffing.
- **DSL Complexity:** Requires understanding the UAST structure to write effective queries.

## Further plans
- Pre-defined queries for common languages.
