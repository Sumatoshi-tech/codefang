//! Node classification helpers. Ported from `classifier.go`.

use crate::node::{uast_types::UAST_LITERAL, Node};

/// Returns `true` if the node type represents a literal value. Mirrors Go's
/// `IsLiteralType`.
pub fn is_literal_type(node_type: &str) -> bool {
    node_type == UAST_LITERAL
}

/// Numeric classification of a node by its token. Mirrors `classifyNodeNumeric`:
/// `0` for empty/nil, `1` for an all-ASCII-digit token, `2` otherwise.
pub(crate) fn classify_node_numeric(node: Option<&Node>) -> i32 {
    match node {
        None => 0,
        Some(n) => classify_token(&n.token),
    }
}

/// Numeric code for a token. Mirrors `classifyToken`.
pub(crate) fn classify_token(token: &str) -> i32 {
    if token.is_empty() {
        return 0;
    }
    if is_numeric_token(token) {
        return 1;
    }
    2
}

/// Reports whether the token is composed solely of ASCII digits `0`–`9`.
/// Mirrors `isNumericToken` (which iterates runes and checks `'0'..='9'`).
pub(crate) fn is_numeric_token(token: &str) -> bool {
    token.chars().all(|c| ('0'..='9').contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_type_detection() {
        assert!(is_literal_type("Literal"));
        assert!(!is_literal_type("Function"));
    }

    #[test]
    fn token_classification() {
        assert_eq!(classify_token(""), 0);
        assert_eq!(classify_token("123"), 1);
        assert_eq!(classify_token("abc"), 2);
        assert_eq!(classify_token("12a"), 2);
    }

    #[test]
    fn numeric_token_check() {
        assert!(is_numeric_token("007"));
        assert!(!is_numeric_token("0x7"));
        assert!(is_numeric_token(""));
    }

    #[test]
    fn classify_node_numeric_handles_none() {
        assert_eq!(classify_node_numeric(None), 0);
        let n = Node::with_token("Literal", "42");
        assert_eq!(classify_node_numeric(Some(&n)), 1);
    }
}
