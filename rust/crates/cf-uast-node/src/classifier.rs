//! Node classification helpers. Ported from `classifier.go`.

use crate::node::uast_types::UAST_LITERAL;

/// Returns `true` if the node type represents a literal value. Mirrors Go's
/// `IsLiteralType`.
pub fn is_literal_type(node_type: &str) -> bool {
    node_type == UAST_LITERAL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_type_detection() {
        assert!(is_literal_type("Literal"));
        assert!(!is_literal_type("Function"));
    }
}
