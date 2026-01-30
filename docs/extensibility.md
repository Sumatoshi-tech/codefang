# Extensibility Guide

Codefang is designed to be a platform, not just a tool. We expect and encourage the addition of new analyzers and languages.

## 1. Adding a New Analyzer

To add a new metric, you need to implement the `Analyzer` interface (or `StaticAnalyzer` for UAST-based metrics).

### Step 1: Define the Analyzer

Create a new package in `pkg/analyzers/myanalyzer`.

```go
package myanalyzer

import (
    "github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
    "github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

type MyAnalyzer struct {
    count int
}

func New() *MyAnalyzer {
    return &MyAnalyzer{}
}

func (a *MyAnalyzer) Name() string {
    return "if-counter"
}

// ... Implement other metadata methods
```

### Step 2: Implement the Visitor

If your analyzer can work by walking the tree (e.g., counting `if` statements), implement `VisitorProvider` and `AnalysisVisitor`.

```go
func (a *MyAnalyzer) CreateVisitor() analyze.AnalysisVisitor {
    return &Visitor{analyzer: a}
}

type Visitor struct {
    analyzer *MyAnalyzer
    localCount int
}

func (v *Visitor) VisitNode(n *node.Node) {
    if n.Type == "if_statement" {
        v.localCount++
    }
}

func (v *Visitor) GetReport() analyze.Report {
    return analyze.Report{
        "if_count": v.localCount,
    }
}
```

### Step 3: Register the Analyzer

In `cmd/codefang/commands/analyze.go`, add your analyzer to the factory registration list.

```go
func (ac *AnalyzeCommand) newService() *Service {
    return &Service{
        availableAnalyzers: []analyze.StaticAnalyzer{
            // ... existing
            myanalyzer.New(), // Add this line
        },
    }
}
```

## 2. Adding a New Language

Codefang uses `tree-sitter` for parsing. Adding a language involves:

1.  **Go Bindings**: Ensure there is a Go binding for the tree-sitter grammar (usually `sitter-lang` or via `go-sitter-forest`).
2.  **UAST Mapping**: This is the hardest part. You must map the language-specific AST nodes (e.g., `PyFunctionDef`) to Universal AST nodes (e.g., `FunctionGroup`).
    *   Edit `pkg/uast/pkg/uast.go` or the relevant mapping file.
    *   Define rules using XPath-like queries.

Example Mapping (Conceptual):
```yaml
uast:
  - type: "function_definition"
    uast_type: "FunctionGroup"
    children:
      - name: "name"
        uast_role: "Name"
```

## 3. Contributing

Please read `CONTRIBUTING.md` (if available) and ensure you run the full test suite before submitting a PR.
