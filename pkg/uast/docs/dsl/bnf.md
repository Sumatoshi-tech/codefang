# UAST Query Language DSL - BNF Grammar Specification

## Overview

This document defines the complete BNF (Backus-Naur Form) grammar for the UAST Query Language DSL, a functional pipeline language for querying Universal Abstract Syntax Trees.

> **Note:** The BNF below is a formal reference. The actual parser is implemented as a PEG grammar using [pointlander/peg](https://github.com/pointlander/peg) in Go. The PEG grammar is the authoritative source for the parser implementation.

## BNF Grammar Rules

### Top-Level Query Structure

```bnf
<query> ::= <pipeline>

<pipeline> ::= <expression> 
            | <expression> "|>" <pipeline>

<expression> ::= <filter_expr>
              | <map_expr>
              | <reduce_expr>
              | <field_access>
              | <parenthesized_expr>

<parenthesized_expr> ::= "(" <pipeline> ")"
```

### Filter Expressions

```bnf
<filter_expr> ::= "filter" "(" <boolean_expr> ")"

<boolean_expr> ::= <comparison_expr>
                | <membership_expr>
                | <boolean_expr> "&&" <boolean_expr>
                | <boolean_expr> "||" <boolean_expr>
                | "!" <boolean_expr>
                | "(" <boolean_expr> ")"

<comparison_expr> ::= <value_expr> <comparison_op> <value_expr>

<comparison_op> ::= "==" | "!=" | "<" | ">" | "<=" | ">="

<membership_expr> ::= <value_expr> "has" <value_expr>
```

### Map and Reduce Expressions

```bnf
<map_expr> ::= "map" "(" <value_expr> ")"

<reduce_expr> ::= "reduce" "(" <reduce_op> ")"

<reduce_op> ::= "count"
             | "sum"
             | "min"
             | "max"
             | "first"
             | "last"
```

### Value Expressions

```bnf
<value_expr> ::= <field_access>
              | <literal>
              | <identifier>
              | "(" <value_expr> ")"

<field_access> ::= "." <identifier>
                | <field_access> "." <identifier>

<literal> ::= <string_literal>
           | <number_literal>
           | <boolean_literal>

<string_literal> ::= "\"" <string_chars> "\""

<string_chars> ::= <string_char>*

<string_char> ::= <printable_char> - "\"" - "\\"
               | "\\" <escape_char>

<escape_char> ::= "\"" | "\\" | "n" | "r" | "t"

<number_literal> ::= <integer> | <float>

<integer> ::= <digit>+

<float> ::= <digit>+ "." <digit>+
         | <digit>+ "." <digit>+ "e" <sign>? <digit>+
         | <digit>+ "e" <sign>? <digit>+

<boolean_literal> ::= "true" | "false"

<identifier> ::= <letter> (<letter> | <digit> | "_")*

<digit> ::= "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"

<letter> ::= "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j" 
          | "k" | "l" | "m" | "n" | "o" | "p" | "q" | "r" | "s" | "t" 
          | "u" | "v" | "w" | "x" | "y" | "z"
          | "A" | "B" | "C" | "D" | "E" | "F" | "G" | "H" | "I" | "J" 
          | "K" | "L" | "M" | "N" | "O" | "P" | "Q" | "R" | "S" | "T" 
          | "U" | "V" | "W" | "X" | "Y" | "Z"

<sign> ::= "+" | "-"

<printable_char> ::= <letter> | <digit> | <special_char>

<special_char> ::= " " | "!" | "#" | "$" | "%" | "&" | "'" | "(" | ")" 
                | "*" | "+" | "," | "-" | "." | "/" | ":" | ";" | "<" 
                | "=" | ">" | "?" | "@" | "[" | "]" | "^" | "_" | "`" 
                | "{" | "|" | "}" | "~"
```

### Whitespace (Ignored)

```bnf
<whitespace> ::= " " | "\t" | "\n" | "\r"
```

## Grammar Features

### 1. **Functional Pipeline Syntax**
The grammar supports left-to-right pipeline composition using the `|>` operator, allowing for natural functional composition:
```
filter(.type == "Function") |> map(.name) |> reduce(count)
```

### 2. **Field Access Patterns**
Direct property access on UAST nodes using dot notation:
```
.type          // Node type
.roles         // Node roles collection
.name          // Node name
.value         // Node value
```

### 3. **Membership Testing**
Optimized membership testing for collections using the `has` operator:
```
.roles has "Exported"    // O(1) hash-set lookup when optimized
```

### 4. **Boolean Logic**
Full boolean expression support with standard operators:
```
.type == "Function" && .roles has "Exported"
.name != "main" || .visibility == "public"
!(.roles has "Private")
```

### 5. **Type System Compatibility**
The grammar is designed to be compatible with Go's static type system, enabling compile-time type checking and optimization.

## Example Queries

### Basic Filtering
```
filter(.type == "Function")
```

### Complex Boolean Logic
```
filter(.type == "Function" && .roles has "Exported" && .name != "init")
```

### Pipeline Composition
```
filter(.type == "Function") |> map(.name) |> reduce(count)
```

### Nested Field Access
```
filter(.metadata.visibility == "public")
```

## Compilation Semantics

Each grammar construct maps to specific Go language features:

- **Pipelines** → Nested function calls with iterator composition
- **Filter expressions** → Boolean predicate closures
- **Map expressions** → Transformation function closures  
- **Membership tests** → Hash-set lookup operations (O(1))
- **Field access** → Direct struct field access
- **Literals** → Go literal values with appropriate types

> **Implementation:**
> - The parser is implemented using a PEG grammar (pointlander/peg) in Go, not BNF.
> - The parser produces an AST, which is lowered to Go closures for efficient, type-safe execution over UAST nodes.
> - All type, role, and property assignment is mapping-driven from YAML files per language.
> - The implementation is non-recursive (explicit stack for AST/UAST traversal).
> - The test suite covers all grammar features, including membership, boolean logic, and pipelines.

## Grammar Properties

- **LL(k) Parseable**: The grammar is designed to be efficiently parsed by LL(k) parsers
- **Left-recursive safe**: No left recursion in the grammar rules
- **Operator precedence**: Boolean operators follow standard precedence rules
- **Type-safe**: All operations are designed to be statically type-checked
- **Optimization-friendly**: Structure enables aggressive compile-time optimizations
- **Implementation is non-recursive**: Uses explicit stack for AST/UAST traversal
- **Mapping-driven**: All type/role/property assignment is from DSL mapping files
- **Test coverage**: The test suite covers all grammar features and edge cases

This BNF specification provides the foundation for implementing a complete parser for the UAST Query Language DSL.