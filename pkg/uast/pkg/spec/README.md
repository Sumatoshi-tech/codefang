# UAST JSON Schema

This directory contains the JSON schema for the Unified Abstract Syntax Tree (UAST) format as specified in `SPEC.md`.

## Files

- `uast-schema.json` - The main JSON schema definition
- `uast-example.json` - Example UAST document demonstrating the schema
- `README-schema.md` - This documentation file

## Schema Overview

The UAST schema defines a canonical, language-agnostic representation of source code ASTs. Each UAST node contains:

### Required Fields

- `type` - Canonical UAST node type (e.g., "Function", "Identifier", "Block")

### Optional Fields

- `id` - Stable identifier for the node (used for diffing and identity comparison)
- `token` - Surface text for leaf nodes; empty for non-leaf nodes
- `roles` - Array of orthogonal syntactic/semantic labels
- `pos` - Position information (line/column/byte offsets)
- `props` - Language-specific properties (arbitrary key-value pairs)
- `children` - Ordered list of child nodes

## Canonical Types

The schema includes all canonical UAST types from the specification:

- **Control Flow**: `If`, `Loop`, `Switch`, `Case`, `Return`, `Break`, `Continue`

> **Note:** The `Loop` node type replaces `For`, `While`, and `DoWhile`. The specific loop kind MUST be specified in the `props.kind` property (e.g., `for`, `while`, `do-while`) or as a role if needed for analysis.

- **Declarations**: `Function`, `FunctionDecl`, `Method`, `Class`, `Interface`, `Struct`, `Enum`, `EnumMember`, `Variable`, `Parameter`
- **Expressions**: `Call`, `Assignment`, `BinaryOp`, `UnaryOp`, `Literal`, `Identifier`
- **Data Structures**: `List`, `Dict`, `Set`, `Tuple`, `KeyValue`, `Index`, `Slice`
- **Language Features**: `Import`, `Package`, `Lambda`, `Try`, `Catch`, `Finally`, `Throw`
- **Documentation**: `Comment`, `DocString`
- **Type System**: `TypeAnnotation`, `Field`, `Property`, `Getter`, `Setter`
- **Advanced Features**: `Module`, `Namespace`, `Decorator`, `Spread`, `Cast`, `Await`, `Yield`, `Generator`, `Comprehension`, `Pattern`, `Match`
- **Fallback**: `Synthetic` - Used as fallback for unmapped nodes

## Canonical Roles

The schema includes all canonical UAST roles from the specification:

- **Semantic**: `Function`, `Declaration`, `Name`, `Reference`, `Assignment`, `Call`, `Parameter`, `Argument`
- **Flow Control**: `Condition`, `Body`, `Loop`, `Branch`, `Return`
- **Visibility**: `Public`, `Private`, `Static`, `Constant`, `Mutable`, `Exported`
- **Language Features**: `Import`, `Comment`, `Doc`, `Type`, `Operator`, `Index`, `Key`, `Value`
- **Object-Oriented**: `Interface`, `Class`, `Struct`, `Enum`, `Member`, `Getter`, `Setter`
- **Advanced Features**: `Module`, `Lambda`, `Try`, `Catch`, `Finally`, `Throw`, `Await`, `Yield`, `Spread`, `Pattern`, `Match`
- **Attributes**: `Attribute`, `Annotation`

## Position Information

The `pos` field contains position information with the following structure:

- `start_line` - Starting line number (1-indexed)
- `start_col` - Starting column number (1-indexed)
- `start_offset` - Starting byte offset
- `end_line` - Ending line number (1-indexed)
- `end_col` - Ending column number (1-indexed)
- `end_offset` - Ending byte offset

## Usage

### Validation

Use any JSON schema validator to validate UAST documents:

```bash
# Using ajv-cli
npm install -g ajv-cli
ajv validate -s uast-schema.json -d uast-example.json

# Using jsonschema (Python)
pip install jsonschema
jsonschema -i uast-example.json uast-schema.json
```

### Programmatic Usage

```go
// Load and validate schema
schemaBytes, _ := os.ReadFile("uast-schema.json")
var schema map[string]interface{}
json.Unmarshal(schemaBytes, &schema)

// Validate UAST document
validator := gojsonschema.NewGoLoader(schema)
document := gojsonschema.NewGoLoader(uastNode)
result, _ := gojsonschema.Validate(validator, document)

if result.Valid() {
    fmt.Println("Valid UAST document")
} else {
    fmt.Println("Invalid UAST document:", result.Errors())
}
```

## Conformance

All UAST implementations should:

1. Generate documents that validate against this schema
2. Use canonical types and roles where applicable
3. Include position information when available
4. Preserve the order of child nodes
5. Use properties for language-specific data
6. Use `Synthetic` type as fallback for unmapped nodes
7. Follow RFC 2119 requirement levels (MUST, SHOULD, MAY)

## Example

See `uast-example.json` for a complete example of a UAST document representing a simple Go function:

```go
func add(a, b int) int {
    return a + b
}
```

The example demonstrates:
- Function declaration with parameters
- Identifier nodes with Name roles
- Binary operation with operator properties
- Proper nesting and role assignment
- Position information
- Language-specific properties 