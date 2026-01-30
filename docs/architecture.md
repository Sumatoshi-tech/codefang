# System Architecture

Codefang is designed as a modular, high-performance analysis pipeline. Unlike monolithic tools, it separates the concerns of **parsing** and **analysis** into distinct operational phases, often running as separate processes.

## The Binary Split

The project is architecturally divided into two primary binaries:

1.  **`uast`**: The Parsing Engine.
    *   **Responsibility**: Converts raw source code (Go, Python, Java, etc.) into a standardized Universal Abstract Syntax Tree (UAST).
    *   **Technology**: Wraps `tree-sitter` bindings for 60+ languages.
    *   **Output**: Language-agnostic Protobuf or JSON representation of the code structure.

2.  **`codefang`**: The Analysis Engine.
    *   **Responsibility**: Consumes UASTs or Git history to calculate metrics.
    *   **Technology**: Pure Go, utilizing specialized data structures.
    *   **Input**: UAST data (piped from `uast`) or `.git` directory.

### Why this separation?

*   **Isolation**: Tree-sitter bindings often require CGO. Keeping them in `uast` keeps the main `codefang` binary pure Go (mostly) and stable.
*   **Scalability**: Parsing is CPU intensive. You can scale `uast` workers independently of analysis workers.
*   **Unix Philosophy**: The tools are composable. You can use `uast` to dump ASTs for other tools without invoking Codefang's analysis.

## High-Level Data Flow

The following diagram illustrates how data flows from the source repository through the parsing engine to the analyzers and finally to the consumers (CLI or AI Agents).

![System Architecture](diagrams/architecture.puml)

*Note: The diagram above is defined in PlantUML. Render it to see the visual flow.*

### 1. History Mining Pipeline

When running in history mode (`codefang history`), the system:
1.  Opens the Git repository using `go-git`.
2.  Traverses the commit graph (DAG).
3.  Feeds commit data (diffs, timestamps, authors) into **Behavioral Analyzers** (Burndown, Couples).
4.  Analyzers use **Red-Black Trees** to efficiently index data over time windows.

### 2. Static Analysis Pipeline

When running in analysis mode (`codefang analyze`):
1.  Source files are identified.
2.  `uast` parses files into AST nodes.
3.  The **Factory** dispatches these nodes to registered **Static Analyzers**.
4.  **Visitors** traverse the AST *once*, notifying multiple analyzers (Complexity, Halstead) simultaneously to save CPU cycles.
5.  Results are aggregated and formatted (JSON/Table).

## Component Interaction

The interaction between components is defined by strict interfaces in the `pkg/` directory:

*   **`pkg/uast`**: Defines the `Node` structure that serves as the common language between the parser and analyzers.
*   **`pkg/analyzers`**: Defines the `Analyzer` interface. Any struct satisfying this interface can be plugged into the pipeline.
*   **`pkg/report`**: Standardizes the output format, ensuring that whether you are checking for typos or calculating cyclomatic complexity, the result structure is consistent.
