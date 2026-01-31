# Codefang: Comprehensive Documentation

> "Codefang is not just a linter; it is a forensic laboratory for software projects."

## Introduction

Codefang is an enterprise-grade, language-agnostic code analysis platform designed to uncover the hidden behavioral and structural patterns in software repositories. While traditional linters focus on syntax and style, Codefang focuses on **evolution** and **complexity**.

It is the spiritual successor to `src-d/hercules`, reimplemented and modernized to support Universal Abstract Syntax Trees (UAST) via Tree-sitter.

## The Philosophy of "Code as Data"

Codefang treats source code not as text, but as a queryable dataset. This involves two distinct dimensions:

1.  **Temporal Dimension (Git History)**: How code changes over time. Who wrote it? How often does it break? Is it dead code?
2.  **Structural Dimension (UAST)**: How code is organized. What are the dependencies? How complex is the control flow? How cohesive are the classes?

By combining these dimensions, Codefang enables questions like: *"Show me the most complex functions written by developers who have left the company."*

## Documentation Structure

This documentation is written for architects, contributors, and curious engineers who want to understand the internal machinery of Codefang.

*   **[Architecture](architecture.md)**: High-level system design, binary separation, and data flow.
*   **[Pipeline Architecture](pipeline-architecture.md)**: Detailed 3-layer pipeline design with caching optimizations for history analysis.
*   **[Core Algorithms](algorithms.md)**: Deep dive into the memory-optimized data structures (Red-Black Trees, Interned Graphs) that allow Codefang to scale.
*   **[Analysis Engine](analyzers.md)**: How the pluggable analyzer system works, with **concrete examples** and case studies.
*   **[Usage Guide & Scenarios](usage.md)**: Real-world stories of using Codefang for refactoring and team management.
*   **[Configuration](configuration.md)**: Full reference for the `.codefang.yaml` file.
*   **[Extensibility](extensibility.md)**: A guide to implementing new analyzers and adding language support.

## Getting Started

If you are just looking to run Codefang, please refer to the [Root README](../README.md) for installation and usage instructions.

## License

Codefang is open-source software. See [LICENSE](../LICENSE) for details.
