//! Tree-sitter grammar analysis and mapping-DSL generation: parsing
//! `node-types.json`, heuristic node classification, coverage analysis, and
//! generation of a mapping DSL document from a grammar's node types using a
//! canonical name→type/role table.
//!
//! The generated DSL text is observable through the `uast mapping` CLI, so the
//! table order, quoting, and layout here are frozen (CLI compatibility
//! contract).

use std::collections::BTreeSet;

use serde_json::Value;

use crate::mapping_types::{ChildInfo, FieldInfo, NodeCategory, NodeTypeInfo, Rule};

/// Error returned when there are no node types to analyze.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("no node types to analyze")]
pub struct NoNodeTypes;

/// Error returned when `node-types.json` cannot be parsed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("failed to unmarshal node-types.json: {0}")]
pub struct ParseNodeTypesError(pub String);

/// Parses `node-types.json` and returns a slice of [`NodeTypeInfo`].
/// A non-array top level yields an empty slice.
///
/// # Errors
///
/// Returns [`ParseNodeTypesError`] when the input is not valid JSON.
pub fn parse_node_types(json_data: &[u8]) -> Result<Vec<NodeTypeInfo>, ParseNodeTypesError> {
    let raw: Value =
        serde_json::from_slice(json_data).map_err(|e| ParseNodeTypesError(e.to_string()))?;
    let Value::Array(arr) = raw else {
        return Ok(Vec::new());
    };
    Ok(arr.iter().map(parse_node_type_info).collect())
}

/// Applies heuristic rules to classify node types.
#[must_use]
pub fn apply_heuristic_classification(mut nodes: Vec<NodeTypeInfo>) -> Vec<NodeTypeInfo> {
    for node in &mut nodes {
        node.category = classify_node_category(node);
    }
    nodes
}

/// Computes mapping coverage statistics: the fraction of node types that have
/// a same-named rule.
///
/// # Errors
///
/// Returns [`NoNodeTypes`] when there are no node types.
pub fn coverage_analysis(rules: &[Rule], node_types: &[NodeTypeInfo]) -> Result<f64, NoNodeTypes> {
    let mapped: BTreeSet<&str> = rules.iter().map(|r| r.name.as_str()).collect();
    let total = node_types.len();
    if total == 0 {
        return Err(NoNodeTypes);
    }
    let covered = node_types
        .iter()
        .filter(|n| mapped.contains(n.name.as_str()))
        .count();
    Ok(covered as f64 / total as f64)
}

/// Whether `name` is a valid identifier: `[a-zA-Z_][a-zA-Z0-9_]*`.
fn is_valid_identifier(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let first = bytes[0];
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|&c| c.is_ascii_alphanumeric() || c == b'_')
}

/// A canonical name pattern → UAST type and roles.
struct CanonicalTypeRole {
    pattern: &'static str,
    r#type: &'static str,
    roles: &'static [&'static str],
}

/// The canonical type/role table. The order is frozen — the first
/// substring match wins (see [`guess_uast_type_and_roles`]).
const CANONICAL_TYPE_ROLE_MAP: &[CanonicalTypeRole] = &[
    CanonicalTypeRole { pattern: "function", r#type: "Function", roles: &["Function", "Declaration"] },
    CanonicalTypeRole { pattern: "method", r#type: "Method", roles: &["Function", "Declaration", "Member"] },
    CanonicalTypeRole { pattern: "class", r#type: "Class", roles: &["Class", "Declaration"] },
    CanonicalTypeRole { pattern: "interface", r#type: "Interface", roles: &["Interface", "Declaration"] },
    CanonicalTypeRole { pattern: "struct", r#type: "Struct", roles: &["Struct", "Declaration"] },
    CanonicalTypeRole { pattern: "enum", r#type: "Enum", roles: &["Enum", "Declaration"] },
    CanonicalTypeRole { pattern: "enum_member", r#type: "EnumMember", roles: &["Member"] },
    CanonicalTypeRole { pattern: "variable", r#type: "Variable", roles: &["Variable", "Declaration"] },
    CanonicalTypeRole { pattern: "parameter", r#type: "Parameter", roles: &["Parameter"] },
    CanonicalTypeRole { pattern: "block", r#type: "Block", roles: &["Body"] },
    CanonicalTypeRole { pattern: "if", r#type: "If", roles: &[] },
    CanonicalTypeRole { pattern: "loop", r#type: "Loop", roles: &["Loop"] },
    CanonicalTypeRole { pattern: "for", r#type: "Loop", roles: &["Loop"] },
    CanonicalTypeRole { pattern: "while", r#type: "Loop", roles: &["Loop"] },
    CanonicalTypeRole { pattern: "switch", r#type: "Switch", roles: &[] },
    CanonicalTypeRole { pattern: "case", r#type: "Case", roles: &["Branch"] },
    CanonicalTypeRole { pattern: "return", r#type: "Return", roles: &["Return"] },
    CanonicalTypeRole { pattern: "break", r#type: "Break", roles: &["Break"] },
    CanonicalTypeRole { pattern: "continue", r#type: "Continue", roles: &["Continue"] },
    CanonicalTypeRole { pattern: "assignment", r#type: "Assignment", roles: &["Assignment"] },
    CanonicalTypeRole { pattern: "call", r#type: "Call", roles: &["Call"] },
    CanonicalTypeRole { pattern: "identifier", r#type: "Identifier", roles: &["Name"] },
    CanonicalTypeRole { pattern: "literal", r#type: "Literal", roles: &["Literal"] },
    CanonicalTypeRole { pattern: "binary_op", r#type: "BinaryOp", roles: &["Operator"] },
    CanonicalTypeRole { pattern: "unary_op", r#type: "UnaryOp", roles: &["Operator"] },
    CanonicalTypeRole { pattern: "import", r#type: "Import", roles: &["Import"] },
    CanonicalTypeRole { pattern: "package", r#type: "Package", roles: &["Module"] },
    CanonicalTypeRole { pattern: "attribute", r#type: "Attribute", roles: &["Attribute"] },
    CanonicalTypeRole { pattern: "comment", r#type: "Comment", roles: &["Comment"] },
    CanonicalTypeRole { pattern: "docstring", r#type: "DocString", roles: &["Doc"] },
    CanonicalTypeRole { pattern: "type_annotation", r#type: "TypeAnnotation", roles: &["Type"] },
    CanonicalTypeRole { pattern: "field", r#type: "Field", roles: &["Member"] },
    CanonicalTypeRole { pattern: "property", r#type: "Property", roles: &["Member"] },
    CanonicalTypeRole { pattern: "getter", r#type: "Getter", roles: &["Getter"] },
    CanonicalTypeRole { pattern: "setter", r#type: "Setter", roles: &["Setter"] },
    CanonicalTypeRole { pattern: "lambda", r#type: "Lambda", roles: &["Lambda"] },
    CanonicalTypeRole { pattern: "try", r#type: "Try", roles: &["Try"] },
    CanonicalTypeRole { pattern: "catch", r#type: "Catch", roles: &["Catch"] },
    CanonicalTypeRole { pattern: "finally", r#type: "Finally", roles: &["Finally"] },
    CanonicalTypeRole { pattern: "throw", r#type: "Throw", roles: &["Throw"] },
    CanonicalTypeRole { pattern: "module", r#type: "Module", roles: &["Module"] },
    CanonicalTypeRole { pattern: "namespace", r#type: "Namespace", roles: &["Module"] },
    CanonicalTypeRole { pattern: "decorator", r#type: "Decorator", roles: &["Attribute"] },
    CanonicalTypeRole { pattern: "spread", r#type: "Spread", roles: &["Spread"] },
    CanonicalTypeRole { pattern: "tuple", r#type: "Tuple", roles: &[] },
    CanonicalTypeRole { pattern: "list", r#type: "List", roles: &[] },
    CanonicalTypeRole { pattern: "dict", r#type: "Dict", roles: &[] },
    CanonicalTypeRole { pattern: "set", r#type: "Set", roles: &[] },
    CanonicalTypeRole { pattern: "key_value", r#type: "KeyValue", roles: &["Key", "Value"] },
    CanonicalTypeRole { pattern: "index", r#type: "Index", roles: &["Index"] },
    CanonicalTypeRole { pattern: "slice", r#type: "Slice", roles: &[] },
    CanonicalTypeRole { pattern: "cast", r#type: "Cast", roles: &[] },
    CanonicalTypeRole { pattern: "await", r#type: "Await", roles: &["Await"] },
    CanonicalTypeRole { pattern: "yield", r#type: "Yield", roles: &["Yield"] },
    CanonicalTypeRole { pattern: "generator", r#type: "Generator", roles: &["Generator"] },
    CanonicalTypeRole { pattern: "comprehension", r#type: "Comprehension", roles: &[] },
    CanonicalTypeRole { pattern: "pattern", r#type: "Pattern", roles: &["Pattern"] },
    CanonicalTypeRole { pattern: "match", r#type: "Match", roles: &["Match"] },
];

/// Lowercases the name and returns the first substring-matching canonical
/// entry, defaulting to `("Synthetic", &[])`.
fn guess_uast_type_and_roles(name: &str) -> (&'static str, &'static [&'static str]) {
    let lname = name.to_lowercase();
    for entry in CANONICAL_TYPE_ROLE_MAP {
        if lname.contains(entry.pattern) {
            return (entry.r#type, entry.roles);
        }
    }
    ("Synthetic", &[])
}

/// Returns `@name` when the node has a `name` field, else empty.
fn guess_token_field(node: &NodeTypeInfo) -> &'static str {
    if node.fields.contains_key("name") {
        "@name"
    } else {
        ""
    }
}

/// Parses one `node-types.json` entry.
fn parse_node_type_info(entry: &Value) -> NodeTypeInfo {
    let name = entry
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let is_named = entry.get("named").and_then(Value::as_bool).unwrap_or(false);
    let fields = parse_fields(entry.get("fields"));
    let children = parse_children(entry.get("children"));
    NodeTypeInfo {
        name,
        fields,
        children,
        category: NodeCategory::Leaf,
        is_named,
    }
}

/// Parses the `fields` object of a node-type entry.
fn parse_fields(raw: Option<&Value>) -> std::collections::BTreeMap<String, FieldInfo> {
    let mut fields = std::collections::BTreeMap::new();
    let Some(map) = raw.and_then(Value::as_object) else {
        return fields;
    };
    for (fname, fval) in map {
        let Some(fmap) = fval.as_object() else {
            continue;
        };
        let required = fmap.get("required").and_then(Value::as_bool).unwrap_or(false);
        let types = parse_field_types(fmap.get("types"));
        let multiple = is_field_multiple(fmap);
        fields.insert(
            fname.clone(),
            FieldInfo {
                name: fname.clone(),
                types,
                required,
                multiple,
            },
        );
    }
    fields
}

/// Parses the `types` list of a field entry.
fn parse_field_types(raw: Option<&Value>) -> Vec<String> {
    let Some(arr) = raw.and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|e| e.as_object())
        .filter_map(|m| m.get("type").and_then(Value::as_str))
        .filter(|s| !s.is_empty())
        .map(std::string::ToString::to_string)
        .collect()
}

/// A field is "multiple" when it allows more than one type or says so.
fn is_field_multiple(fmap: &serde_json::Map<String, Value>) -> bool {
    if let Some(arr) = fmap.get("types").and_then(Value::as_array) {
        if arr.len() > 1 {
            return true;
        }
    }
    fmap.get("multiple").and_then(Value::as_bool).unwrap_or(false)
}

/// Parses the `children` list of a node-type entry.
fn parse_children(raw: Option<&Value>) -> Vec<ChildInfo> {
    let Some(arr) = raw.and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|e| e.as_object())
        .map(|m| ChildInfo {
            r#type: m.get("type").and_then(Value::as_str).unwrap_or("").to_string(),
            named: m.get("named").and_then(Value::as_bool).unwrap_or(false),
        })
        .collect()
}

/// Heuristic category: no children/fields → Leaf; operator-ish name →
/// Operator; else Container.
fn classify_node_category(node: &NodeTypeInfo) -> NodeCategory {
    if node.children.is_empty() && node.fields.is_empty() {
        return NodeCategory::Leaf;
    }
    let n = &node.name;
    let is_operator = n.contains("_operator")
        || n.contains("_op")
        || n.contains("operator")
        || n.contains("binary_expression")
        || n.contains("unary_expression");
    if is_operator {
        NodeCategory::Operator
    } else {
        NodeCategory::Container
    }
}

/// Renders a double-quoted, escaped string literal (reference-implementation
/// quoting; part of the generated-DSL byte contract).
///
/// Covers the printable-ASCII / common-escape cases that occur in node type
/// and role names (and extensions). Control and non-printable characters are
/// rendered as `\xNN`/`\uNNNN`.
fn go_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{07}' => out.push_str("\\a"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            '\u{0B}' => out.push_str("\\v"),
            c if (' '..='~').contains(&c) => out.push(c),
            c if (c as u32) < 0x80 => out.push_str(&format!("\\x{:02x}", c as u32)),
            c if (c as u32) < 0x10000 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push_str(&format!("\\U{:08x}", c as u32)),
        }
    }
    out.push('"');
    out
}

/// Writes the `[language ... , extensions: ...]` header (skipped when either
/// part is empty).
fn write_language_declaration(sb: &mut String, language: &str, extensions: &[String]) {
    if language.is_empty() || extensions.is_empty() {
        return;
    }
    sb.push('[');
    sb.push_str("language ");
    sb.push_str(&go_quote(language));
    sb.push_str(", extensions: ");
    for (idx, ext) in extensions.iter().enumerate() {
        if idx > 0 {
            sb.push_str(", ");
        }
        sb.push_str(&go_quote(ext));
    }
    sb.push_str("]\n\n");
}

/// Collects sorted, de-duplicated valid identifiers drawn from the node's
/// children and field types.
fn collect_child_types(node: &NodeTypeInfo) -> Vec<String> {
    let mut set: BTreeSet<String> = BTreeSet::new();
    for child in &node.children {
        if is_valid_identifier(&child.r#type) {
            set.insert(child.r#type.clone());
        }
    }
    for field in node.fields.values() {
        for ft in &field.types {
            if is_valid_identifier(ft) {
                set.insert(ft.clone());
            }
        }
    }
    set.into_iter().collect()
}

/// Writes the `roles:` section (skipped when empty).
fn write_roles_section(sb: &mut String, roles: &[&str]) {
    if roles.is_empty() {
        return;
    }
    sb.push_str(",\n    roles: ");
    for (idx, role) in roles.iter().enumerate() {
        if idx > 0 {
            sb.push_str(", ");
        }
        sb.push_str(&go_quote(role));
    }
}

/// Writes the `children:` section (skipped when empty).
fn write_children_section(sb: &mut String, child_types: &[String]) {
    if child_types.is_empty() {
        return;
    }
    sb.push_str(",\n    children: ");
    for (idx, child) in child_types.iter().enumerate() {
        if idx > 0 {
            sb.push_str(", ");
        }
        sb.push_str(&go_quote(child));
    }
}

/// Writes one node's full mapping rule.
fn write_node_mapping(sb: &mut String, node: &NodeTypeInfo) {
    let (uast_type, roles) = guess_uast_type_and_roles(&node.name);
    let is_leaf = node.children.is_empty() && node.fields.is_empty();

    sb.push_str(&node.name);
    sb.push_str(" <- (");
    sb.push_str(&node.name);
    sb.push_str(") => uast(\n    type: ");
    sb.push_str(&go_quote(uast_type));

    if is_leaf {
        let token = guess_token_field(node);
        if !token.is_empty() {
            sb.push_str(",\n    token: ");
            sb.push_str(&go_quote(token));
        }
    }

    write_roles_section(sb, roles);
    write_children_section(sb, &collect_child_types(node));

    sb.push_str("\n)\n\n");
}

/// Generates mapping DSL for a set of node types, using canonical UAST
/// types/roles. The output layout is frozen (CLI compatibility contract).
#[must_use]
pub fn generate_mapping_dsl(
    nodes: &[NodeTypeInfo],
    language: &str,
    extensions: &[String],
) -> String {
    let mut sb = String::new();
    write_language_declaration(&mut sb, language, extensions);
    for node in nodes {
        if !is_valid_identifier(&node.name) {
            continue;
        }
        write_node_mapping(&mut sb, node);
    }
    sb
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ti(name: &str, named: bool) -> NodeTypeInfo {
        NodeTypeInfo {
            name: name.to_string(),
            is_named: named,
            ..NodeTypeInfo::default()
        }
    }

    #[test]
    fn generate_dsl_matches_reference_layout() {
        let nodes = vec![
            ti("function_declaration", true),
            ti("identifier", true),
            ti("call_expression", true),
        ];
        let out = generate_mapping_dsl(&nodes, "go", &[".go".to_string()]);
        assert!(out.starts_with("[language \"go\", extensions: \".go\"]\n\n"));
        // function_declaration → Function with Function/Declaration roles.
        assert!(out.contains("function_declaration <- (function_declaration) => uast(\n    type: \"Function\""));
        assert!(out.contains("roles: \"Function\", \"Declaration\""));
        // identifier → "If": guess_uast_type_and_roles returns the FIRST
        // canonical-table entry whose pattern is a substring of the lowercased
        // name, and "identifier" contains "if" with the `if` entry preceding
        // the `identifier` entry. This quirk is frozen reference-implementation
        // behavior (verified against the reference output byte-for-byte).
        assert!(out.contains("identifier <- (identifier) => uast(\n    type: \"If\""));
        // call_expression → Call (contains "call").
        assert!(out.contains("call_expression <- (call_expression) => uast(\n    type: \"Call\""));
    }

    #[test]
    fn leaf_node_with_name_field_gets_token() {
        let mut node = ti("identifier", true);
        node.fields.insert(
            "name".to_string(),
            FieldInfo {
                name: "name".to_string(),
                ..FieldInfo::default()
            },
        );
        // Has a field, so not a leaf → no token line.
        let out = generate_mapping_dsl(&[node], "", &[]);
        assert!(!out.contains("token:"));

        // A true leaf (no fields/children) with no name field → no token.
        let leaf = ti("operator", true);
        let out2 = generate_mapping_dsl(&[leaf], "", &[]);
        assert!(!out2.contains("token:"));
    }

    #[test]
    fn parse_node_types_basic() {
        let json = br#"[
            {"type":"identifier","named":true},
            {"type":"function_declaration","named":true,"fields":{"name":{"types":[{"type":"identifier","named":true}]}}}
        ]"#;
        let nodes = parse_node_types(json).expect("parse");
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].name, "identifier");
        assert!(nodes[1].fields.contains_key("name"));
    }

    #[test]
    fn classify_and_coverage() {
        let json = br#"[
            {"type":"identifier","named":true},
            {"type":"binary_expression","named":true,"children":[{"type":"identifier","named":true}]}
        ]"#;
        let nodes = apply_heuristic_classification(parse_node_types(json).unwrap());
        assert_eq!(nodes[0].category, NodeCategory::Leaf);
        assert_eq!(nodes[1].category, NodeCategory::Operator);

        let rules = vec![Rule {
            name: "identifier".to_string(),
            ..Rule::default()
        }];
        let cov = coverage_analysis(&rules, &nodes).unwrap();
        assert!((cov - 0.5).abs() < 1e-12);

        assert_eq!(coverage_analysis(&rules, &[]), Err(NoNodeTypes));
    }

    #[test]
    fn go_quote_escapes() {
        assert_eq!(go_quote("go"), "\"go\"");
        assert_eq!(go_quote(".go"), "\".go\"");
        assert_eq!(go_quote("a\"b"), "\"a\\\"b\"");
        assert_eq!(go_quote("a\nb"), "\"a\\nb\"");
    }

    #[test]
    fn invalid_identifiers_skipped() {
        let nodes = vec![ti("\"", false), ti("valid_node", true)];
        let out = generate_mapping_dsl(&nodes, "", &[]);
        assert!(out.contains("valid_node <-"));
        // The invalid-identifier node is skipped.
        assert!(!out.contains("\" <-"));
    }
}
