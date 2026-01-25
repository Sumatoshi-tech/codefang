# UAST Query Language DSL - PEG Grammar Specification

## Overview

This document defines the complete PEG (Parsing Expression Grammar) for the UAST Query Language DSL. PEG provides unambiguous parsing with ordered choice, making it ideal for parsing this functional pipeline language.

> **Implementation:** The parser is implemented using [pointlander/peg](https://github.com/pointlander/peg) in Go. The grammar below is the actual implementation grammar. The parser produces an AST, which is lowered to Go closures for efficient, type-safe execution over UAST nodes. All type, role, and property assignment is mapping-driven from YAML files per language. The implementation is non-recursive (explicit stack for AST/UAST traversal), and the test suite covers all grammar features, including membership, boolean logic, and pipelines.

## PEG Grammar Rules

### Entry Point

```peg
Query <- Pipeline EOF

Pipeline <- Expression (PipeOp Expression)*

PipeOp <- Spacing "|>" Spacing
```

### Core Expressions

```peg
Expression <- FilterExpr / MapExpr / ReduceExpr / FieldAccess / ParenExpr

ParenExpr <- "(" Spacing Pipeline Spacing ")"

FilterExpr <- "filter" Spacing "(" Spacing BooleanExpr Spacing ")"

MapExpr <- "map" Spacing "(" Spacing ValueExpr Spacing ")"

ReduceExpr <- "reduce" Spacing "(" Spacing ReduceOp Spacing ")"
```

### Boolean Expressions (with precedence)

```peg
BooleanExpr <- OrExpr

OrExpr <- AndExpr (OrOp AndExpr)*

AndExpr <- NotExpr (AndOp NotExpr)*

NotExpr <- NotOp? PrimaryBoolExpr

PrimaryBoolExpr <- ComparisonExpr / MembershipExpr / "(" Spacing BooleanExpr Spacing ")"

OrOp <- Spacing "||" Spacing

AndOp <- Spacing "&&" Spacing

NotOp <- "!" Spacing
```

### Comparison and Membership

```peg
ComparisonExpr <- ValueExpr Spacing ComparisonOp Spacing ValueExpr

ComparisonOp <- "==" / "!=" / "<=" / ">=" / "<" / ">"

MembershipExpr <- ValueExpr Spacing "has" Spacing ValueExpr
```

### Value Expressions

```peg
ValueExpr <- FieldAccess / Literal / Identifier / "(" Spacing ValueExpr Spacing ")"

FieldAccess <- "." Identifier ("." Identifier)*

Literal <- StringLiteral / NumberLiteral / BooleanLiteral
```

### Reduce Operations

```peg
ReduceOp <- "count" / "sum" / "min" / "max" / "first" / "last"
```

### Literals

```peg
StringLiteral <- "\"" StringChar* "\""

StringChar <- EscapedChar / (![\"] .)

EscapedChar <- "\\" ([\\"nrt] / UnicodeEscape)

UnicodeEscape <- "u" [0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F]

NumberLiteral <- Float / Integer

Float <- Integer "." [0-9]+ Exponent?
       / Integer Exponent

Integer <- [0-9]+

Exponent <- [eE] [+-]? [0-9]+

BooleanLiteral <- "true" / "false"
```

### Identifiers

```peg
Identifier <- [a-zA-Z_][a-zA-Z0-9_]*
```

### Whitespace and Comments

```peg
Spacing <- (Space / Comment)*

Space <- [ \t\n\r]

Comment <- "//" (![\n] .)* [\n]
         / "/*" (!"*/" .)* "*/"

EOF <- !.
```

## PEG-Specific Features

### 1. **Ordered Choice**
PEG uses ordered choice (/) rather than unordered alternation, ensuring deterministic parsing:
```peg
ComparisonOp <- "==" / "!=" / "<=" / ">=" / "<" / ">"
```
Order matters: `<=` must come before `<` to prevent incorrect parsing.

### 2. **Greedy Matching**
PEG operators are greedy by default:
```peg
StringChar* # Matches as many characters as possible
[0-9]+      # Matches one or more digits greedily
```

### 3. **Syntactic Predicates**
PEG supports lookahead without consumption:
```peg
![\"] .     # Match any character except quote
!"*/" .     # Match any character not starting "*/"
```

### 4. **Left-Factorization**
The grammar is structured to avoid backtracking:
```peg
# Good: left-factored
NumberLiteral <- Float / Integer
Float <- Integer "." [0-9]+ Exponent?

# Avoid: would cause backtracking
# NumberLiteral <- Integer "." [0-9]+ / Integer
```

## Semantic Actions (Implementation Notes)

When implementing this PEG grammar, each rule should produce appropriate AST nodes:

### Filter Expression
```go
func (p *Parser) FilterExpr() *FilterNode {
    // filter(boolean_expr) -> FilterNode{Predicate: BooleanExpr}
}
```

### Pipeline
```go
func (p *Parser) Pipeline() *PipelineNode {
    // expr |> expr |> expr -> PipelineNode{Stages: []Expression}
}
```

### Membership Expression  
```go
func (p *Parser) MembershipExpr() *MembershipNode {
    // .roles has "Exported" -> MembershipNode{Left: FieldAccess, Right: Literal}
    // Should optimize to hash-set lookup during compilation
}
```

## Error Recovery

The PEG grammar includes strategic points for error recovery:

```peg
# Recover from missing closing parenthesis
FilterExpr <- "filter" Spacing "(" Spacing BooleanExpr Spacing ")"?

# Recover from incomplete pipelines  
Pipeline <- Expression (PipeOp Expression / ERROR)*

ERROR <- # Custom error recovery rule
```

## Optimization Opportunities

### 1. **Membership Test Recognition**
The parser should identify `has` expressions for hash-set optimization:
```peg
MembershipExpr <- ValueExpr Spacing "has" Spacing ValueExpr
# Mark for O(1) hash-set lookup optimization
```

### 2. **Constant Folding**
Identify compile-time constant expressions:
```peg
ComparisonExpr <- ValueExpr Spacing ComparisonOp Spacing ValueExpr
# If both ValueExpr are literals, fold at compile-time
```

### 3. **Field Access Chains**
Optimize nested field access:
```peg
FieldAccess <- "." Identifier ("." Identifier)*
# Convert to single struct traversal operation
```

## Example Parse Trees

### Simple Filter
```
Input: filter(.type == "Function")

ParseTree:
Query
└── Pipeline
    └── FilterExpr
        └── ComparisonExpr
            ├── FieldAccess(.type)
            ├── ComparisonOp(==)
            └── StringLiteral("Function")
```

### Complex Pipeline
```
Input: filter(.type == "Function") |> map(.name) |> reduce(count)

ParseTree:
Query
└── Pipeline
    ├── FilterExpr
    │   └── ComparisonExpr...
    ├── PipeOp(|>)
    ├── MapExpr
    │   └── FieldAccess(.name)
    ├── PipeOp(|>)
    └── ReduceExpr
        └── ReduceOp(count)
```

## Implementation Considerations

### 1. **Packrat Parsing**
This grammar is suitable for packrat parsing with memoization to improve performance on complex expressions.

### 2. **Error Messages**
PEG's deterministic nature enables precise error reporting:
```
Error at line 1, column 15: Expected closing parenthesis after boolean expression
filter(.type == "Function"
                      ^
```

### 3. **AST Generation**
Each grammar rule should generate appropriate AST nodes that can be lowered to Go code:
- `FilterExpr` → `FilterNode` → Go predicate closure
- `MembershipExpr` → `MembershipNode` → Hash-set lookup
- `Pipeline` → `PipelineNode` → Composed function calls

### 4. **Type Checking Integration**
The PEG parser should integrate with the type checker to validate field access and operation compatibility with UAST node types.

## Performance Characteristics

- **Linear Time**: O(n) parsing for most inputs
- **Predictable**: No exponential worst-case behavior
- **Memory Efficient**: Streaming parsing with bounded lookahead
- **Cache Friendly**: Packrat memoization improves repeated sub-expression parsing

This PEG specification provides a complete, unambiguous grammar for parsing the UAST Query Language DSL with excellent performance characteristics and optimization opportunities.