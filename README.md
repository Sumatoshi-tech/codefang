# Codefang

<p align="center">
  <img src="assets/uast.png" alt="Codefang Logo" width="500">
</p>

[![CI](https://github.com/Sumatoshi-tech/codefang/actions/workflows/ci.yml/badge.svg)](https://github.com/Sumatoshi-tech/codefang/actions/workflows/ci.yml)
[![Go Reference](https://pkg.go.dev/badge/github.com/Sumatoshi-tech/codefang.svg)](https://pkg.go.dev/github.com/Sumatoshi-tech/codefang)

**The heavy lifter for your codebase.**

Codefang is a comprehensive code analysis platform that understands your code deeply—not just as text, but as structure (AST) and history (Git). Whether you're tracking technical debt, analyzing developer churn, or feeding context to an AI, Codefang does the heavy lifting so you don't have to.

---

## 1. Preface

So, you've stumbled upon Codefang. You might be thinking, *"Wait, didn't `src-d` build something like this years ago?"*

Yes, they did. And it was glorious. But like many great empires, `src-d` faded into the annals of history, and the original `hercules` was left gathering dust in the digital attic.

**This is not that project.**

This is a **reincarnation**. We took the core philosophy, stripped out the obsolete parts, and rebuilt it with a modern engine. It's not a drop-in replacement; it's a spiritual successor with a gym membership and a PhD in Abstract Syntax Trees.

---

## 2. Historical Context

Once upon a time, there was a company called **source{d}**. They pioneered "Mining Software Repositories" on a massive scale. Their crown jewel was `hercules`, a tool that could chew through git logs faster than you can say `git blame`.

It gave us insights like:
- **Burndown Charts**: How much code from 2015 is still surviving today?
- **Coupling**: If I change `A.go`, does `B.go` always change with it?

When `src-d` ceased operations, the tool became abandonware. But the need to understand code didn't go away.

We revived the project with a new mission: **Combine history with structure.**
The old `hercules` was great at Git history. Codefang adds **UAST (Universal Abstract Syntax Tree)** powers, allowing it to understand the *meaning* of the code across 60+ languages, not just the diffs.

---

## 3. Quickstart

Enough history class. Let's see some muscles.

### Installation

You'll need Go installed. Then, just grab the binaries:

```sh
# The brain: Code analysis and history mining
go install github.com/Sumatoshi-tech/codefang/cmd/codefang@latest

# The eyes: Universal parser (supports 60+ languages)
go install github.com/Sumatoshi-tech/codefang/cmd/uast@latest
```

### Let's Flex

Codefang follows the UNIX philosophy: small tools, joined by pipes.

**1. Analyze complexity (Static Analysis):**
Parse your code and pipe it into the analyzer.

```sh
# How messy is my main.go?
uast parse main.go | codefang analyze -a complexity

# ...or my entire codebase?
uast parse **/*.go | codefang analyze -a complexity
```

**2. Analyze history (Git Forensics):**
See who knows what, and how code is aging.

```sh
# Generate a burndown chart (lines surviving over time)
codefang history -a burndown .
```

**3. Find the experts:**
Who actually wrote the code that's running in production?

```sh
codefang history -a devs --head .
```

---

## 4. Architecture & Design Decisions

We made a few bold choices in the rewrite.

### The Split: `uast` vs `codefang`

The original `hercules` was a monolith. We split it in two:

1.  **`uast` (The Parser)**: This tool focuses on one thing—turning source code into a standardized tree structure. It uses **Tree-sitter** under the hood to support practically every language you care about (Go, Python, JS, Rust, C++, Java, and even COBOL... probably).
2.  **`codefang` (The Analyzer)**: This tool consumes the data. It takes UASTs for static analysis or Git repositories for history analysis.

### Why UAST?

Most linters are language-specific. `eslint` for JS, `golangci-lint` for Go. Codefang uses a **Universal** AST. This means we can write a single "Complexity Analyzer" and it immediately works for Python, Go, and TypeScript.

### Pluggable Analysis

Analyzers are modular. You want to measure "Sentiment of Code Comments"? There's an analyzer for that (`sentiment`). You want to find "Typos in Variable Names"? There's one for that (`typos`).

---

## 5. Codefang as an AI Agent Tool

Don't just use AI to write code. Use Codefang to verify it. By giving `codefang` and `uast` to your AI agent as tools (via MCP or shell), you create a self-correcting quality loop.

**1. The Self-Correcting Coder**
*   **Scenario:** Agent generates a new function or refactors a file.
*   **Action:** Agent runs `codefang analyze -a complexity` on the new code.
*   **Outcome:** If complexity scores are high (e.g., > 15), the agent self-reflects: *"This is too complex. I need to simplify logic before showing it to the user."*
*   **Result:** Clean, maintainable code *before* it even hits the PR.

**2. Architectural Context Injection**
*   **Scenario:** You ask the agent about a high-level architectural change.
*   **Problem:** "Context Window Exceeded" or the agent hallucinates file relationships.
*   **Solution:** Agent runs `uast parse` to get the AST structure or `codefang analyze -a imports` to map the dependency graph. It learns the system architecture without reading every single line of text.

**3. Risk-Aware Refactoring**
*   **Scenario:** Agent is asked to refactor a legacy module.
*   **Action:** Agent runs `codefang history -a couples`.
*   **Insight:** *"Warning: Changing `User.go` usually breaks `Billing.go` 80% of the time."*
*   **Result:** Agent proactively checks related files for regressions, preventing bugs that a simple text-based analysis would miss.

**4. Style & Consistency Enforcement**
*   **Scenario:** "Make this look like the rest of the project."
*   **Action:** Agent analyzes `cohesion` and `comments` metrics of existing high-quality files to set a baseline.
*   **Result:** Agent generates code that matches the *structural* quality standards of your specific repo, not just generic language syntax.

---

### Ready to lift?

Check out the [Documentation](docs/) for deep dives, or just start piping commands and see what breaks.

*Happy Mining!*
