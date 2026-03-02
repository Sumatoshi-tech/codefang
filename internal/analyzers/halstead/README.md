# Halstead Complexity Analysis

## Preface
Halstead metrics attempt to measure the "volume" and "difficulty" of a program based on the number of operators and operands, treating code like a piece of literature.

## Problem
How do we compare the complexity of two different programs? Simple LOC (Lines of Code) is misleading. We need a metric that accounts for the density of logic.

## How analyzer solves it
The Halstead analyzer computes a suite of metrics:
- **Program Length (N):** Total number of operators + operands.
- **Vocabulary (n):** Unique operators + unique operands.
- **Volume (V):** Information content of the program.
- **Difficulty (D):** How hard it is to write or understand.
- **Effort (E):** Mental effort required.

## Historical context
Proposed by Maurice Halstead in 1977 in his book "Elements of Software Science". It was one of the first attempts to apply scientific measurement to software.

## Real world examples
- **Algorithmic Complexity:** Comparing different implementations of the same algorithm.
- **Code Quality Baselines:** Establishing a baseline for "acceptable" module volume.

## How analyzer works here
1.  **UAST Traversal:** Scans the Abstract Syntax Tree.
2.  **Token Classification:** Identifies tokens as either **Operators** (arithmetic, logical, assignments, function calls) or **Operands** (variables, constants, strings).
3.  **Calculation:** Applies Halstead's formulas to the counts.

## Limitations
- **Modern Relevance:** Developed for Algol/Fortran. Some argue it's less relevant for modern, high-level, expressive languages, but it still provides a useful relative comparison.

## Further plans
- Calibrating coefficients for modern languages.
