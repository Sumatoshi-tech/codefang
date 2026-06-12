//! `uast_parse` tool handler.
//!
//! Validates the inline code, parses it into a UAST, optionally filters nodes
//! by type, and returns the (possibly filtered) root as report-compatible
//! pretty JSON.

use cf_uast_node::Node;

use crate::errors::ToolError;
use crate::gojson::node_to_json;
use crate::providers::UastParser;
use crate::result::{ToolOutput, ToolResult};
use crate::tools::{synthetic_filename, validate_code_input, UastParseInput};

/// The synthetic node type used to wrap query matches.
pub const FILTERED_RESULTS_TYPE: &str = "filtered_results";

/// Processes a `uast_parse` tool call.
///
/// The step order is observable through the returned errors, so keep it:
/// 1. validate the code input.
/// 2. unsupported language → error.
/// 3. parse (wrapped `parse code: <err>`).
/// 4. if `query` non-empty, replace the root with a `filtered_results` node
///    holding every node whose type equals the query.
/// 5. return the root as pretty JSON.
#[must_use]
pub fn handle_uast_parse(parser: &dyn UastParser, input: &UastParseInput) -> (ToolResult, ToolOutput) {
    if let Err(err) = validate_code_input(&input.code, &input.language) {
        return (ToolResult::error(&err), ToolOutput::empty());
    }

    let filename = synthetic_filename(&input.language);

    if !parser.is_supported(&filename) {
        let err = ToolError::UnsupportedLanguage {
            language: input.language.clone(),
        };
        return (ToolResult::error(&err), ToolOutput::empty());
    }

    let mut root = match parser.parse(&filename, input.code.as_bytes()) {
        Ok(root) => root,
        Err(e) => {
            let err = ToolError::wrap("parse code", e.to_string());
            return (ToolResult::error(&err), ToolOutput::empty());
        }
    };

    if !input.query.is_empty() {
        root = filter_nodes_by_type(&root, &input.query);
    }

    let value = node_to_json(&root);
    (ToolResult::json(&value), ToolOutput::with_data(value))
}

/// Builds a filtered tree containing only nodes whose type equals `node_type`.
///
/// Returns a node of type `"filtered_results"` whose children are the matches
/// (in document order; a matched node is not descended into — see
/// [`collect_matching_nodes`]).
#[must_use]
pub fn filter_nodes_by_type(root: &Node, node_type: &str) -> Node {
    let mut matches: Vec<Node> = Vec::new();
    collect_matching_nodes(root, node_type, &mut matches);

    let mut filtered = Node::with_token(FILTERED_RESULTS_TYPE, "");
    filtered.children = matches;
    filtered
}

/// Walks the tree collecting nodes whose type equals `node_type`.
///
/// When a node matches, it is collected and its subtree is **not** descended
/// (tool contract: an outer match shadows nested matches); otherwise it
/// recurses into the children in order.
pub fn collect_matching_nodes(current: &Node, node_type: &str, matches: &mut Vec<Node>) {
    if current.node_type == node_type {
        matches.push(current.clone());
        return;
    }
    for child in &current.children {
        collect_matching_nodes(child, node_type, matches);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::MAX_CODE_INPUT_BYTES;
    use cf_uast_node::Builder;

    /// Parser double that builds a small tree for the WithQuery test and accepts
    /// only `code.go`.
    struct FakeParser;
    impl UastParser for FakeParser {
        fn is_supported(&self, filename: &str) -> bool {
            filename == "code.go"
        }
        fn parse(&self, _filename: &str, _code: &[u8]) -> Result<Node, ToolError> {
            // Root "Package" with two "Function" children carrying tokens.
            let mut root = Node::with_token("Package", "");
            root.children = vec![
                Node::with_token("Function", "hello"),
                Node::with_token("Function", "world"),
            ];
            Ok(root)
        }
    }

    #[test]
    fn valid_go_code_returns_package_and_function() {
        let input = UastParseInput {
            code: "package main\nfunc main() {}\n".into(),
            language: "go".into(),
            query: String::new(),
        };
        let (res, _) = handle_uast_parse(&FakeParser, &input);
        assert!(!res.is_error);
        assert!(res.first_text().contains("Function"));
        assert!(res.first_text().contains("Package"));
    }

    #[test]
    fn empty_code_is_error() {
        let input = UastParseInput {
            code: String::new(),
            language: "go".into(),
            query: String::new(),
        };
        let (res, _) = handle_uast_parse(&FakeParser, &input);
        assert!(res.is_error);
        assert!(res.first_text().contains("code parameter is required"));
    }

    #[test]
    fn empty_language_is_error() {
        let input = UastParseInput {
            code: "package main".into(),
            language: String::new(),
            query: String::new(),
        };
        let (res, _) = handle_uast_parse(&FakeParser, &input);
        assert!(res.is_error);
        assert!(res.first_text().contains("language parameter is required"));
    }

    #[test]
    fn unsupported_language_is_error() {
        let input = UastParseInput {
            code: "some code".into(),
            language: "brainfuck".into(),
            query: String::new(),
        };
        let (res, _) = handle_uast_parse(&FakeParser, &input);
        assert!(res.is_error);
        assert!(res.first_text().contains("unsupported language"));
    }

    #[test]
    fn code_too_large_is_error() {
        let input = UastParseInput {
            code: "a".repeat(MAX_CODE_INPUT_BYTES + 1),
            language: "go".into(),
            query: String::new(),
        };
        let (res, _) = handle_uast_parse(&FakeParser, &input);
        assert!(res.is_error);
        assert!(res.first_text().contains("exceeds maximum size"));
    }

    #[test]
    fn with_query_filters_functions() {
        let input = UastParseInput {
            code: "package main\nfunc hello() {}\nfunc world() {}\n".into(),
            language: "go".into(),
            query: "Function".into(),
        };
        let (res, _) = handle_uast_parse(&FakeParser, &input);
        assert!(!res.is_error);
        let text = res.first_text();
        assert!(text.contains("Function"));
        assert!(text.contains("hello"));
        assert!(text.contains("world"));
    }

    #[test]
    fn matched_node_is_not_descended() {
        // A Function containing a nested Function must yield ONE match (the
        // outer), because collect_matching_nodes returns on a match.
        let mut outer = Node::with_token("Function", "outer");
        outer.children = vec![Node::with_token("Function", "inner")];
        let mut root = Node::with_token("Package", "");
        root.children = vec![outer];

        let filtered = filter_nodes_by_type(&root, "Function");
        assert_eq!(filtered.node_type, "filtered_results");
        assert_eq!(filtered.children.len(), 1);
        assert_eq!(filtered.children[0].token, "outer");
    }

    #[test]
    fn filtered_results_wrapper_type() {
        let root = Builder::new().with_type("File").build();
        let filtered = filter_nodes_by_type(&root, "Nothing");
        assert_eq!(filtered.node_type, "filtered_results");
        assert!(filtered.children.is_empty());
    }
}
