//! The comments analyzer.
//!
//! Finds comment nodes and declarations (function / method / class /
//! interface / struct), groups adjacent comments into blocks, places every
//! block against the declarations ([`crate::placement`]) and derives the
//! per-file metrics: a declaration is documented iff a block is placed
//! `above` it; a comment is good iff its block is placed `above` something.
//!
//! Report building is routed through [`cf_gojson::GoValue`] using a
//! *map-origin* [`cf_gojson::GoMap`] so the encoder byte-sorts keys at encode
//! time. Nothing here uses `serde_json`.

use cf_gojson::{GoMap, GoValue, MapOrigin};
use cf_uast_node::Node;

use crate::placement::{anchor_line, resolve, BlockSpan, Resolved, TargetSpan};
use crate::traverse::{
    comment_lines, comment_text, declaration_name, find_nodes_with_role, has_role, node_lines,
};
use crate::types::{
    uast, CommentConfig, CommentCounts, CommentDetail, CommentMetrics, FunctionInfo, Placement,
    ROLE_ATTRIBUTE, ROLE_BODY, ROLE_DOC, ROLE_NAME, ROLE_PARAMETER, ROLE_TYPE, UNKNOWN_NAME,
};

/// Score of a comment attached to no declaration.
const UNASSOCIATED_PENALTY: f64 = -0.2;
/// Inline penalty for a declaration type without a configured penalty.
const DEFAULT_INLINE_PENALTY: f64 = -0.1;
/// Comment previews longer than this many bytes are shortened.
const PREVIEW_MAX_BYTES: usize = 50;
/// Bytes kept in a shortened preview before the ellipsis.
const PREVIEW_KEEP_BYTES: usize = 47;

/// Node types counted as declarations.
const DECLARATION_TYPES: [&str; 5] = [
    uast::FUNCTION,
    uast::METHOD,
    uast::CLASS,
    uast::INTERFACE,
    uast::STRUCT,
];
/// Declaration types that only count when they carry a name: a nameless
/// class/struct/interface node is a type literal or a class body, not a
/// declaration.
const NAMED_ONLY_TYPES: [&str; 3] = [uast::CLASS, uast::INTERFACE, uast::STRUCT];
/// Node types that form a declaration's header (everything before the first
/// body statement): a `Doc` comment preceded only by these is a docstring.
const HEADER_TYPES: [&str; 7] = [
    uast::IDENTIFIER,
    uast::PARAMETER,
    uast::PATTERN,
    uast::TYPE_ANNOTATION,
    uast::DECORATOR,
    uast::ATTRIBUTE,
    uast::COMMENT,
];

/// Error returned when analysis is given no root. The error text is part of
/// the CLI contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NilRootNode;

impl std::fmt::Display for NilRootNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "root node is nil")
    }
}

impl std::error::Error for NilRootNode {}

/// The comments analyzer.
#[derive(Debug, Clone, Copy, Default)]
pub struct Analyzer;

impl Analyzer {
    /// Creates a new analyzer.
    #[must_use]
    pub fn new() -> Self {
        Analyzer
    }

    /// Returns the analyzer name (`"comments"`).
    #[must_use]
    pub fn name(&self) -> &'static str {
        "comments"
    }

    /// Returns the CLI flag (`"comments-analysis"`).
    #[must_use]
    pub fn flag(&self) -> &'static str {
        "comments-analysis"
    }

    /// Returns the default analysis configuration.
    #[must_use]
    pub fn default_config(&self) -> CommentConfig {
        CommentConfig::default_config()
    }

    /// Performs comment analysis and returns the per-file report map.
    ///
    /// A file with declarations but no comments keeps its declarations (all
    /// undocumented):
    ///
    /// ```
    /// use cf_comments::Analyzer;
    /// use cf_gojson::GoValue;
    /// use cf_uast_node::{Builder, Positions};
    ///
    /// let func = Builder::new()
    ///     .with_type("Function")
    ///     .with_props([("name".to_string(), "f".to_string())].into())
    ///     .with_position(Some(Positions { start_line: 1, end_line: 3, ..Default::default() }))
    ///     .build();
    /// let mut root = Builder::new().with_type("File").build();
    /// root.add_child(func);
    /// let report = Analyzer::new().analyze(Some(&root)).unwrap();
    /// let m = report.as_map().unwrap();
    /// assert_eq!(m.get("total_comments"), Some(&GoValue::Int(0)));
    /// assert_eq!(m.get("total_functions"), Some(&GoValue::Int(1)));
    /// assert_eq!(m.get("documented_functions"), Some(&GoValue::Int(0)));
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`NilRootNode`] when `root` is `None`.
    pub fn analyze(&self, root: Option<&Node>) -> Result<GoValue, NilRootNode> {
        self.analyze_with_find_depth(root, None)
    }

    /// [`analyze`](Self::analyze) with a depth cap on the discovery walks:
    /// with `Some(cap)`, only nodes at depth `<= cap` below the root (root at
    /// depth 0) are found. The history `quality` analyzer's comments instance
    /// runs capped; the static surfaces run uncapped.
    ///
    /// # Errors
    ///
    /// Returns [`NilRootNode`] when `root` is `None`.
    pub fn analyze_with_find_depth(
        &self,
        root: Option<&Node>,
        find_depth: Option<usize>,
    ) -> Result<GoValue, NilRootNode> {
        Ok(build_report(&self.analyze_metrics(root, find_depth)?))
    }

    /// Performs comment analysis and returns the typed per-file metrics.
    ///
    /// # Errors
    ///
    /// Returns [`NilRootNode`] when `root` is `None`.
    pub fn analyze_metrics(
        &self,
        root: Option<&Node>,
        find_depth: Option<usize>,
    ) -> Result<CommentMetrics, NilRootNode> {
        let root = root.ok_or(NilRootNode)?;
        let declarations = collect_declarations(root, find_depth);
        let blocks = collect_comment_blocks(root, find_depth);
        Ok(compute_metrics(
            &blocks,
            declarations,
            &self.default_config(),
        ))
    }
}

// --- declarations -------------------------------------------------------------

/// A declaration found in the tree.
struct Declaration {
    name: String,
    kind: String,
    start: i64,
    end: i64,
    anchor: i64,
}

fn collect_declarations(root: &Node, find_depth: Option<usize>) -> Vec<Declaration> {
    let attributes: Vec<(i64, i64)> = find_nodes_with_role(root, ROLE_ATTRIBUTE, find_depth)
        .into_iter()
        .filter_map(node_lines)
        .collect();
    let mut out = Vec::new();
    walk_declarations(root, find_depth, None, &mut out);
    for d in &mut out {
        d.anchor = anchor_line(d.start, &attributes);
        if d.name.is_empty() {
            d.name = UNKNOWN_NAME.to_string();
        }
    }
    out
}

fn walk_declarations(
    n: &Node,
    remaining: Option<usize>,
    enclosing: Option<usize>,
    out: &mut Vec<Declaration>,
) {
    let mut enclosing = enclosing;
    if DECLARATION_TYPES.contains(&n.node_type.as_str()) {
        enclosing = register_declaration(n, enclosing, out);
    }
    let child_remaining = match remaining {
        Some(0) => return,
        Some(r) => Some(r - 1),
        None => None,
    };
    for child in &n.children {
        walk_declarations(child, child_remaining, enclosing, out);
    }
}

/// Registers a declaration node and returns the index of the declaration
/// that encloses its children.
fn register_declaration(
    n: &Node,
    enclosing: Option<usize>,
    out: &mut Vec<Declaration>,
) -> Option<usize> {
    let Some((start, end)) = node_lines(n) else {
        return enclosing;
    };
    let name = declaration_name(n);
    // A nameless declaration whose child declaration starts on the same line
    // is that child's definition (C lowers `function_definition` over
    // `function_declarator`): the child names it and is no declaration of
    // its own.
    if let Some(parent) = enclosing.map(|i| &mut out[i]) {
        if parent.start == start && parent.name.is_empty() {
            parent.name = name.unwrap_or_default();
            return enclosing;
        }
    }
    if name.is_none() && NAMED_ONLY_TYPES.contains(&n.node_type.as_str()) {
        return enclosing;
    }
    out.push(Declaration {
        name: name.unwrap_or_default(),
        kind: n.node_type.clone(),
        start,
        end,
        anchor: start,
    });
    Some(out.len() - 1)
}

// --- comment blocks -----------------------------------------------------------

/// Adjacent comment nodes, each `(text, start_line)`.
struct Block {
    start: i64,
    text_end: i64,
    /// Whether the block opens with a leading documentation comment of a
    /// declaration body (a docstring).
    leading_doc: bool,
    comments: Vec<(String, i64)>,
}

/// One comment node: its line geometry, text and whether it is a leading
/// documentation comment (the first statement of a declaration body).
struct FoundComment {
    start: i64,
    text_end: i64,
    text: String,
    leading_doc: bool,
}

fn collect_comment_blocks(root: &Node, find_depth: Option<usize>) -> Vec<Block> {
    let mut comments = Vec::new();
    collect_comments(root, find_depth, &mut comments);
    comments.sort_by_key(|c| c.start);

    let mut blocks: Vec<Block> = Vec::new();
    for c in comments {
        match blocks.last_mut() {
            Some(cur) if c.start <= cur.text_end + 1 => {
                cur.text_end = cur.text_end.max(c.text_end);
                cur.comments.push((c.text, c.start));
            }
            _ => blocks.push(Block {
                start: c.start,
                text_end: c.text_end,
                leading_doc: c.leading_doc,
                comments: vec![(c.text, c.start)],
            }),
        }
    }
    blocks
}

/// Collects every `Comment` node below `n` (never descending into a comment:
/// nested pieces are one comment), honoring the same depth cap as
/// [`crate::traverse::find_nodes_by_type`].
fn collect_comments(n: &Node, remaining: Option<usize>, out: &mut Vec<FoundComment>) {
    let child_remaining = match remaining {
        Some(0) => return,
        Some(r) => Some(r - 1),
        None => None,
    };
    for (index, child) in n.children.iter().enumerate() {
        if child.node_type == uast::COMMENT {
            if let Some((start, text_end)) = comment_lines(child) {
                out.push(FoundComment {
                    start,
                    text_end,
                    text: comment_text(child),
                    leading_doc: has_role(child, ROLE_DOC) && is_leading_position(n, index),
                });
            }
            continue;
        }
        collect_comments(child, child_remaining, out);
    }
}

/// Whether child `index` of `parent` is the first statement of a declaration
/// body: the first child of a `Body` block, or a direct child of a
/// declaration preceded only by header nodes (name, parameters, types,
/// decorators, other comments).
fn is_leading_position(parent: &Node, index: usize) -> bool {
    if parent.node_type == uast::BLOCK && has_role(parent, ROLE_BODY) {
        return index == 0;
    }
    DECLARATION_TYPES.contains(&parent.node_type.as_str())
        && parent.children[..index].iter().all(is_header_node)
}

fn is_header_node(n: &Node) -> bool {
    HEADER_TYPES.contains(&n.node_type.as_str())
        || [ROLE_NAME, ROLE_PARAMETER, ROLE_TYPE, ROLE_ATTRIBUTE]
            .iter()
            .any(|role| has_role(n, role))
}

// --- metrics ------------------------------------------------------------------

fn compute_metrics(
    blocks: &[Block],
    declarations: Vec<Declaration>,
    config: &CommentConfig,
) -> CommentMetrics {
    let spans: Vec<TargetSpan> = declarations
        .iter()
        .map(|d| TargetSpan {
            start: d.start,
            end: d.end,
            anchor: d.anchor,
        })
        .collect();
    let mut doc_lines: Vec<Option<i64>> = vec![None; declarations.len()];
    let mut comments = Vec::new();
    for block in blocks {
        let span = BlockSpan {
            start: block.start,
            text_end: block.text_end,
            leading_doc: block.leading_doc,
        };
        let (placement, target) = match resolve(span, &spans) {
            Resolved::Above(i) => (Placement::Above, Some(i)),
            Resolved::Docstring(i) => (Placement::Docstring, Some(i)),
            Resolved::Inline(i) => (Placement::Inline, Some(i)),
            Resolved::Unassociated => (Placement::Unassociated, None),
        };
        if matches!(placement, Placement::Above | Placement::Docstring) {
            if let Some(i) = target {
                doc_lines[i].get_or_insert(block.start);
            }
        }
        let (target_type, target_name) = target
            .map(|i| (declarations[i].kind.clone(), declarations[i].name.clone()))
            .unwrap_or_default();
        let score = score_for(placement, &target_type, config);
        comments.extend(block.comments.iter().map(|(text, line)| CommentDetail {
            text: text.clone(),
            line: *line,
            placement,
            score,
            target_type: target_type.clone(),
            target_name: target_name.clone(),
        }));
    }

    let functions: Vec<FunctionInfo> = declarations
        .into_iter()
        .zip(doc_lines)
        .map(|(d, doc_line)| FunctionInfo {
            name: d.name,
            kind: d.kind,
            line: d.start,
            lines: d.end - d.start + 1,
            doc_line,
        })
        .collect();

    let good_comments = comments.iter().filter(|c| c.is_good()).count() as i64;
    let counts = CommentCounts {
        total_comments: comments.len() as i64,
        good_comments,
        bad_comments: comments.len() as i64 - good_comments,
        total_functions: functions.len() as i64,
        documented_functions: functions.iter().filter(|f| f.is_documented()).count() as i64,
    };
    CommentMetrics {
        counts,
        comments,
        functions,
    }
}

fn score_for(placement: Placement, target_type: &str, config: &CommentConfig) -> f64 {
    match placement {
        Placement::Above | Placement::Docstring => config.reward_score,
        Placement::Inline => config
            .penalty_scores
            .get(target_type)
            .copied()
            .unwrap_or(DEFAULT_INLINE_PENALTY),
        Placement::Unassociated => UNASSOCIATED_PENALTY,
    }
}

// --- report building (routed through cf-gojson) -------------------------------

/// Builds the per-file report map: the counters, the derived ratios, the
/// `comments` and `functions` tables and the quality `message`.
#[must_use]
pub fn build_report(metrics: &CommentMetrics) -> GoValue {
    let counts = &metrics.counts;
    // Dynamic report map => map-origin so cf-gojson byte-sorts keys.
    let mut m = GoMap::new(MapOrigin::Map);
    m.push("total_comments", GoValue::Int(counts.total_comments));
    m.push("good_comments", GoValue::Int(counts.good_comments));
    m.push("bad_comments", GoValue::Int(counts.bad_comments));
    m.push("overall_score", GoValue::Float(counts.overall_score()));
    m.push(
        "good_comments_ratio",
        GoValue::Float(counts.overall_score()),
    );
    m.push("total_functions", GoValue::Int(counts.total_functions));
    m.push(
        "documented_functions",
        GoValue::Int(counts.documented_functions),
    );
    m.push(
        "documentation_coverage",
        GoValue::Float(counts.documentation_coverage()),
    );
    m.push("comments", comments_table(&metrics.comments));
    m.push("functions", functions_table(&metrics.functions));
    m.push("message", GoValue::Str(counts.message().to_string()));
    GoValue::Map(m)
}

fn comments_table(comments: &[CommentDetail]) -> GoValue {
    let rows = comments
        .iter()
        .map(|c| {
            let mut m = GoMap::new(MapOrigin::Map);
            m.push("line", GoValue::Int(c.line));
            m.push("comment", GoValue::Str(comment_preview(&c.text)));
            m.push("placement", GoValue::Str(c.placement.as_str().to_string()));
            m.push("target", GoValue::Str(c.target_name.clone()));
            m.push("assessment", GoValue::Str(c.assessment().to_string()));
            GoValue::Map(m)
        })
        .collect();
    GoValue::Array(rows)
}

fn functions_table(functions: &[FunctionInfo]) -> GoValue {
    let rows = functions
        .iter()
        .map(|f| {
            let mut m = GoMap::new(MapOrigin::Map);
            m.push("function", GoValue::Str(f.name.clone()));
            m.push("type", GoValue::Str(f.kind.clone()));
            m.push("line", GoValue::Int(f.line));
            m.push("lines", GoValue::Int(f.lines));
            m.push("doc_line", GoValue::Int(f.doc_line.unwrap_or(0)));
            m.push("assessment", GoValue::Str(f.assessment().to_string()));
            GoValue::Map(m)
        })
        .collect();
    GoValue::Array(rows)
}

/// The comment text shortened for tables: more than 50 bytes keeps the first
/// 47 (backed off to a char boundary) plus `...`.
#[must_use]
pub fn comment_preview(text: &str) -> String {
    if text.len() <= PREVIEW_MAX_BYTES {
        return text.to_string();
    }
    let mut end = PREVIEW_KEEP_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &text[..end])
}
