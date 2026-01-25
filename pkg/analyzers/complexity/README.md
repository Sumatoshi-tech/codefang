# Complexity Analysis

## Preface
Complexity is the enemy of reliability. The more complex the code, the harder it is to test and the more likely it is to contain bugs.

## Problem
Developers often write clever, complex code that is hard to read. Nested loops, deep conditional chains, and massive functions make the codebase fragile.

## How analyzer solves it
This analyzer computes standard complexity metrics to identify "hotspots" of complexity in the code.
- **Cyclomatic Complexity:** Measures the number of linearly independent paths through a program's source code.
- **Cognitive Complexity:** Measures how difficult a unit of code is to intuitively understand.
- **Nesting Depth:** How deeply nested control structures are.

## Historical context
- **Cyclomatic Complexity** was developed by Thomas McCabe in 1976.
- **Cognitive Complexity** was introduced by SonarSource in 2017 to provide a more human-centric measure of readability.

## Real world examples
- **Refactoring Prioritization:** Target the top 10% most complex functions for simplification.
- **Risk Assessment:** Complex modules are statistically more likely to have defects.

## How analyzer works here
1.  **UAST Traversal:** Walks the Abstract Syntax Tree.
2.  **Pattern Matching:**
    - Counts decision points (`if`, `for`, `while`, `case`, `catch`) for Cyclomatic Complexity.
    - Evaluates nesting levels and "breaks" in linear flow for Cognitive Complexity.
    - Tracks nesting depth.
3.  **Reporting:** Aggregates metrics per function and file.

## Limitations
- **Language Nuances:** Some language-specific constructs (like list comprehensions in Python) might be underestimated if the UAST mapping isn't perfect.

## Further plans
- Trend analysis: visualizing complexity growth over time.
