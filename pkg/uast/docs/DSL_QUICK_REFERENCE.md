# UAST DSL Quick Reference

This is a quick reference for the UAST DSL syntax and features.

## Basic Syntax

```
rule_name <- (tree_sitter_pattern) => uast(
    type: "UASTType",
    token: "token_spec",
    roles: "Role1", "Role2",
    children: "child1", "child2",
    props: {
        "key": "value"
    }
)
```

## Tree-sitter Pattern Syntax

### Basic Patterns
```dsl
# Simple node
(identifier)

# Named capture
(identifier) @name

# Field with capture
(function_declaration name: (identifier) @name)

# Multiple fields
(function_declaration name: (identifier) @name body: (block) @body)
```

### Pattern Operators
```dsl
# Optional node
(identifier)?

# One or more
(expression)+

# Zero or more
(statement)*

# Choice
(if_statement / while_statement / for_statement)
```

## UAST Specification

### Type
```dsl
type: "Function"           # Simple type
type: "BinaryExpression"   # Complex type
```

### Token Extraction
```dsl
token: "@name"                    # From capture
token: "self"                     # From node itself
token: "child:identifier"         # From direct child
token: "descendant:identifier"    # From any descendant
```

### Roles
```dsl
roles: "Declaration"                    # Single role
roles: "Function", "Declaration"        # Multiple roles
roles: "Expression", "Binary", "Arithmetic"  # Many roles
```

### Children
```dsl
children: "@body"                    # Single child
children: "@params", "@body"         # Multiple children
children: "@left", "@right", "@op"   # Many children
```

### Properties
```dsl
props: {
    "name": "@name",
    "type": "descendant:type_annotation",
    "value": "@value"
}
```

## Advanced Features

### Inheritance
```dsl
base_rule <- (expression) => uast(
    type: "Expression",
    roles: "Expression"
)

derived_rule <- (binary_expression left: (expression) @left right: (expression) @right) => uast(
    type: "BinaryExpression",
    roles: "Expression", "Binary"
) # Extends base_rule
```

### Conditional Logic
```dsl
# Simple condition
arithmetic_op <- (binary_expression operator: (operator) @op) => uast(
    type: "ArithmeticExpression"
) when op == "+" || op == "-"

# Complex condition
valid_identifier <- (identifier) => uast(
    type: "Identifier"
) when self != "undefined" && self != "null" && self != ""
```

### Children Deduplication
```dsl
# Automatic deduplication
complex_node <- (complex_node field1: (child1) @c1 (child2) @c2 (child1) @c1) => uast(
    type: "Complex",
    children: "@c1", "@c2"  # @c1 appears only once
)
```

## Common Patterns

### Function Declaration
```dsl
function_declaration <- (function_declaration name: (identifier) @name params: (parameter_list) @params body: (block) @body) => uast(
    type: "Function",
    token: "@name",
    roles: "Declaration", "Function",
    children: "@params", "@body",
    props: {
        "name": "@name",
        "params": "@params"
    }
)
```

### Variable Declaration
```dsl
variable_declaration <- (variable_declaration name: (identifier) @name type: (type_annotation) @type value: (expression) @value) => uast(
    type: "Variable",
    token: "@name",
    roles: "Declaration", "Variable",
    children: "@type", "@value",
    props: {
        "name": "@name",
        "type": "descendant:type_annotation"
    }
)
```

### Class Declaration
```dsl
class_declaration <- (class_declaration name: (identifier) @name body: (class_body) @body) => uast(
    type: "Class",
    token: "@name",
    roles: "Declaration", "Class", "Type",
    children: "@body"
)
```

### Method Declaration
```dsl
method_declaration <- (method_declaration name: (identifier) @name receiver: (parameter_list) @receiver params: (parameter_list) @params body: (block) @body) => uast(
    type: "Method",
    token: "@name",
    roles: "Declaration", "Method",
    children: "@receiver", "@params", "@body"
) # Extends function_declaration
```

### Conditional Statement
```dsl
if_statement <- (if_statement condition: (expression) @cond consequence: (block) @conseq alternative: (block) @alt) => uast(
    type: "If",
    roles: "Statement", "Conditional",
    children: "@cond", "@conseq", "@alt"
)
```

### Loop Statement
```dsl
for_statement <- (for_statement init: (for_clause) @init condition: (expression) @cond post: (expression) @post body: (block) @body) => uast(
    type: "For",
    roles: "Statement", "Loop",
    children: "@init", "@cond", "@post", "@body"
)
```

### Expression Patterns
```dsl
# Binary expression
binary_expression <- (binary_expression left: (expression) @left operator: (operator) @op right: (expression) @right) => uast(
    type: "BinaryExpression",
    token: "@op",
    roles: "Expression", "Binary",
    children: "@left", "@right"
)

# Call expression
call_expression <- (call_expression function: (expression) @func arguments: (argument_list) @args) => uast(
    type: "Call",
    token: "child:identifier",
    roles: "Expression", "Call",
    children: "@func", "@args"
)
```

## Language-Specific Examples

### Go
```dsl
# Go function
function_declaration <- (function_declaration name: (identifier) @name params: (parameter_list) @params body: (block) @body) => uast(
    type: "Function",
    token: "@name",
    roles: "Declaration",
    children: "@params", "@body"
)

# Go method
method_declaration <- (method_declaration receiver: (parameter_list) @receiver name: (identifier) @name params: (parameter_list) @params body: (block) @body) => uast(
    type: "Method",
    token: "@name",
    roles: "Declaration", "Method",
    children: "@receiver", "@params", "@body"
)
```

### JavaScript
```dsl
# JS function expression
function_expression <- (function_expression name: (identifier) @name params: (formal_parameters) @params body: (statement_block) @body) => uast(
    type: "Function",
    token: "@name",
    roles: "Expression", "Function",
    children: "@params", "@body"
)

# JS arrow function
arrow_function <- (arrow_function params: (formal_parameters) @params body: (statement_block) @body) => uast(
    type: "ArrowFunction",
    roles: "Expression", "Function",
    children: "@params", "@body"
)
```

### Python
```dsl
# Python function definition
function_definition <- (function_definition name: (identifier) @name parameters: (parameters) @params body: (block) @body) => uast(
    type: "Function",
    token: "@name",
    roles: "Declaration", "Function",
    children: "@params", "@body"
)

# Python class definition
class_definition <- (class_definition name: (identifier) @name body: (block) @body) => uast(
    type: "Class",
    token: "@name",
    roles: "Declaration", "Class",
    children: "@body"
)
```

## Best Practices

1. **Use descriptive names** for UAST types and roles
2. **Leverage inheritance** for common patterns
3. **Test thoroughly** with real code samples
4. **Document complex logic** with comments
5. **Use consistent patterns** across similar node types
6. **Handle edge cases** in your conditions
7. **Optimize for performance** by avoiding overly complex patterns

## Common Issues

- **Parse errors**: Check parentheses and quotes
- **Missing captures**: Ensure all `@name` references exist
- **Invalid roles**: Use valid string literals
- **Circular dependencies**: Avoid circular inheritance
- **Performance issues**: Simplify complex patterns

## Testing Commands

```bash
# Test parsing
uast parse -lang go -mapping mapping.uastmap test.go

# Query UAST
uast parse -lang go test.go | uast query 'filter(.type == "Function")'

# Debug mode
uast parse -lang go -debug test.go
``` 