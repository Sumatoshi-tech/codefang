# UAST DSL Syntax

Query and transformation language for UAST trees.

Key primitives: `map`, `filter`, `reduce`, `rmap`, `rfilter`, field access, literals, function calls, pipelines (`|>`), logical operators (`&&`, `||`, `!`), comparisons (`==`, `!=`, `>`, `<`, `>=`, `<=`), membership (`has`).

Example queries:
- `map(.children) |> filter(.type == "FunctionDecl")`
- `filter(.roles has "Function")`
- `reduce(count)`

Grammar (PEG excerpt):

```
Pipeline <- Expr (Spacing '|' '>' Spacing Expr)*
Expr <- RFilter / RMap / Filter / Map / Reduce / OrExpr
Filter <- 'filter' ((Spacing '(' Spacing Predicate Spacing ')') / (Spacing Predicate))
Map <- 'map' ((Spacing '(' Spacing OrExpr Spacing ')') / (Spacing OrExpr))
Reduce <- 'reduce' ((Spacing '(' Spacing ReducerName Spacing ')') / (Spacing ReducerName))
FieldAccess <- '.' Identifier ('.' Identifier)*
Comparison <- Value Spacing CompOp Spacing Value
Membership <- FieldAccess Spacing 'has' Spacing Value
Literal <- String / Number / Boolean
Identifier <- [a-zA-Z_][a-zA-Z0-9_]*
```

See GoDoc for more details and examples. 