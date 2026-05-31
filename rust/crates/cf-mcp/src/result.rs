//! Tool result helpers.
//!
//! Ports the Go `errorResult` / `jsonResult` helpers and the `ToolOutput`
//! wrapper from `internal/mcp/tools.go`.
//!
//! The MCP `CallToolResult` carries one or more `Content` items plus an
//! `IsError` flag. We model the subset the Go code uses: a list of text content
//! strings and the error flag. The concrete wire `CallToolResult` is built from
//! a [`ToolResult`] at the transport boundary in [`crate::transport`], keeping
//! the wire type out of the byte-identity-critical serialization path.

use crate::errors::ToolError;
use crate::gojson::{Encoder, JsonValue};

/// Structured side-channel output mirroring the Go `ToolOutput{ Data any }`.
///
/// The Go tool functions return this as the second element of their tuple; the
/// SDK exposes it as the call's structured content. We carry the same payload as
/// a [`JsonValue`] so it can be re-encoded byte-identically if needed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolOutput {
    /// The wrapped data value, or `None` for the empty `ToolOutput{}`.
    pub data: Option<JsonValue>,
}

impl ToolOutput {
    /// Empty output — the Go `ToolOutput{}` returned alongside error results.
    #[must_use]
    pub fn empty() -> Self {
        ToolOutput { data: None }
    }

    /// Output wrapping a data value — the Go `ToolOutput{Data: value}`.
    #[must_use]
    pub fn with_data(data: JsonValue) -> Self {
        ToolOutput { data: Some(data) }
    }
}

/// The text-content view of an MCP `CallToolResult`.
///
/// Mirrors the fields the Go handlers populate: a list of text strings (each Go
/// `*mcpsdk.TextContent`) and the `IsError` flag. Additional content (e.g. the
/// `trace_id=...` line appended by tracing) is appended via [`ToolResult::push_text`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResult {
    /// Ordered text-content items, each one Go `TextContent.Text`.
    pub content: Vec<String>,
    /// Whether this result represents an error (Go `CallToolResult.IsError`).
    pub is_error: bool,
}

impl ToolResult {
    /// Builds an error result containing a single text item with the error
    /// message, with `is_error = true`.
    ///
    /// Reproduces Go `errorResult(err)`.
    #[must_use]
    pub fn error(err: &ToolError) -> Self {
        ToolResult {
            content: vec![err.to_string()],
            is_error: true,
        }
    }

    /// Builds a success result whose single text item is the Go-compatible
    /// pretty JSON encoding of `value`.
    ///
    /// Reproduces Go `jsonResult(value)`:
    /// `json.MarshalIndent(value, "", "  ")` → indent two spaces, HTML escaping
    /// ON, **no** trailing newline. Routed through [`crate::gojson`], never
    /// `serde_json`, to preserve byte-identity. See `DESIGN.md` §2.3.
    #[must_use]
    pub fn json(value: &JsonValue) -> Self {
        let bytes = Encoder::indented("  ").encode(value);
        ToolResult {
            content: vec![String::from_utf8_lossy(&bytes).into_owned()],
            is_error: false,
        }
    }

    /// Appends a text-content item (used by tracing to add `trace_id=<id>`).
    ///
    /// Mirrors `result.Content = append(result.Content, &TextContent{...})`.
    pub fn push_text(&mut self, text: impl Into<String>) {
        self.content.push(text.into());
    }

    /// Returns the first text-content item, if any.
    ///
    /// Mirrors the Go test helper `extractText` and the
    /// `result.Content[0].(*TextContent).Text` accesses throughout the tests.
    #[must_use]
    pub fn first_text(&self) -> &str {
        self.content.first().map(String::as_str).unwrap_or("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_result_sets_is_error_and_message() {
        let res = ToolResult::error(&ToolError::EmptyCode);
        assert!(res.is_error);
        assert_eq!(res.content.len(), 1);
        assert!(res.first_text().contains("code parameter is required"));
    }

    #[test]
    fn json_result_is_not_error_and_pretty_prints() {
        let v = JsonValue::sorted_object(vec![("a".to_string(), JsonValue::Int(1))]);
        let res = ToolResult::json(&v);
        assert!(!res.is_error);
        // Two-space indent, no trailing newline (Go MarshalIndent).
        assert_eq!(res.first_text(), "{\n  \"a\": 1\n}");
    }

    #[test]
    fn json_result_html_escapes_like_go() {
        let v = JsonValue::sorted_object(vec![(
            "k".to_string(),
            JsonValue::Str("a<b>&c".to_string()),
        )]);
        let res = ToolResult::json(&v);
        assert!(res.first_text().contains("\\u003c"));
        assert!(res.first_text().contains("\\u003e"));
        assert!(res.first_text().contains("\\u0026"));
    }

    #[test]
    fn push_text_appends_trace_line() {
        let mut res = ToolResult::json(&JsonValue::Null);
        res.push_text("trace_id=abc123");
        assert_eq!(res.content.last().unwrap(), "trace_id=abc123");
    }
}
