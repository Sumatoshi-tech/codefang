//! Static mapping-DSL completion items, UAST field items, and hover docs.
//!
//! Port of the package-level `var` block in Go `pkg/uast/lsp/server.go`:
//! `mappingDSLKeywords`, `uastFields`, `hoverDocs`, and the `completionItem`
//! constructor. The label/detail/doc strings are reproduced byte-for-byte so the
//! LSP responses match the Go server.

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind};

/// Builds a [`CompletionItem`] with a label, kind, and detail.
///
/// Equivalent to Go `completionItem(label, kind, detail)`: sets `Label`, and the
/// optional `Kind`/`Detail` fields, leaving everything else at its default.
#[must_use]
pub fn completion_item(label: &str, kind: CompletionItemKind, detail: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(kind),
        detail: Some(detail.to_string()),
        ..CompletionItem::default()
    }
}

/// Mapping-DSL keyword completion items.
///
/// Mirrors Go `mappingDSLKeywords` (same order, labels, kinds, and details).
#[must_use]
pub fn mapping_dsl_keywords() -> Vec<CompletionItem> {
    vec![
        completion_item("<-", CompletionItemKind::KEYWORD, "Pattern assignment"),
        completion_item("=>", CompletionItemKind::KEYWORD, "UAST mapping assignment"),
        completion_item("uast", CompletionItemKind::KEYWORD, "UAST specification block"),
    ]
}

/// UAST field completion items.
///
/// Mirrors Go `uastFields` (same order, labels, kinds, and details).
#[must_use]
pub fn uast_fields() -> Vec<CompletionItem> {
    vec![
        completion_item("type", CompletionItemKind::FIELD, "UAST node type (string)"),
        completion_item("token", CompletionItemKind::FIELD, "Token/capture for node label"),
        completion_item("roles", CompletionItemKind::FIELD, "UAST node roles (list)"),
        completion_item("props", CompletionItemKind::FIELD, "UAST node properties (map)"),
        completion_item(
            "children",
            CompletionItemKind::FIELD,
            "UAST children (list of captures)",
        ),
    ]
}

/// All completion items offered by `textDocument/completion`: keywords followed
/// by UAST fields, exactly as Go's `completion` handler concatenates them.
#[must_use]
pub fn all_completions() -> Vec<CompletionItem> {
    let mut items = mapping_dsl_keywords();
    items.extend(uast_fields());
    items
}

/// Returns the hover documentation for a DSL keyword/field, if any.
///
/// Mirrors a lookup into Go's `hoverDocs` map. The match arms reproduce the map
/// keys and Markdown values byte-for-byte.
#[must_use]
pub fn hover_doc(word: &str) -> Option<&'static str> {
    match word {
        "<-" => Some("Assigns a pattern to a rule name. Example: `rule <- (pattern)`."),
        "=>" => Some("Assigns a UAST mapping to a pattern. Example: `(pattern) => uast(...)`."),
        "uast" => Some("Begins a UAST specification block for mapping output."),
        "type" => Some("UAST node type. Example: `type: \"Function\"`."),
        "token" => Some("Token or capture used as the node label. Example: `token: \"@name\"`."),
        "roles" => Some("List of UAST roles for this node. Example: `roles: \"Declaration\"`."),
        "props" => {
            Some("Map of additional node properties. Example: `props: [\"receiver\": \"true\"]`.")
        }
        "children" => Some("List of child captures for this node. Example: `children: [\"@body\"]`."),
        _ => None,
    }
}

/// The complete set of hover-doc keys, for parity assertions and discovery.
///
/// Order matches the literal listing in Go's `hoverDocs` map declaration; the Go
/// map itself is unordered, so callers must not depend on this order for output.
pub const HOVER_DOC_KEYS: [&str; 8] =
    ["<-", "=>", "uast", "type", "token", "roles", "props", "children"];

#[cfg(test)]
mod tests {
    use super::*;

    /// Ported from Go `TestCompletionItem`.
    #[test]
    fn test_completion_item() {
        let item = completion_item("test", CompletionItemKind::KEYWORD, "Test detail");
        assert_eq!(item.label, "test", "Expected label \"test\"");
        assert_eq!(
            item.kind,
            Some(CompletionItemKind::KEYWORD),
            "Expected CompletionItemKind::KEYWORD"
        );
        assert_eq!(
            item.detail.as_deref(),
            Some("Test detail"),
            "Expected detail \"Test detail\""
        );
    }

    /// Ported from Go `TestMappingDSLKeywords`.
    #[test]
    fn test_mapping_dsl_keywords() {
        let keywords = mapping_dsl_keywords();
        assert!(!keywords.is_empty(), "Expected mapping DSL keywords to be defined");

        for expected in ["<-", "=>", "uast"] {
            assert!(
                keywords.iter().any(|i| i.label == expected),
                "Expected keyword {expected:?} not found in mapping_dsl_keywords"
            );
        }
    }

    /// Ported from Go `TestUastFields`.
    #[test]
    fn test_uast_fields() {
        let fields = uast_fields();
        assert!(!fields.is_empty(), "Expected UAST fields to be defined");

        for expected in ["type", "token", "roles", "props", "children"] {
            assert!(
                fields.iter().any(|i| i.label == expected),
                "Expected field {expected:?} not found in uast_fields"
            );
        }
    }

    /// Ported from Go `TestHoverDocs`.
    #[test]
    fn test_hover_docs() {
        // Every documented key resolves and is non-empty.
        for key in HOVER_DOC_KEYS {
            let doc = hover_doc(key);
            assert!(doc.is_some(), "Expected hover doc for {key:?} not found");
            assert!(!doc.unwrap().is_empty(), "Hover doc for {key:?} is empty");
        }
        // Unknown words resolve to None (Go: comma-ok false).
        assert!(hover_doc("unknown").is_none());
    }

    /// `all_completions` is keywords followed by fields, matching the Go handler.
    #[test]
    fn test_all_completions_order() {
        let got: Vec<String> = all_completions().into_iter().map(|i| i.label).collect();
        let expected: Vec<String> = mapping_dsl_keywords()
            .into_iter()
            .chain(uast_fields())
            .map(|i| i.label)
            .collect();
        assert_eq!(got, expected);
    }
}
