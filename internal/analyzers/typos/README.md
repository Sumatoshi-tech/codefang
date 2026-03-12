# Typos Dataset Builder

## Preface
Clean code is professional code. But more than that, typos in identifiers can be bugs waiting to happen (e.g., overriding a method but misspelling the name).

## Problem
- Finding common spelling mistakes in the codebase.
- Generating datasets for training Machine Learning models to automatically fix typos (the original intent of this analyzer).

## How analyzer solves it
The Typos analyzer looks for "typo-fix" patterns in the commit history. It identifies cases where an identifier was changed to another identifier with a very small Levenshtein distance (e.g., `recieve` -> `receive`).

## Historical context
This was likely developed to support "Natural Code" research—building tools that autocorrect code like a spellchecker.

## Real world examples
- **Dataset Generation:** Creating a list of 10,000 real-world typo fixes to train a neural network.
- **QA:** Scanning recent commits to catch typos that slipped through review.

## How analyzer works here
1.  **Diff Scan:** Looks at modified lines in commits.
2.  **Identifier Extraction:** Uses UAST/Tokenization to find identifiers in the "before" and "after" versions.
3.  **Distance Calculation:** Computes Levenshtein distance.
4.  **Filtering:** If the distance is small (e.g., 1 or 2 edits) and the context is similar, it records it as a typo fix.

## Limitations
- **False Positives:** `color` -> `colour` might be a localization change, not a typo. `i` -> `j` in a loop is logic, not a typo.

## Further plans
- Context-aware validation.
