# Creating UAST Mappings

This guide covers how to create and customize UAST mappings using our DSL (Domain Specific Language). The DSL provides a powerful way to transform Tree-sitter AST nodes into UAST (Universal Abstract Syntax Tree) nodes with rich semantic information.

## Table of Contents

1. [Basic Mapping Syntax](#basic-mapping-syntax)
2. [Advanced Features](#advanced-features)
   - [Multi-Value Roles](#multi-value-roles)
   - [Inheritance](#inheritance)
   - [Conditional Logic](#conditional-logic)
   - [Token Extraction](#token-extraction)
   - [Property Mapping](#property-mapping)
   - [Children Deduplication](#children-deduplication)
3. [Real-World Examples](#real-world-examples)
4. [Recipes and Best Practices](#recipes-and-best-practices)
5. [Testing Your Mappings](#testing-your-mappings)

## Basic Mapping Syntax

The basic mapping syntax follows this pattern:

```
node_type <- (tree_sitter_pattern) => uast(
    type: "UASTType",
    token: "token_specification",
    roles: "Role1", "Role2",
    children: "child1", "child2",
    property1: "property_value1",
    property2: "property_value2"
)
```

### Simple Example

```dsl
function_declaration <- (function_declaration name: (identifier) @name body: (block) @body) => uast(
    type: "Function",
    token: "@name",
    roles: "Declaration",
    children: "@body"
)
```

This maps a Tree-sitter `function_declaration` node to a UAST `Function` node with:
- Token extracted from the `@name` capture
- `Declaration` role
- Children from the `@body` capture

## Advanced Features

### Multi-Value Roles

You can assign multiple roles to a single node:

```dsl
class_declaration <- (class_declaration name: (identifier) @name) => uast(
    type: "Class",
    token: "@name",
    roles: "Class", "Declaration", "Type"
)
```

### Inheritance

Mappings can inherit from other mappings using the `# Extends` comment:

```dsl
base_expression <- (expression) => uast(
    type: "Expression",
    roles: "Expression"
)

binary_expression <- (binary_expression left: (expression) @left op: (operator) @op right: (expression) @right) => uast(
    type: "BinaryExpression",
    token: "@op",
    roles: "Expression", "Binary"
) # Extends base_expression
```

### Conditional Logic

Use `when` clauses to apply mappings conditionally:

```dsl
arithmetic_expression <- (arithmetic_expression left: (expression) @left op: (arithmetic_operator) @op right: (expression) @right) => uast(
    type: "ArithmeticExpression",
    token: "@op",
    roles: "Expression", "Arithmetic"
) when op == "+" || op == "-" || op == "*" || op == "/"
```

### Token Extraction

Advanced token extraction supports multiple strategies:

```dsl
# Extract from self (node's own text)
self_token <- (identifier) => uast(
    type: "Identifier",
    token: "self"
)

# Extract from child field
child_token <- (function_call function: (identifier) @func) => uast(
    type: "Call",
    token: "child:identifier"
)

# Extract from descendant (anywhere in subtree)
descendant_token <- (complex_expression) => uast(
    type: "Expression",
    token: "descendant:identifier"
)
```

### Property Mapping

Map additional properties from the AST:

```dsl
function_declaration <- (function_declaration name: (identifier) @name params: (parameter_list) @params body: (block) @body) => uast(
    type: "Function",
    token: "@name",
    roles: "Declaration",
    children: "@params", "@body",
    name: "@name",
    params: "@params",
    body: "@body"
)
```

### Children Deduplication

The DSL automatically deduplicates children to avoid redundancy:

```dsl
complex_node <- (complex_node field1: (child1) @c1 (child2) @c2 (child1) @c1) => uast(
    type: "Complex",
    children: "@c1", "@c2"
)
```

Even though `@c1` appears twice in the pattern, it will only appear once in the children list.

## Real-World Examples

### Go Language Mapping

```dsl
function_declaration <- (function_declaration name: (identifier) @name params: (parameter_list) @params body: (block) @body) => uast(
    type: "Function",
    token: "@name",
    roles: "Declaration",
    children: "@params", "@body",
    name: "@name",
    params: "@params"
)

method_declaration <- (method_declaration name: (identifier) @name receiver: (parameter_list) @receiver params: (parameter_list) @params body: (block) @body) => uast(
    type: "Method",
    token: "@name",
    roles: "Declaration", "Method"
) # Extends function_declaration

var_declaration <- (var_declaration name: (identifier) @name type: (type_annotation) @type value: (expression) @value) => uast(
    type: "Variable",
    token: "@name",
    roles: "Declaration", "Variable",
    children: "@type", "@value",
    name: "@name",
    type_info: "descendant:type_annotation"
)

if_statement <- (if_statement condition: (expression) @cond consequence: (block) @conseq alternative: (block) @alt) => uast(
    type: "If",
    roles: "Statement", "Conditional",
    children: "@cond", "@conseq", "@alt"
)
```

### JavaScript/TypeScript Mapping

```dsl
function_expression <- (function_expression name: (identifier) @name params: (formal_parameters) @params body: (statement_block) @body) => uast(
    type: "Function",
    token: "@name",
    roles: "Expression", "Function",
    children: "@params", "@body"
)

arrow_function <- (arrow_function params: (formal_parameters) @params body: (statement_block) @body) => uast(
    type: "ArrowFunction",
    roles: "Expression", "Function",
    children: "@params", "@body"
)

class_declaration <- (class_declaration name: (identifier) @name body: (class_body) @body) => uast(
    type: "Class",
    token: "@name",
    roles: "Declaration", "Class",
    children: "@body"
)

import_statement <- (import_statement source: (string) @source) => uast(
    type: "Import",
    roles: "Statement", "Import",
    source: "@source"
)
```

### Python Mapping

```dsl
function_definition <- (function_definition name: (identifier) @name parameters: (parameters) @params body: (block) @body) => uast(
    type: "Function",
    token: "@name",
    roles: "Declaration", "Function",
    children: "@params", "@body"
)

class_definition <- (class_definition name: (identifier) @name body: (block) @body) => uast(
    type: "Class",
    token: "@name",
    roles: "Declaration", "Class",
    children: "@body"
)

base_expression <- (expression) => uast(
    type: "Expression",
    roles: "Expression"
)

comparison_expression <- (comparison_expression left: (expression) @left operator: (comparison_operator) @op right: (expression) @right) => uast(
    type: "Comparison",
    token: "@op",
    roles: "Expression", "Comparison"
) # Extends base_expression
```

## Recipes and Best Practices

### Recipe 1: Language-Agnostic Function Mapping

Create a mapping that works across multiple languages:

```dsl
function_base <- (function_declaration name: (identifier) @name body: (block) @body) => uast(
    type: "Function",
    token: "@name",
    roles: "Declaration", "Function",
    children: "@body"
)

go_function <- (function_declaration name: (identifier) @name params: (parameter_list) @params body: (block) @body) => uast(
    type: "Function",
    token: "@name",
    roles: "Declaration", "Function",
    children: "@params", "@body"
) # Extends function_base

js_function <- (function_declaration name: (identifier) @name params: (formal_parameters) @params body: (statement_block) @body) => uast(
    type: "Function",
    token: "@name",
    roles: "Declaration", "Function",
    children: "@params", "@body"
) # Extends function_base
```

### Recipe 2: Conditional Type Mapping

Map different types based on conditions:

```dsl
expression <- (expression) => uast(
    type: "Expression",
    roles: "Expression"
)

arithmetic_expression <- (binary_expression left: (expression) @left operator: (arithmetic_operator) @op right: (expression) @right) => uast(
    type: "ArithmeticExpression",
    token: "@op",
    roles: "Expression", "Arithmetic"
)

logical_expression <- (binary_expression left: (expression) @left operator: (logical_operator) @op right: (expression) @right) => uast(
    type: "LogicalExpression",
    token: "@op",
    roles: "Expression", "Logical"
)
```

### Recipe 3: Advanced Token Extraction

```dsl
function_call <- (call_expression function: (identifier) @func) => uast(
    type: "Call",
    token: "child:identifier",
    roles: "Expression", "Call",
    function: "child:identifier"
)

typed_variable <- (variable_declaration name: (identifier) @name type: (type_annotation) @type) => uast(
    type: "Variable",
    token: "@name",
    roles: "Declaration", "Variable",
    name: "child:identifier",
    type_info: "descendant:type_annotation"
)
```

### Recipe 4: Error Handling and Validation

```dsl
safe_property <- (object_property key: (property_identifier) @key value: (expression) @value) => uast(
    type: "Property",
    token: "@key",
    roles: "Property",
    key: "@key",
    value: "@value"
)

conditional_role <- (identifier) => uast(
    type: "Identifier",
    token: "self",
    roles: "Name"
)
```

## Testing Your Mappings

### 1. Create Test Files

Create test files with your mappings:

```bash
# Create a test mapping file
cat > test_mapping.uastmap << 'EOF'
function_declaration <- (function_declaration name: (identifier) @name body: (block) @body) => uast(
    type: "Function",
    token: "@name",
    roles: "Declaration",
    children: "@body"
)
EOF
```

### 2. Test with Sample Code

```bash
# Test Go mapping
echo 'package main

func Hello() {
    fmt.Println("Hello, World!")
}' > test.go

# Parse with your mapping
uast parse -lang go -mapping test_mapping.uastmap test.go
```

### 3. Validate UAST Output

```bash
# Query the generated UAST
uast parse -lang go test.go | uast query 'filter(.type == "Function")'
```

## Best Practices

1. **Use Descriptive Names**: Choose clear, descriptive names for your UAST types and roles.

2. **Leverage Inheritance**: Create base mappings and extend them for language-specific variations.

3. **Test Thoroughly**: Always test your mappings with real code samples.

4. **Document Assumptions**: Comment your mappings to explain complex logic.

5. **Use Consistent Patterns**: Maintain consistency across similar node types.

6. **Optimize for Performance**: Avoid overly complex patterns that might impact parsing performance.

7. **Handle Edge Cases**: Consider error conditions and edge cases in your mappings.

## Troubleshooting

### Common Issues

1. **Parse Errors**: Check your DSL syntax carefully, especially parentheses and quotes.

2. **Missing Captures**: Ensure all referenced captures (`@name`) exist in your pattern.

3. **Invalid Roles**: Make sure role names are valid strings.

4. **Circular Dependencies**: Avoid circular inheritance in your mappings.

### Debugging Tips

1. Use the `-debug` flag when parsing to see detailed information.
2. Test patterns incrementally, starting with simple cases.
3. Validate your Tree-sitter patterns separately before adding UAST mappings.
4. Check the generated UAST structure to ensure it matches your expectations.

## Next Steps

- Explore the [DSL Syntax Reference](DSL_SYNTAX.md) for detailed grammar information
- Check [Adding Language Support](ADDING_LANGUAGE.md) for language-specific setup
- Review [Mapping Format](MAPPING_FORMAT.md) for file structure details
- Test your mappings with the UAST CLI tools 