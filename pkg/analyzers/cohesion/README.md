# Cohesion Analysis

## Preface
High cohesion and low coupling are the holy grails of software design. Cohesion measures how well the elements inside a module (like a class or function) belong together.

## Problem
"God classes" or "God objects" are classes that do too much. They are hard to understand, hard to test, and hard to change. Low cohesion usually indicates that a class should be split into smaller, more focused classes.

## How analyzer solves it
The Cohesion analyzer calculates the **LCOM (Lack of Cohesion of Methods)** metric (specifically LCOM4 variant) and other cohesion scores. It analyzes how methods in a class interact with the class's fields. If methods operate on disjoint sets of fields, the class likely lacks cohesion.

## Historical context
LCOM metrics were introduced by Chidamber and Kemerer in 1994 as part of their object-oriented metrics suite. It has since been a standard metric in static analysis tools.

## Real world examples
- **Refactoring Candidates:** A high LCOM score flags classes that are likely doing too many unrelated things and should be refactored.
- **Architecture Review:** Ensuring that core domain objects remain focused and cohesive.

## How analyzer works here
1.  **UAST Traversal:** It uses Universal Abstract Syntax Trees (UAST) to parse code in a language-agnostic way.
2.  **Function/Class Discovery:** It identifies functions, methods, and classes.
3.  **Variable Usage:** For each method, it tracks which class variables (fields) are used.
4.  **Graph Construction:** It effectively builds a graph where methods are nodes and shared variable usage creates edges.
5.  **Metrics:**
    - **LCOM4:** Number of connected components in the graph.
    - **Cohesion Score:** A normalized score based on the density of connections.

## Limitations
- **Static Analysis:** It only looks at static usage. It cannot track dynamic field access (e.g., reflection).
- **Language Support:** Relies on the quality of the UAST extraction for the specific language.

## Further plans
- Support for more granular cohesion metrics.
- Visualization of the method-field usage graph.
