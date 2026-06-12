//! Tool name constants, input schemas, limits, and shared validation.
//!
//! The three tool-name constants, the [`MAX_CODE_INPUT_BYTES`] limit, the
//! input structs (`AnalyzeInput`, `HistoryInput`, `UastParseInput`) with their
//! JSON field names, the shared code-input validation, and the
//! synthetic-filename helper.

use serde::Deserialize;

use crate::errors::ToolError;

/// Tool name for static analysis.
pub const TOOL_NAME_ANALYZE: &str = "codefang_analyze";
/// Tool name for Git history analysis.
pub const TOOL_NAME_HISTORY: &str = "codefang_history";
/// Tool name for UAST parsing.
pub const TOOL_NAME_UAST: &str = "uast_parse";

/// Maximum allowed size for inline code input (1 MiB).
pub const MAX_CODE_INPUT_BYTES: usize = 1 << 20;

/// Input schema for the `codefang_analyze` tool.
///
/// JSON field names are part of the tool schema contract.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AnalyzeInput {
    /// Optional list of analyzer names to run (default: all).
    #[serde(default)]
    pub analyzers: Vec<String>,
    /// Source code to analyze.
    #[serde(default)]
    pub code: String,
    /// Programming language (e.g. go python javascript).
    #[serde(default)]
    pub language: String,
}

/// Input schema for the `codefang_history` tool.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct HistoryInput {
    /// Optional list of history analyzers (default: all).
    #[serde(default)]
    pub analyzers: Vec<String>,
    /// Follow only the first parent of merge commits.
    #[serde(default)]
    pub first_parent: bool,
    /// Maximum number of commits to analyze (default: 1000).
    #[serde(default)]
    pub limit: i64,
    /// Absolute path to a Git repository.
    #[serde(default)]
    pub repo_path: String,
    /// Only analyze commits after this time (e.g. 24h or 2024-01-01).
    #[serde(default)]
    pub since: String,
}

/// Input schema for the `uast_parse` tool.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UastParseInput {
    /// Source code to parse into UAST.
    #[serde(default)]
    pub code: String,
    /// Programming language (e.g. go python javascript).
    #[serde(default)]
    pub language: String,
    /// Optional node type filter (e.g. function_declaration).
    #[serde(default)]
    pub query: String,
}

/// Validates the common code-input constraints shared by `codefang_analyze` and
/// `uast_parse`.
///
/// Empty code → [`ToolError::EmptyCode`], empty language →
/// [`ToolError::EmptyLanguage`], oversized code → [`ToolError::CodeTooLarge`].
/// The size compared is the **byte** length of the code, not the character
/// count.
///
/// # Errors
/// Returns the first failing constraint as a [`ToolError`].
pub fn validate_code_input(code: &str, language: &str) -> Result<(), ToolError> {
    if code.is_empty() {
        return Err(ToolError::EmptyCode);
    }
    if language.is_empty() {
        return Err(ToolError::EmptyLanguage);
    }
    if code.len() > MAX_CODE_INPUT_BYTES {
        return Err(ToolError::CodeTooLarge {
            size: code.len(),
            max: MAX_CODE_INPUT_BYTES,
        });
    }
    Ok(())
}

/// Builds the synthetic filename `"code.<language>"` so the parser can
/// dispatch by extension.
#[must_use]
pub fn synthetic_filename(language: &str) -> String {
    format!("code.{language}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_names_match_contract() {
        assert_eq!(TOOL_NAME_ANALYZE, "codefang_analyze");
        assert_eq!(TOOL_NAME_HISTORY, "codefang_history");
        assert_eq!(TOOL_NAME_UAST, "uast_parse");
    }

    #[test]
    fn max_code_input_bytes_is_one_mib() {
        assert_eq!(MAX_CODE_INPUT_BYTES, 1_048_576);
    }

    #[test]
    fn validate_rejects_empty_code() {
        assert_eq!(validate_code_input("", "go"), Err(ToolError::EmptyCode));
    }

    #[test]
    fn validate_rejects_empty_language() {
        assert_eq!(
            validate_code_input("package main", ""),
            Err(ToolError::EmptyLanguage)
        );
    }

    #[test]
    fn validate_rejects_oversized_code() {
        let big = "a".repeat(MAX_CODE_INPUT_BYTES + 1);
        match validate_code_input(&big, "go") {
            Err(ToolError::CodeTooLarge { size, max }) => {
                assert_eq!(size, MAX_CODE_INPUT_BYTES + 1);
                assert_eq!(max, MAX_CODE_INPUT_BYTES);
            }
            other => panic!("expected CodeTooLarge, got {other:?}"),
        }
    }

    #[test]
    fn validate_accepts_valid_input() {
        assert!(validate_code_input("package main", "go").is_ok());
    }

    #[test]
    fn validate_uses_byte_length_not_char_count() {
        // 'é' is 2 bytes in UTF-8; a string just over MAX bytes but under MAX
        // chars must still be rejected (the limit is the byte length).
        let s = "é".repeat(MAX_CODE_INPUT_BYTES / 2 + 1);
        assert!(s.chars().count() <= MAX_CODE_INPUT_BYTES);
        assert!(matches!(
            validate_code_input(&s, "go"),
            Err(ToolError::CodeTooLarge { .. })
        ));
    }

    #[test]
    fn synthetic_filename_prefixes_code_dot() {
        assert_eq!(synthetic_filename("go"), "code.go");
        assert_eq!(synthetic_filename("python"), "code.python");
    }

    #[test]
    fn analyze_input_deserializes_schema_field_names() {
        let json = r#"{"code":"x","language":"go","analyzers":["complexity"]}"#;
        let input: AnalyzeInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.code, "x");
        assert_eq!(input.language, "go");
        assert_eq!(input.analyzers, vec!["complexity"]);
    }

    #[test]
    fn history_input_deserializes_first_parent_and_limit() {
        let json = r#"{"repo_path":"/r","first_parent":true,"limit":5,"since":"24h"}"#;
        let input: HistoryInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.repo_path, "/r");
        assert!(input.first_parent);
        assert_eq!(input.limit, 5);
        assert_eq!(input.since, "24h");
    }

    #[test]
    fn uast_input_deserializes_query() {
        let json = r#"{"code":"x","language":"go","query":"Function"}"#;
        let input: UastParseInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.query, "Function");
    }
}
