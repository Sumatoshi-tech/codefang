# Unified Abstract Syntax Tree (UAST) Specification

**Document Status:** Draft Specification
**Version:** 1.0
**Date:** 2024
**Category:** Technical Specification

## Status of this Document

This document specifies the Unified Abstract Syntax Tree (UAST) format, a language-agnostic representation of source code designed for static analysis, refactoring, and cross-language tooling. This specification is intended for implementers and users of UAST-based tools and libraries.

Distribution of this memo is unlimited.

## Abstract

The Unified Abstract Syntax Tree (UAST) is a canonical, language-agnostic representation of source code designed for static analysis, refactoring, and cross-language tooling. UAST provides a standardized format for representing abstract syntax trees across multiple programming languages, enabling cross-language static analysis, refactoring tools, and code transformation systems.

## 1. Introduction

### 1.1 Purpose and Scope

The UAST specification defines a standardized format for representing abstract syntax trees across multiple programming languages. The goal is to enable cross-language static analysis, refactoring tools, and code transformation systems.

### 1.2 Terminology

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://datatracker.ietf.org/doc/html/rfc2119).

### 1.3 Document Structure

This specification is organized as follows:
- Section 2: UAST Node Structure
- Section 3: Canonical Types and Roles
- Section 4: Serialization Formats
- Section 5: Extensibility Guidelines
- Section 6: Implementation Requirements
- Section 7: Examples
- Section 8: Security Considerations
- Section 9: IANA Considerations

## 2. UAST Node Structure

### 2.1 Core Node Definition

Each UAST node MUST be represented by a data structure with the following fields:

- `id`: A stable identifier for the node (OPTIONAL)
- `type`: A canonical, language-agnostic type identifier (REQUIRED)
- `token`: The surface text for leaf nodes; empty for non-leaf nodes (OPTIONAL)
- `roles`: A list of orthogonal syntactic or semantic labels (OPTIONAL)
- `pos`: Position information in the source code (OPTIONAL)
- `props`: Arbitrary key-value pairs for language-specific or extra data (OPTIONAL)
- `children`: An ordered list of child nodes (REQUIRED)

### 2.2 Required Fields

#### 2.2.1 Type Field

The `type` field MUST be present and MUST contain a canonical, language-agnostic type identifier. The type MUST be stable across different implementations and MUST be drawn from the canonical types defined in Section 3.1.

#### 2.2.2 Children Field

The `children` field MUST be present and MUST contain an ordered list of child nodes. The order MUST match the order of children in the source abstract syntax tree.

### 2.3 Optional Fields

#### 2.3.1 Id Field

The `id` field MAY be present and SHOULD contain a stable identifier for the node. This field is used for diffing and identity comparison but is not required for all workflows.

#### 2.3.2 Token Field

The `token` field MAY be present and MUST contain the surface text for leaf nodes. For non-leaf nodes, this field MUST be empty.

#### 2.3.3 Roles Field

The `roles` field MAY be present and MUST contain a list of orthogonal syntactic or semantic labels. Each role MUST be a string from the canonical role set defined in Section 3.2.

#### 2.3.4 Pos Field

The `pos` field MAY be present and MUST contain position information when available. The position information MUST include:

- `start_line`: 1-indexed line number
- `start_col`: 1-indexed column number
- `start_offset`: Byte offset from start of file
- `end_line`: 1-indexed line number
- `end_col`: 1-indexed column number
- `end_offset`: Byte offset from start of file

#### 2.3.5 Props Field

The `props` field MAY be present and MUST contain arbitrary key-value pairs for language-specific or extra data. All keys and values MUST be strings.

#### 2.3.1. Synthetic Node Type (Fallback for Unmapped Nodes)

If a parser encounters a language construct that does not have a canonical UAST mapping, the implementation MAY emit a node of type `Synthetic` as a fallback. This `Synthetic` node MUST wrap any mapped children of the unmapped construct. If there is only one mapped child, the implementation MAY emit that child directly. If there are no mapped children, the implementation MUST omit the node.

The `Synthetic` node type is reserved for this fallback purpose and MUST NOT be used for any canonical language construct.

## 3. Canonical Types and Roles

### 3.1 Canonical UAST Types

The following table defines the canonical, language-agnostic UAST types. All implementations SHOULD use these types where applicable. New types MAY be added as needed for language-specific constructs, but these form the core cross-language vocabulary.

| Type              | Description                                      | Usage Requirements |
|-------------------|--------------------------------------------------|-------------------|
| File              | Root node for a source file                      | MUST be used for file roots |
| Function          | Function or method declaration                   | SHOULD be used for function declarations |
| FunctionDecl      | Function declaration (alias for Function)        | MAY be used as alternative to Function |
| Method            | Method declaration (object-oriented)             | SHOULD be used for OOP methods |
| Class             | Class declaration                                | SHOULD be used for class declarations |
| Interface         | Interface declaration                            | SHOULD be used for interface declarations |
| Struct            | Struct/record declaration                        | SHOULD be used for struct declarations |
| Enum              | Enum declaration                                 | SHOULD be used for enum declarations |
| EnumMember        | Enum member/variant                              | SHOULD be used for enum members |
| Variable          | Variable declaration                             | SHOULD be used for variable declarations |
| Parameter         | Function/method parameter                        | SHOULD be used for function parameters |
| Block             | Block of statements                              | SHOULD be used for statement blocks |
| If                | If statement                                     | SHOULD be used for conditional statements |
| Loop              | Loop construct (for, while, do-while, etc.)      | SHOULD be used for all loop constructs; loop kind MUST be specified in `props.kind` or as a role (e.g., `For`, `While`, `DoWhile`) |
| Switch            | Switch/case statement                            | SHOULD be used for switch statements |
| Case              | Case/branch in switch                            | SHOULD be used for switch cases |
| Return            | Return statement                                 | SHOULD be used for return statements |
| Break             | Break statement                                  | SHOULD be used for break statements |
| Continue          | Continue statement                               | SHOULD be used for continue statements |
| Assignment        | Assignment statement                             | SHOULD be used for assignments |
| Call              | Function/method call                             | SHOULD be used for function calls |
| Identifier        | Identifier (variable, function, etc.)            | SHOULD be used for identifiers |
| Literal           | Literal value (string, number, etc.)             | SHOULD be used for literal values |
| BinaryOp          | Binary operator (e.g., +, -, *, /, ==, etc.)     | SHOULD be used for binary operations |
| UnaryOp           | Unary operator (e.g., !, -, ++)                  | SHOULD be used for unary operations |
| Import            | Import/include statement                         | SHOULD be used for import statements |
| Package           | Package/module declaration                       | SHOULD be used for package declarations |
| Attribute         | Attribute/annotation (e.g., @Override)           | SHOULD be used for attributes |
| Comment           | Comment node                                     | SHOULD be used for comments |
| DocString         | Documentation string                             | SHOULD be used for documentation |
| TypeAnnotation    | Type annotation                                  | SHOULD be used for type annotations |
| Field             | Field/member variable declaration                | SHOULD be used for field declarations |
| Property          | Property (object-oriented)                       | SHOULD be used for properties |
| Getter            | Getter method                                    | SHOULD be used for getter methods |
| Setter            | Setter method                                    | SHOULD be used for setter methods |
| Lambda            | Lambda/anonymous function                        | SHOULD be used for lambda functions |
| Try               | Try/catch/finally block                          | SHOULD be used for try blocks |
| Catch             | Catch block                                      | SHOULD be used for catch blocks |
| Finally           | Finally block                                    | SHOULD be used for finally blocks |
| Throw             | Throw/raise statement                            | SHOULD be used for throw statements |
| Module            | Module declaration                               | SHOULD be used for module declarations |
| Namespace         | Namespace declaration                            | SHOULD be used for namespace declarations |
| Decorator         | Decorator (Python, TypeScript, etc.)             | SHOULD be used for decorators |
| Spread            | Spread/rest element (e.g., ...args)              | SHOULD be used for spread operations |
| Tuple             | Tuple expression                                 | SHOULD be used for tuple expressions |
| List              | List/array expression                            | SHOULD be used for list expressions |
| Dict              | Dictionary/map expression                        | SHOULD be used for dictionary expressions |
| Set               | Set expression                                   | SHOULD be used for set expressions |
| KeyValue          | Key-value pair                                   | SHOULD be used for key-value pairs |
| Index             | Indexing operation (e.g., arr[0])                | SHOULD be used for indexing operations |
| Slice             | Slice operation (e.g., arr[1:3])                 | SHOULD be used for slice operations |
| Cast              | Type cast                                        | SHOULD be used for type casts |
| Await             | Await expression                                 | SHOULD be used for await expressions |
| Yield             | Yield expression                                 | SHOULD be used for yield expressions |
| Generator         | Generator function                               | SHOULD be used for generator functions |
| Comprehension     | List/dict/set comprehension                      | SHOULD be used for comprehensions |
| Pattern           | Pattern matching construct                       | SHOULD be used for pattern matching |
| Match             | Match statement (e.g., Python 3.10+)             | SHOULD be used for match statements |

> **Note:** The `Loop` node type replaces `For`, `While`, and `DoWhile`. The specific loop kind MUST be specified in the `props.kind` property (e.g., `for`, `while`, `do-while`) or as a role if needed for analysis.

### 3.2 Canonical UAST Roles

The following table defines the canonical, language-agnostic UAST roles. Roles annotate nodes with orthogonal syntactic or semantic properties, enabling richer queries and analyses. All implementations SHOULD use these roles where applicable.

| Role         | Description                                                      | Usage Requirements |
|--------------|------------------------------------------------------------------|-------------------|
| Function     | Node represents a function or method                             | SHOULD be used for function nodes |
| Declaration  | Node is a declaration (function, variable, class, etc.)          | SHOULD be used for declaration nodes |
| Name         | Node is a name/identifier (of a variable, function, etc.)        | SHOULD be used for identifier nodes |
| Reference    | Node is a reference to a declared entity                         | SHOULD be used for reference nodes |
| Assignment   | Node is an assignment target or statement                        | SHOULD be used for assignment nodes |
| Call         | Node is a function/method call                                   | SHOULD be used for function call nodes |
| Parameter    | Node is a function/method parameter                              | SHOULD be used for parameter nodes |
| Argument     | Node is an argument in a call                                    | SHOULD be used for argument nodes |
| Condition    | Node is a condition (e.g., in if/while)                          | SHOULD be used for condition nodes |
| Body         | Node is a body/block of statements                               | SHOULD be used for body nodes |
| Exported     | Node is exported/public (as per language rules)                  | SHOULD be used for exported nodes |
| Public       | Node is explicitly public                                        | SHOULD be used for public nodes |
| Private      | Node is explicitly private                                       | SHOULD be used for private nodes |
| Static       | Node is static (class-level, not instance)                       | SHOULD be used for static nodes |
| Constant     | Node is a constant definition                                    | SHOULD be used for constant nodes |
| Mutable      | Node is mutable/assignable                                       | SHOULD be used for mutable nodes |
| Getter       | Node is a getter method/property                                 | SHOULD be used for getter nodes |
| Setter       | Node is a setter method/property                                 | SHOULD be used for setter nodes |
| Literal      | Node is a value (string, number, etc.)                           | SHOULD be used for literal values |
| Variable     | Node is a variable                                               | SHOULD be used for variable nodes |
| Loop         | Node is a loop construct (for, while, etc.)                      | SHOULD be used for loop nodes |
| Branch       | Node is a branch/case in a switch/match                          | SHOULD be used for branch nodes |
| Import       | Node is an import/include statement                              | SHOULD be used for import nodes |
| Doc          | Node is a documentation string                                   | SHOULD be used for documentation nodes |
| Comment      | Node is a comment                                                | SHOULD be used for comment nodes |
| Attribute    | Node is an attribute/annotation/decorator                        | SHOULD be used for attribute nodes |
| Annotation   | Node is an annotation (Java, etc.)                               | SHOULD be used for annotation nodes |
| Operator     | Node is an operator (binary/unary)                               | SHOULD be used for operator nodes |
| Index        | Node is an index (e.g., arr[0])                                  | SHOULD be used for index nodes |
| Key          | Node is a key in a key-value pair                                | SHOULD be used for key nodes |
| Value        | Node is a value in a key-value pair                              | SHOULD be used for value nodes |
| Type         | Node is a type annotation or reference                           | SHOULD be used for type nodes |
| Interface    | Node is an interface declaration                                 | SHOULD be used for interface nodes |
| Class        | Node is a class declaration                                      | SHOULD be used for class nodes |
| Struct       | Node is a struct/record declaration                              | SHOULD be used for struct nodes |
| Enum         | Node is an enum declaration                                      | SHOULD be used for enum nodes |
| Member       | Node is a member/field/property                                  | SHOULD be used for member nodes |
| Module       | Node is a module/package/namespace declaration                   | SHOULD be used for module nodes |
| Lambda       | Node is a lambda/anonymous function                              | SHOULD be used for lambda nodes |
| Try          | Node is a try/catch/finally block                                | SHOULD be used for try nodes |
| Catch        | Node is a catch block                                            | SHOULD be used for catch nodes |
| Finally      | Node is a finally block                                          | SHOULD be used for finally nodes |
| Throw        | Node is a throw/raise statement                                  | SHOULD be used for throw nodes |
| Await        | Node is an await expression                                      | SHOULD be used for await nodes |
| Generator    | Generator function                                               | SHOULD be used for generator functions |
| Yield        | Node is a yield expression                                       | SHOULD be used for yield nodes |
| Spread       | Node is a spread/rest element (e.g., ...args)                    | SHOULD be used for spread nodes |
| Pattern      | Node is a pattern in pattern matching                            | SHOULD be used for pattern nodes |
| Match        | Node is a match statement (e.g., Python 3.10+)                   | SHOULD be used for match nodes |
| Return       | Node is a return stmt                                            | SHOULD be used for return nodes |
| Break        | Node is a break stmt                                             | SHOULD be used for break nodes |
| Continue     | Node is a continue stmt                                          | SHOULD be used for continue nodes |

## 4. Serialization Formats

### 4.1 JSON Serialization

UAST nodes MUST be serializable to JSON format. The JSON serialization MUST include all non-empty fields and MUST omit nil/empty fields where idiomatic.

#### 4.1.1 JSON Schema

The JSON serialization MUST conform to a schema that validates the UAST node structure. The schema MUST enforce:

- Required fields: `type`, `children`
- Optional fields: `id`, `token`, `roles`, `pos`, `props`
- Field types and constraints as defined in Section 2

#### 4.1.2 JSON Example

```json
{
  "type": "Function",
  "roles": ["Function", "Declaration"],
  "props": {"name": "add"},
  "children": [
    {
      "type": "Identifier",
      "token": "add",
      "roles": ["Name"]
    }
  ]
}
```

### 4.2 Alternative Serialization Formats

Additional serialization formats MAY be defined for specific use cases:

- Protocol Buffers for performance and gRPC communication
- MessagePack for compact binary serialization
- YAML for human-readable configuration

## 5. Extensibility Guidelines

### 5.1 Adding New Types

New canonical types MAY be added to the specification. When adding new types:

1. The type MUST be language-agnostic.
2. The type MUST be clearly documented with its purpose and usage.
3. The type MUST be added to the canonical types table in Section 3.1.
4. Implementations SHOULD be updated to use the new type where applicable.

### 5.2 Adding New Roles

New canonical roles MAY be added to the specification. When adding new roles:

1. The role MUST be orthogonal to existing roles.
2. The role MUST be clearly documented with its purpose and usage.
3. The role MUST be added to the canonical roles table in Section 3.2.
4. Implementations SHOULD be updated to use the new role where applicable.

## 6. Implementation Requirements

### 6.1 Core Requirements

All UAST implementations MUST:

1. Generate nodes with stable, language-agnostic `type` fields.
2. Include all mapped roles in the `roles` field.
3. Include all mapped properties in the `props` field.
4. Preserve the order of children as in the source AST.
5. Support the JSON serialization format.
6. Validate input data to prevent security vulnerabilities.

### 6.2 Optional Requirements

UAST implementations SHOULD:

1. Include position information when available.
2. Generate stable IDs for diffing operations.
3. Support additional serialization formats as needed.
4. Provide comprehensive test coverage.
5. Document all public APIs and formats.

### 6.3 Performance Requirements

UAST implementations SHOULD:

1. Use efficient traversal algorithms.
2. Minimize memory allocations during conversion.
3. Support streaming processing for large files.
4. Provide caching mechanisms for repeated operations.

## 7. Examples

### 7.1 Function Declaration Example

Given source code that declares a function:

```javascript
function add(a, b) { return a + b; }
```

The UAST representation MUST include:

```json
{
  "type": "Function",
  "roles": ["Function", "Declaration"],
  "props": {"name": "add"},
  "children": [
    {
      "type": "Identifier",
      "token": "add",
      "roles": ["Name"]
    },
    {
      "type": "Parameter",
      "roles": ["Parameter"],
      "props": {"name": "a"},
      "children": [
        {
          "type": "Identifier",
          "token": "a",
          "roles": ["Name"]
        }
      ]
    },
    {
      "type": "Parameter",
      "roles": ["Parameter"],
      "props": {"name": "b"},
      "children": [
        {
          "type": "Identifier",
          "token": "b",
          "roles": ["Name"]
        }
      ]
    },
    {
      "type": "Block",
      "roles": ["Body"],
      "children": [
        {
          "type": "Return",
          "roles": ["Return"],
          "children": [
            {
              "type": "BinaryOp",
              "roles": ["Operator"],
              "props": {"operator": "+"},
              "children": [
                {
                  "type": "Identifier",
                  "token": "a",
                  "roles": ["Name"]
                },
                {
                  "type": "Identifier",
                  "token": "b",
                  "roles": ["Name"]
                }
              ]
            }
          ]
        }
      ]
    }
  ]
}
```

### 7.2 Class Declaration Example

Given source code that declares a class:

```python
class Calculator:
    def add(self, a, b):
        return a + b
```

The UAST representation MUST include:

- A `Class` node with `Class` and `Declaration` roles
- A `Method` node with `Function`, `Declaration`, and `Member` roles
- Proper parameter and body structure

## 8. Security Considerations

### 8.1 Input Validation

UAST implementations MUST validate all input data to prevent security vulnerabilities:

1. Source code input MUST be sanitized to prevent injection attacks.
2. Position information MUST be validated to prevent integer overflow.
3. Property values MUST be validated to prevent malicious content.
4. Node structures MUST be validated to prevent malformed trees.

### 8.2 Memory Safety

UAST implementations MUST ensure memory safety:

1. Memory allocations MUST be bounded to prevent denial of service.
2. Large files MUST be processed in streaming mode when possible.
3. Recursive algorithms MUST be avoided to prevent stack overflow.

### 8.3 Information Disclosure

UAST implementations SHOULD consider information disclosure risks:

1. Position information MAY reveal sensitive details about source code structure.
2. Token values MAY contain sensitive data (passwords, keys, etc.).
3. Property values MAY contain implementation-specific information.

## 9. IANA Considerations

This specification does not require any IANA registrations.

## 10. References

### 10.1 Normative References

- [RFC 2119](https://datatracker.ietf.org/doc/html/rfc2119): Key words for use in RFCs to Indicate Requirement Levels
- [RFC 8174](https://datatracker.ietf.org/doc/html/rfc8174): Ambiguity of Uppercase vs Lowercase in RFC 2119 Key Words

### 10.2 Informative References

- Abstract Syntax Tree: https://en.wikipedia.org/wiki/Abstract_syntax_tree
- Static Analysis: https://en.wikipedia.org/wiki/Static_program_analysis
- Code Refactoring: https://en.wikipedia.org/wiki/Code_refactoring

---

*This document is the authoritative specification for the UAST format. All implementations MUST conform to this specification.*
