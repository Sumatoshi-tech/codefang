//! Data types for the comments analyzer, ported from
//! `internal/analyzers/comments/types.go`.
//!
//! Field names mirror the Go structs. Report key names are produced explicitly
//! in [`crate::analyzer`] (routed through `cf-gojson`) so they match the Go
//! `Report = map[string]any` keys byte-for-byte.

use std::collections::BTreeMap;

use cf_uast_node::Node;

/// UAST node-type strings used by the comments analyzer.
///
/// These mirror the Go `node.UAST*` string constants. The string values are the
/// contract; [`cf_uast_node::Type`] is a string newtype carrying exactly these.
pub mod uast {
    /// Comment node type.
    pub const COMMENT: &str = "Comment";
    /// Function node type.
    pub const FUNCTION: &str = "Function";
    /// Method node type.
    pub const METHOD: &str = "Method";
    /// Class node type.
    pub const CLASS: &str = "Class";
    /// Interface node type.
    pub const INTERFACE: &str = "Interface";
    /// Struct node type.
    pub const STRUCT: &str = "Struct";
    /// Variable node type.
    pub const VARIABLE: &str = "Variable";
    /// Identifier node type (used for name extraction).
    pub const IDENTIFIER: &str = "Identifier";
}

/// The `Name` role string (mirrors Go `node.RoleName`).
pub const ROLE_NAME: &str = "Name";

/// Per-analysis configuration (Go `CommentConfig`).
#[derive(Debug, Clone, PartialEq)]
pub struct CommentConfig {
    /// Score awarded to a well-placed comment.
    pub reward_score: f64,
    /// Maximum comment length (informational; Go field `MaxCommentLength`).
    pub max_comment_length: i64,
    /// Penalty scores keyed by target node type.
    pub penalty_scores: BTreeMap<String, f64>,
}

impl CommentConfig {
    /// Returns the default configuration, matching Go `(*Analyzer).DefaultConfig`
    /// (`comments.go::getDefaultPenaltyScores`).
    ///
    /// Asserted by the ported `TestAnalyzer_DefaultConfig`: `reward_score = 1.0`,
    /// `max_comment_length = 500`, penalties `Function/Method -0.5`,
    /// `Class/Interface/Struct -0.3`, `Variable/Assignment/Call -0.1`.
    pub fn default_config() -> Self {
        let mut penalty_scores = BTreeMap::new();
        penalty_scores.insert(uast::FUNCTION.to_string(), -0.5);
        penalty_scores.insert(uast::METHOD.to_string(), -0.5);
        penalty_scores.insert(uast::CLASS.to_string(), -0.3);
        penalty_scores.insert(uast::INTERFACE.to_string(), -0.3);
        penalty_scores.insert(uast::STRUCT.to_string(), -0.3);
        penalty_scores.insert(uast::VARIABLE.to_string(), -0.1);
        penalty_scores.insert("Assignment".to_string(), -0.1);
        penalty_scores.insert("Call".to_string(), -0.1);
        CommentConfig {
            reward_score: 1.0,
            max_comment_length: 500,
            penalty_scores,
        }
    }
}

/// A contiguous block of comment nodes (Go `CommentBlock`).
#[derive(Debug, Clone)]
pub struct CommentBlock {
    /// The comment nodes composing this block, in line order.
    pub comments: Vec<Node>,
    /// 1-based start line of the first comment.
    pub start_line: i64,
    /// 1-based end line of the last comment.
    pub end_line: i64,
    /// Concatenated comment text (tokens joined with `"\n"`).
    pub full_text: String,
}

/// Per-comment analysis detail (Go `CommentDetail`).
#[derive(Debug, Clone, PartialEq)]
pub struct CommentDetail {
    /// Node type of the comment.
    pub kind: String,
    /// Comment token text.
    pub token: String,
    /// Quality score.
    pub score: f64,
    /// Whether the comment is considered good.
    pub is_good: bool,
    /// Target node type, if any.
    pub target_type: String,
    /// Target name, if any.
    pub target_name: String,
    /// Relative position (`above`/`below`/`inline`/`unassociated`/`unknown`).
    pub position: String,
    /// 1-based line number of the comment.
    pub line_number: i64,
}

/// Documentation status for a single function (Go `FunctionInfo`).
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionInfo {
    /// Function name.
    pub name: String,
    /// Function node type.
    pub kind: String,
    /// Whether the function has an associated good comment.
    pub has_comment: bool,
    /// The comment type, when documented.
    pub comment_type: String,
}

/// Aggregate metrics for a file (Go `CommentMetrics`).
#[derive(Debug, Clone)]
pub struct CommentMetrics {
    /// Total number of comment details.
    pub total_comments: i64,
    /// Count of good comments.
    pub good_comments: i64,
    /// Count of bad comments.
    pub bad_comments: i64,
    /// Overall quality score (`good / total`).
    pub overall_score: f64,
    /// All comment details.
    pub comment_details: Vec<CommentDetail>,
    /// Function name -> documentation info.
    pub function_summary: BTreeMap<String, FunctionInfo>,
    /// Total number of functions.
    pub total_functions: i64,
    /// Number of documented functions.
    pub documented_functions: i64,
}

/// A row of the detailed comments table (Go `CommentReportItem`).
#[derive(Debug, Clone, PartialEq)]
pub struct CommentReportItem {
    /// Comment line number.
    pub line: i64,
    /// (Truncated) comment text.
    pub comment: String,
    /// Placement string.
    pub placement: String,
    /// Target name.
    pub target: String,
    /// Assessment string.
    pub assessment: String,
}

/// A row of the detailed functions table (Go `FunctionReportItem`).
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionReportItem {
    /// Function name.
    pub function: String,
    /// Function type.
    pub kind: String,
    /// Function line count.
    pub lines: i64,
    /// Comment type / "None".
    pub comment: String,
    /// Assessment string.
    pub assessment: String,
}
