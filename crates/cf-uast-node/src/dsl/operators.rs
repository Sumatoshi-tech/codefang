//! DSL operators: builtin functions and value comparison.

use crate::comparison::sort_nodes;
use crate::node::Node;
use crate::types::{DslLiteral, DslNode};

/// Applies a map operation: runs `f` on each node singly and concatenates.
pub fn apply_map(nodes: &[Node], f: &dyn Fn(&[Node]) -> Vec<Node>) -> Vec<Node> {
    let mut result = Vec::with_capacity(nodes.len());
    for n in nodes {
        result.extend(f(std::slice::from_ref(n)));
    }
    result
}

/// Applies a filter operation.
pub fn apply_filter(nodes: &[Node], predicate: &dyn Fn(&Node) -> bool) -> Vec<Node> {
    nodes.iter().filter(|n| predicate(n)).cloned().collect()
}

/// Applies a reduce operation (just runs `f` over the whole slice).
pub fn apply_reduce(nodes: &[Node], f: &dyn Fn(&[Node]) -> Vec<Node>) -> Vec<Node> {
    f(nodes)
}

/// Extracts a string value from a node by field name: `type`/`token`/`id` are
/// built-ins, else a prop lookup.
pub fn extract_string_value(n: &Node, field: &str) -> String {
    match field {
        "type" => n.node_type.clone(),
        "token" => n.token.clone(),
        "id" => String::from_utf8_lossy(&n.id).into_owned(),
        _ => n.props.get(field).cloned().unwrap_or_default(),
    }
}

/// Compares two strings with the given operator (lexicographic ordering).
pub fn compare_values(left: &str, right: &str, op: &str) -> bool {
    match op {
        "==" => left == right,
        "!=" => left != right,
        "<" => left < right,
        "<=" => left <= right,
        ">" => left > right,
        ">=" => left >= right,
        _ => false,
    }
}

/// Sorts nodes by a field.
pub fn sort_by_field(nodes: &[Node], field: &str, ascending: bool) -> Vec<Node> {
    let mut sorted = nodes.to_vec();
    sorted.sort_by(|a, b| {
        let l = extract_string_value(a, field);
        let r = extract_string_value(b, field);
        if ascending {
            l.cmp(&r)
        } else {
            r.cmp(&l)
        }
    });
    sorted
}

/// Applies a builtin function by name. Unknown names pass the input through
/// unchanged.
pub fn apply_builtin(name: &str, nodes: &[Node], args: &[DslNode]) -> Vec<Node> {
    match name {
        "contains" => apply_contains(nodes, args),
        "startsWith" => apply_starts_with(nodes, args),
        "endsWith" => apply_ends_with(nodes, args),
        "has" => apply_has(nodes, args),
        "count" => apply_count(nodes),
        "first" => apply_first(nodes),
        "last" => apply_last(nodes),
        "sort" => apply_sort(nodes, args),
        _ => nodes.to_vec(),
    }
}

/// Extracts the string value of the first literal argument, if any.
fn first_literal_arg(args: &[DslNode]) -> Option<String> {
    args.iter().find_map(|a| match a {
        DslNode::Literal(DslLiteral::Str(s) | DslLiteral::Number(s)) => Some(s.clone()),
        DslNode::Literal(DslLiteral::Bool(b)) => Some(b.to_string()),
        _ => None,
    })
}

fn apply_contains(nodes: &[Node], args: &[DslNode]) -> Vec<Node> {
    let needle = first_literal_arg(args).unwrap_or_default();
    nodes
        .iter()
        .filter(|n| n.token.contains(&needle))
        .cloned()
        .collect()
}

fn apply_starts_with(nodes: &[Node], args: &[DslNode]) -> Vec<Node> {
    let prefix = first_literal_arg(args).unwrap_or_default();
    nodes
        .iter()
        .filter(|n| n.token.starts_with(&prefix))
        .cloned()
        .collect()
}

fn apply_ends_with(nodes: &[Node], args: &[DslNode]) -> Vec<Node> {
    let suffix = first_literal_arg(args).unwrap_or_default();
    nodes
        .iter()
        .filter(|n| n.token.ends_with(&suffix))
        .cloned()
        .collect()
}

/// `has(field)` keeps nodes that have a non-empty value for `field`.
fn apply_has(nodes: &[Node], args: &[DslNode]) -> Vec<Node> {
    let field = match args.first() {
        Some(DslNode::Field(fields)) if !fields.is_empty() => fields[0].clone(),
        Some(DslNode::Literal(DslLiteral::Str(s))) => s.clone(),
        _ => return nodes.to_vec(),
    };
    nodes
        .iter()
        .filter(|n| !extract_string_value(n, &field).is_empty())
        .cloned()
        .collect()
}

/// `count()` collapses the set to a single literal node holding the count.
fn apply_count(nodes: &[Node]) -> Vec<Node> {
    vec![Node::literal(nodes.len().to_string())]
}

fn apply_first(nodes: &[Node]) -> Vec<Node> {
    nodes.first().cloned().into_iter().collect()
}

fn apply_last(nodes: &[Node]) -> Vec<Node> {
    nodes.last().cloned().into_iter().collect()
}

/// `sort()` sorts by type then token; `sort(.field)` sorts by that field.
fn apply_sort(nodes: &[Node], args: &[DslNode]) -> Vec<Node> {
    if let Some(DslNode::Field(fields)) = args.first() {
        if let Some(field) = fields.first() {
            return sort_by_field(nodes, field, true);
        }
    }
    let mut out = nodes.to_vec();
    sort_nodes(&mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DslLiteral;

    fn nodes() -> Vec<Node> {
        vec![
            Node::with_token("Function", "alpha"),
            Node::with_token("Function", "beta"),
            Node::with_token("Variable", "gamma"),
        ]
    }

    #[test]
    fn compare_values_ops() {
        assert!(compare_values("a", "a", "=="));
        assert!(compare_values("a", "b", "!="));
        assert!(compare_values("a", "b", "<"));
        assert!(!compare_values("a", "b", ">"));
        assert!(!compare_values("a", "b", "??"));
    }

    #[test]
    fn extract_string_value_builtins_and_props() {
        let mut n = Node::with_token("Function", "foo");
        n.props.insert("k".into(), "v".into());
        assert_eq!(extract_string_value(&n, "type"), "Function");
        assert_eq!(extract_string_value(&n, "token"), "foo");
        assert_eq!(extract_string_value(&n, "k"), "v");
        assert_eq!(extract_string_value(&n, "missing"), "");
    }

    #[test]
    fn contains_starts_ends() {
        let arg = vec![DslNode::Literal(DslLiteral::Str("al".into()))];
        assert_eq!(apply_builtin("contains", &nodes(), &arg).len(), 1);
        assert_eq!(apply_builtin("startsWith", &nodes(), &arg).len(), 1);
        let end = vec![DslNode::Literal(DslLiteral::Str("ma".into()))];
        assert_eq!(apply_builtin("endsWith", &nodes(), &end).len(), 1); // gamma
    }

    #[test]
    fn count_first_last() {
        assert_eq!(apply_builtin("count", &nodes(), &[])[0].token, "3");
        assert_eq!(apply_builtin("first", &nodes(), &[])[0].token, "alpha");
        assert_eq!(apply_builtin("last", &nodes(), &[])[0].token, "gamma");
    }

    #[test]
    fn sort_builtin() {
        let mut unsorted = vec![
            Node::with_token("Function", "b"),
            Node::with_token("Class", "z"),
        ];
        unsorted.reverse();
        let out = apply_builtin("sort", &unsorted, &[]);
        assert_eq!(out[0].node_type, "Class");
    }

    #[test]
    fn unknown_builtin_passes_through() {
        assert_eq!(apply_builtin("nope", &nodes(), &[]).len(), 3);
    }
}
