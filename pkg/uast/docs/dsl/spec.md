# UAST Query Language DSL - Complete Specification

## Executive Summary

The UAST Query Language DSL is a functional pipeline language designed for querying Universal Abstract Syntax Trees. It combines the expressiveness of functional programming with the performance of compiled Go code, featuring static compilation to Go closures and O(1) optimized membership testing.

> **Implementation:** The DSL is parsed using a PEG grammar ([pointlander/peg](https://github.com/pointlander/peg)) in Go (not BNF). The parser produces an AST, which is lowered to Go closures for efficient, type-safe execution over UAST nodes. All type, role, and property assignment is mapping-driven from YAML files per language. The implementation is non-recursive (explicit stack for AST/UAST traversal), and the test suite covers all grammar features, including membership, boolean logic, and pipelines.

## Language Overview

### Core Philosophy

The DSL embodies three key principles:

1. **Functional Pipeline Composition**: Operations flow left-to-right using the `|>` operator
2. **Static Compilation**: All queries compile to efficient Go closures with no runtime reflection  
3. **Performance Optimization**: Membership predicates use hash-set lookups for O(1) performance
4. **Mapping-driven**: All type, role, and property assignment is from DSL mapping files per language
5. **Non-recursive implementation**: Explicit stack for AST/UAST traversal
6. **Test coverage**: The test suite covers all grammar features and edge cases

### Key Features

- **Functional pipeline syntax**: `map`, `filter`, `reduce`, field access, and membership testing
- **Compiles to static Go closures**: No reflection in hot loops
- **Embeddable**: CLI (`uast query 'filter(.roles has "Exported")'`), HTTP, or gRPC
- **Optimized membership predicates**: `.roles has "Exported"` uses O(1) hash-set lookup
- **Type-safe**: Leverages Go's static type system for compile-time validation

## Syntax Reference

### Basic Operations

#### Filter
Filters nodes based on boolean predicates:
```
filter(.type == "Function")
filter(.type == "Function" && .roles has "Exported")
```

#### Map  
Transforms each node using an expression:
```
map(.name)
map(.value)
```

#### Reduce
Aggregates nodes into a single result:
```
reduce(count)
reduce(sum)
reduce(first)
```

### Field Access
Direct property access using dot notation:
```
.type          # Node type
.roles         # Node roles collection  
.name          # Node name
.value         # Node value
.metadata      # Nested metadata access
```

### Membership Testing
Optimized collection membership testing:
```
.roles has "Exported"     # O(1) hash-set lookup
.tags has "deprecated"    # Check for tag presence
```

### Boolean Logic
Standard boolean operators with C-style precedence:
```
.type == "Function" && .roles has "Exported"
.name != "main" || .visibility == "public"  
!(.roles has "Private")
```

### Pipeline Composition
Left-to-right functional composition:
```
filter(.type == "Function") |> map(.name) |> reduce(count)
filter(.roles has "Exported") |> map(.metadata.documentation)
```

## Type System

### Node Types
The DSL operates on UAST nodes with these common fields:

```go
type Node struct {
    Type     string            // Node type ("Function", "Class", etc.)
    Roles    []string          // Semantic roles  
    Name     string            // Node identifier
    Value    interface{}       // Node value
    Children []Node            // Child nodes
    Metadata map[string]any    // Additional metadata
}
```

### Expression Types
- **Boolean expressions**: Result of comparisons and logical operations
- **Value expressions**: Field access, literals, identifiers
- **Pipeline expressions**: Composed functional operations

### Type Inference
The compiler performs static type checking:
```
filter(.type == "Function")     # ✓ Valid: string comparison
filter(.children > 5)           # ✓ Valid: slice length comparison  
filter(.name has "test")        # ✗ Invalid: string doesn't support 'has'
```

## Compilation Architecture

### 1. Parsing Phase
**Input**: DSL query string  
**Process**: PEG parser ([pointlander/peg](https://github.com/pointlander/peg)) generates Abstract Syntax Tree  
**Output**: DSL AST nodes  
**Optimization**: Syntax validation and error reporting

### 2. AST Lowering Phase  
**Input**: DSL AST  
**Process**: Transform DSL constructs to Go AST  
**Output**: Go function AST  
**Optimization**: Type inference and dead code elimination

### 3. Predicate Analysis Phase
**Input**: Boolean expressions  
**Process**: Rule engine analyzes and optimizes predicates  
**Output**: Optimized predicate functions  
**Optimization**: Hash-set conversion for membership tests

### 4. Code Generation Phase
**Input**: Go AST  
**Process**: Go compiler generates machine code  
**Output**: Static closures over UAST nodes  
**Optimization**: Inlining and register allocation

### 5. Runtime Execution Phase
**Input**: Compiled closures and UAST data  
**Process**: Iterator pipeline execution  
**Output**: Filtered/transformed results  
**Optimization**: Lazy evaluation and streaming

> **Note:** The mapping is loaded from YAML files and is required for correct operation of the parser and UAST conversion.

## Performance Optimizations

### Hash-Set Membership Testing
Membership predicates like `.roles has "Exported"` are optimized using hash-sets:

**Before optimization**:
```go
func hasRole(roles []string, target string) bool {
    for _, role := range roles {  // O(n) linear search
        if role == target {
            return true
        }
    }
    return false
}
```

**After optimization**:
```go
func hasRole(roleSet map[string]bool, target string) bool {
    return roleSet[target]  // O(1) hash lookup
}
```

### Static Closure Generation
Queries compile to static Go closures:

**DSL Query**:
```
filter(.type == "Function" && .roles has "Exported")
```

**Generated Go Closure**:
```go
func(node *Node) bool {
    return node.Type == "Function" && 
           roleSet[node.Type]["Exported"]
}
```

### Iterator Pipeline Composition
Pipelines compile to efficient nested function calls:

**DSL Pipeline**:
```
filter(.type == "Function") |> map(.name) |> reduce(count)
```

**Generated Go Code**:
```go
func(nodes Iterator) int {
    return reduce.Count(
        iter.Map(
            iter.Filter(nodes, func(n *Node) bool {
                return n.Type == "Function"
            }),
            func(n *Node) string {
                return n.Name
            }))
}
```

## Integration Patterns

### CLI Usage
```bash
# Filter exported functions
uast query 'filter(.type == "Function" && .roles has "Exported")'

# Count string literals  
uast query 'filter(.type == "String") |> reduce(count)'

# Extract function names
uast query 'filter(.type == "Function") |> map(.name)'
```

### Go API Usage
```go
// Navigation/query using the UAST DSL
nodes := root.QueryDSL("filter(.type == \"Function\" && .roles has \"Exported\")")

// Compiled query for reuse
query := uast.Compile("filter(.type == \"Function\")")
results := query.Execute(root)
```

### HTTP API Integration
```http
POST /api/query
Content-Type: application/json

{
  "query": "filter(.type == \"Function\") |> map(.name)",
  "ast": "...base64-encoded-uast..."
}
```

### gRPC Integration
```protobuf
service UASTQueryService {
  rpc Query(QueryRequest) returns (QueryResponse);
}

message QueryRequest {
  string query = 1;
  bytes ast_data = 2;
}
```

## Error Handling

### Compile-Time Errors
- **Syntax errors**: Invalid DSL syntax
- **Type errors**: Incompatible operations
- **Field errors**: Unknown node fields

### Runtime Errors  
- **Null reference**: Accessing fields on nil nodes
- **Type assertion**: Unexpected node types
- **Resource limits**: Query timeout or memory limits

## Future Extensions

### Planned Features
- **Recursive queries**: Support for `//` descendant traversal
- **Pattern matching**: Advanced node pattern matching
- **Custom functions**: User-defined transformation functions
- **Query optimization**: Advanced predicate pushdown
- **Parallel execution**: Multi-threaded query execution

### Compatibility
The DSL is designed to be forward-compatible with future UAST schema versions and additional language features.

## Grammar Specifications

This specification includes complete formal grammars:

1. **BNF Grammar** (`uast-dsl-bnf-grammar.md`): Complete Backus-Naur Form specification
2. **PEG Grammar** (`uast-dsl-peg-grammar.md`): Parsing Expression Grammar with implementation details

## Conclusion

The UAST Query Language DSL provides a powerful, efficient, and type-safe way to query Universal Abstract Syntax Trees. By combining functional programming paradigms with Go's performance characteristics, it enables both elegant query composition and high-performance execution suitable for production use cases including static analysis tools, code transformation systems, and development environments.