//! Data types for the comments analyzer.
//!
//! Report key names are produced explicitly in [`crate::analyzer`] (routed
//! through `cf-gojson`); the key strings are part of the report contract.

use std::collections::BTreeMap;

/// UAST node-type strings used by the comments analyzer.
///
/// The string values are the contract; [`cf_uast_node::Type`] is a string
/// alias carrying exactly these.
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
    /// Block node type (a declaration body carries the `Body` role).
    pub const BLOCK: &str = "Block";
    /// Parameter node type (part of a declaration header).
    pub const PARAMETER: &str = "Parameter";
    /// Pattern node type (a return-type / parameter pattern in the header).
    pub const PATTERN: &str = "Pattern";
    /// Type-annotation node type (part of a declaration header).
    pub const TYPE_ANNOTATION: &str = "TypeAnnotation";
    /// Decorator node type (part of a declaration header).
    pub const DECORATOR: &str = "Decorator";
    /// Attribute node type (part of a declaration header).
    pub const ATTRIBUTE: &str = "Attribute";
}

/// The `Name` role string.
pub const ROLE_NAME: &str = "Name";
/// The `Type` role string: an identifier carrying it names a type, never the
/// declaration it sits in (C return types, Go receiver types).
pub const ROLE_TYPE: &str = "Type";
/// The `Attribute` role string: attribute / decorator / annotation lines
/// that belong to the declaration they precede.
pub const ROLE_ATTRIBUTE: &str = "Attribute";
/// The `Doc` role string: a documentation comment (`///`, `/** */`, a
/// Python docstring).
pub const ROLE_DOC: &str = "Doc";
/// The `Body` role string: the block holding a declaration's statements.
pub const ROLE_BODY: &str = "Body";
/// The `Parameter` role string (declaration header).
pub const ROLE_PARAMETER: &str = "Parameter";

/// Name reported for a declaration whose name cannot be resolved.
pub const UNKNOWN_NAME: &str = "unknown";

/// Where a comment block sits relative to the declarations of its file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// The block ends on the line directly above a declaration (attribute
    /// lines included): it documents that declaration.
    Above,
    /// The block is a documentation comment that is the first statement of
    /// a declaration body (a Python docstring): it documents that
    /// declaration.
    Docstring,
    /// The block lies inside a declaration's span: a body comment.
    Inline,
    /// The block is attached to no declaration.
    Unassociated,
}

impl Placement {
    /// The report string for this placement.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Placement::Above => "above",
            Placement::Docstring => "docstring",
            Placement::Inline => "inline",
            Placement::Unassociated => "unassociated",
        }
    }
}

/// Per-analysis configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct CommentConfig {
    /// Score awarded to a comment that documents a declaration.
    pub reward_score: f64,
    /// Penalty for an inline comment, keyed by the enclosing declaration's
    /// node type.
    pub penalty_scores: BTreeMap<String, f64>,
}

impl CommentConfig {
    /// Returns the default configuration (report contract):
    /// `reward_score = 1.0`, penalties `Function/Method -0.5`,
    /// `Class/Interface/Struct -0.3`, `Variable/Assignment/Call -0.1`.
    ///
    /// ```
    /// use cf_comments::CommentConfig;
    /// let cfg = CommentConfig::default_config();
    /// assert_eq!(cfg.reward_score, 1.0);
    /// assert_eq!(cfg.penalty_scores.get("Function"), Some(&-0.5));
    /// assert_eq!(cfg.penalty_scores.get("Class"), Some(&-0.3));
    /// assert_eq!(cfg.penalty_scores.get("Variable"), Some(&-0.1));
    /// ```
    #[must_use]
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
            penalty_scores,
        }
    }
}

/// Per-comment analysis detail (one row per comment node).
#[derive(Debug, Clone, PartialEq)]
pub struct CommentDetail {
    /// Comment text (the node's token, or its nested comment tokens).
    pub text: String,
    /// 1-based line the comment starts on.
    pub line: i64,
    /// Placement relative to the file's declarations.
    pub placement: Placement,
    /// Quality score (reward when `above`, a penalty otherwise).
    pub score: f64,
    /// Type of the declaration the comment is attached to (empty when
    /// unassociated).
    pub target_type: String,
    /// Name of the declaration the comment is attached to (empty when
    /// unassociated).
    pub target_name: String,
}

/// Assessment of a comment that documents a declaration.
pub const COMMENT_OK: &str = "✅ OK";
/// Assessment of a comment that documents nothing.
pub const COMMENT_NOT_OK: &str = "❌ Not OK";
/// Assessment of a documented declaration.
pub const FUNCTION_DOCUMENTED: &str = "✅ Well Documented";
/// Assessment of an undocumented declaration.
pub const FUNCTION_UNDOCUMENTED: &str = "❌ No Comment";

impl CommentDetail {
    /// Whether the comment documents a declaration.
    #[must_use]
    pub fn is_good(&self) -> bool {
        matches!(self.placement, Placement::Above | Placement::Docstring)
    }

    /// The table assessment string.
    #[must_use]
    pub fn assessment(&self) -> &'static str {
        if self.is_good() {
            COMMENT_OK
        } else {
            COMMENT_NOT_OK
        }
    }
}

/// Documentation status of a single declaration (function, method, class,
/// interface or struct).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionInfo {
    /// Declaration name (`unknown` when unresolved).
    pub name: String,
    /// Declaration node type.
    pub kind: String,
    /// 1-based line of the declaration.
    pub line: i64,
    /// Number of source lines the declaration spans.
    pub lines: i64,
    /// Line of the comment block documenting it; `None` when undocumented.
    pub doc_line: Option<i64>,
}

impl FunctionInfo {
    /// Whether a comment block documents this declaration.
    #[must_use]
    pub fn is_documented(&self) -> bool {
        self.doc_line.is_some()
    }

    /// The table assessment string.
    #[must_use]
    pub fn assessment(&self) -> &'static str {
        if self.is_documented() {
            FUNCTION_DOCUMENTED
        } else {
            FUNCTION_UNDOCUMENTED
        }
    }
}

/// The five counters every comments report is built from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CommentCounts {
    /// Number of comment nodes.
    pub total_comments: i64,
    /// Comments that document a declaration.
    pub good_comments: i64,
    /// Comments that do not (`inline` or `unassociated`).
    pub bad_comments: i64,
    /// Number of declarations.
    pub total_functions: i64,
    /// Declarations with a documenting comment.
    pub documented_functions: i64,
}

/// Quality-message thresholds over the good-comment ratio.
const EXCELLENT_MIN: f64 = 0.8;
const GOOD_MIN: f64 = 0.6;
const FAIR_MIN: f64 = 0.4;

/// Message for a report with neither comments nor declarations.
pub const NO_COMMENTS_MESSAGE: &str = "No comments found";

impl CommentCounts {
    /// Good-comment ratio (`good / total`), 0 when there are no comments.
    #[must_use]
    pub fn overall_score(&self) -> f64 {
        safe_div(self.good_comments as f64, self.total_comments as f64)
    }

    /// Documentation coverage (`documented / total`), 0 when there are no
    /// declarations.
    #[must_use]
    pub fn documentation_coverage(&self) -> f64 {
        safe_div(
            self.documented_functions as f64,
            self.total_functions as f64,
        )
    }

    /// The quality message: the threshold label of
    /// [`overall_score`](Self::overall_score) (`>= 0.8` excellent, `>= 0.6`
    /// good, `>= 0.4` fair, else poor), or [`NO_COMMENTS_MESSAGE`] when the
    /// report has neither comments nor declarations.
    #[must_use]
    pub fn message(&self) -> &'static str {
        if self.total_comments == 0 && self.total_functions == 0 {
            return NO_COMMENTS_MESSAGE;
        }
        quality_message(self.overall_score())
    }

    /// Adds another report's counters to these.
    pub fn add(&mut self, other: &CommentCounts) {
        self.total_comments += other.total_comments;
        self.good_comments += other.good_comments;
        self.bad_comments += other.bad_comments;
        self.total_functions += other.total_functions;
        self.documented_functions += other.documented_functions;
    }
}

/// The threshold label of a good-comment ratio.
#[must_use]
pub fn quality_message(score: f64) -> &'static str {
    if score >= EXCELLENT_MIN {
        "Excellent comment quality and placement"
    } else if score >= GOOD_MIN {
        "Good comment quality with room for improvement"
    } else if score >= FAIR_MIN {
        "Fair comment quality - consider improving placement"
    } else {
        "Poor comment quality - significant improvement needed"
    }
}

/// `a / b`, or 0 when `b` is 0.
#[must_use]
pub fn safe_div(a: f64, b: f64) -> f64 {
    if b == 0.0 {
        0.0
    } else {
        a / b
    }
}

/// One file's analysis: counters plus the per-comment and per-declaration
/// rows they were computed from.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommentMetrics {
    /// The counters.
    pub counts: CommentCounts,
    /// One row per comment node, in line order.
    pub comments: Vec<CommentDetail>,
    /// One row per declaration, in document order.
    pub functions: Vec<FunctionInfo>,
}
