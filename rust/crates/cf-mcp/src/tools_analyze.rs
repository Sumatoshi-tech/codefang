//! `codefang_analyze` tool handler.
//!
//! Ports `internal/mcp/tools_analyze.go`. Validates the inline code, parses it
//! into a UAST, runs the selected (or all default) static analyzers, and returns
//! the result map as Go-compatible pretty JSON.
//!
//! The concrete UAST parser and analyzer factory are taken behind the
//! [`UastParser`] / [`StaticAnalysisProvider`] traits (see [`crate::providers`]);
//! the handler logic, ordering, default-analyzer set, and error wording are
//! fully ported here.

use crate::errors::ToolError;
use crate::providers::{StaticAnalysisProvider, UastParser};
use crate::result::{ToolOutput, ToolResult};
use crate::tools::{synthetic_filename, validate_code_input, AnalyzeInput};

/// Names of the default static analyzers, in the exact order the Go
/// `defaultStaticAnalyzers()` constructs them: `complexity, comments, halstead,
/// cohesion, imports`. Go derives the name list from each `Analyzer.Name()`.
pub const DEFAULT_STATIC_ANALYZER_NAMES: &[&str] =
    &["complexity", "comments", "halstead", "cohesion", "imports"];

/// Returns the default static-analyzer name list (Go `allStaticAnalyzerNames`).
#[must_use]
pub fn all_static_analyzer_names() -> Vec<String> {
    DEFAULT_STATIC_ANALYZER_NAMES
        .iter()
        .map(|s| (*s).to_string())
        .collect()
}

/// Processes a `codefang_analyze` tool call.
///
/// Reproduces Go `handleAnalyze` step-for-step:
/// 1. `validateCodeInput` → error result on failure.
/// 2. unsupported language → `unsupported language: <lang>`.
/// 3. parse the code (wrapped `parse code: <err>` on failure).
/// 4. default to all analyzer names when none requested.
/// 5. run analyzers (wrapped `run analyzers: <err>` on failure).
/// 6. return the result map as pretty JSON.
#[must_use]
pub fn handle_analyze(
    parser: &dyn UastParser,
    provider: &dyn StaticAnalysisProvider,
    input: &AnalyzeInput,
) -> (ToolResult, ToolOutput) {
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

    let root = match parser.parse(&filename, input.code.as_bytes()) {
        Ok(root) => root,
        Err(e) => {
            let err = ToolError::wrap("parse code", e.to_string());
            return (ToolResult::error(&err), ToolOutput::empty());
        }
    };

    let names = if input.analyzers.is_empty() {
        all_static_analyzer_names()
    } else {
        input.analyzers.clone()
    };

    let results = match provider.run(&root, &names) {
        Ok(v) => v,
        Err(e) => {
            let err = ToolError::wrap("run analyzers", e.to_string());
            return (ToolResult::error(&err), ToolOutput::empty());
        }
    };

    (ToolResult::json(&results), ToolOutput::with_data(results))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gojson::JsonValue;
    use crate::tools::MAX_CODE_INPUT_BYTES;
    use cf_uast_node::Node;

    /// Test double whose `is_supported` accepts only `code.go`.
    struct FakeParser;
    impl UastParser for FakeParser {
        fn is_supported(&self, filename: &str) -> bool {
            filename == "code.go"
        }
        fn parse(&self, _filename: &str, _code: &[u8]) -> Result<Node, ToolError> {
            Ok(Node::with_token("Package", ""))
        }
    }

    /// Test double returning a map containing a `complexity` key, so JSON output
    /// contains the substring the Go tests assert on.
    struct FakeProvider;
    impl StaticAnalysisProvider for FakeProvider {
        fn run(&self, _root: &Node, names: &[String]) -> Result<JsonValue, ToolError> {
            let entries = names
                .iter()
                .map(|n| (n.clone(), JsonValue::Int(1)))
                .collect();
            Ok(JsonValue::sorted_object(entries))
        }
    }

    #[test]
    fn empty_code_is_error() {
        let input = AnalyzeInput {
            code: String::new(),
            language: "go".into(),
            ..Default::default()
        };
        let (res, _) = handle_analyze(&FakeParser, &FakeProvider, &input);
        assert!(res.is_error);
        assert!(res.first_text().contains("code parameter is required"));
    }

    #[test]
    fn empty_language_is_error() {
        let input = AnalyzeInput {
            code: "package main".into(),
            language: String::new(),
            ..Default::default()
        };
        let (res, _) = handle_analyze(&FakeParser, &FakeProvider, &input);
        assert!(res.is_error);
        assert!(res.first_text().contains("language parameter is required"));
    }

    #[test]
    fn unsupported_language_is_error() {
        let input = AnalyzeInput {
            code: "some code".into(),
            language: "brainfuck".into(),
            ..Default::default()
        };
        let (res, _) = handle_analyze(&FakeParser, &FakeProvider, &input);
        assert!(res.is_error);
        assert!(res.first_text().contains("unsupported language"));
    }

    #[test]
    fn code_too_large_is_error() {
        let input = AnalyzeInput {
            code: "a".repeat(MAX_CODE_INPUT_BYTES + 1),
            language: "go".into(),
            ..Default::default()
        };
        let (res, _) = handle_analyze(&FakeParser, &FakeProvider, &input);
        assert!(res.is_error);
        assert!(res.first_text().contains("exceeds maximum size"));
    }

    #[test]
    fn valid_go_code_runs_all_analyzers() {
        let input = AnalyzeInput {
            code: "package main\nfunc main() {}\n".into(),
            language: "go".into(),
            ..Default::default()
        };
        let (res, out) = handle_analyze(&FakeParser, &FakeProvider, &input);
        assert!(!res.is_error);
        assert!(res.first_text().contains("complexity"));
        assert!(out.data.is_some());
    }

    #[test]
    fn selected_analyzers_only() {
        let input = AnalyzeInput {
            code: "package main\nfunc main() {}\n".into(),
            language: "go".into(),
            analyzers: vec!["complexity".into()],
        };
        let (res, _) = handle_analyze(&FakeParser, &FakeProvider, &input);
        assert!(!res.is_error);
        assert!(res.first_text().contains("complexity"));
    }

    #[test]
    fn default_names_match_go_order() {
        assert_eq!(
            DEFAULT_STATIC_ANALYZER_NAMES,
            &["complexity", "comments", "halstead", "cohesion", "imports"]
        );
    }

    #[test]
    fn parse_failure_is_wrapped() {
        struct FailingParser;
        impl UastParser for FailingParser {
            fn is_supported(&self, _f: &str) -> bool {
                true
            }
            fn parse(&self, _f: &str, _c: &[u8]) -> Result<Node, ToolError> {
                Err(ToolError::wrap("boom", "inner"))
            }
        }
        let input = AnalyzeInput {
            code: "x".into(),
            language: "go".into(),
            ..Default::default()
        };
        let (res, _) = handle_analyze(&FailingParser, &FakeProvider, &input);
        assert!(res.is_error);
        assert!(res.first_text().starts_with("parse code: "));
    }
}
