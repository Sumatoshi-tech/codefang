# UAST Documentation

This directory contains comprehensive documentation for the UAST (Universal Abstract Syntax Tree) system.

## Documentation Index

### Getting Started
- **[Creating Mappings](CREATING_MAPPINGS.md)** - Complete guide to creating UAST mappings with real-world examples and recipes
- **[DSL Quick Reference](DSL_QUICK_REFERENCE.md)** - Quick reference for DSL syntax and features
- **[Adding Language Support](ADDING_LANGUAGE.md)** - How to add support for new programming languages

### Reference
- **[DSL Syntax](DSL_SYNTAX.md)** - Detailed DSL grammar and syntax reference
- **[Mapping Format](MAPPING_FORMAT.md)** - File format and structure for mapping files

### Advanced Topics
- **[DSL Grammar](dsl/)** - Detailed grammar specifications for the DSL

## Quick Start

1. **Read [Creating Mappings](CREATING_MAPPINGS.md)** for a comprehensive introduction
2. **Use [DSL Quick Reference](DSL_QUICK_REFERENCE.md)** for syntax lookup
3. **Follow [Adding Language Support](ADDING_LANGUAGE.md)** to add new languages

## Key Features

The UAST DSL supports:

- **Multi-value roles** - Assign multiple semantic roles to nodes
- **Inheritance** - Create base mappings and extend them
- **Conditional logic** - Apply mappings based on conditions
- **Advanced token extraction** - Extract tokens from various sources
- **Property mapping** - Map additional properties from AST
- **Children deduplication** - Automatic removal of duplicate children

## Example

```dsl
function_declaration <- (function_declaration name: (identifier) @name body: (block) @body) => uast(
    type: "Function",
    token: "@name",
    roles: "Declaration", "Function",
    children: "@body"
)
```

## Testing

```bash
# Test your mappings
uast parse -lang go -mapping mapping.uastmap test.go

# Query the generated UAST
uast parse -lang go test.go | uast query 'filter(.type == "Function")'
```

## Contributing

When adding new documentation:

1. Follow the existing style and structure
2. Include real-world examples
3. Test all code examples
4. Update this index when adding new files 