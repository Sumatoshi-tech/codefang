//! The comments analyzer.
//!
//! Finds comment and function/method/class/interface/struct nodes, groups
//! consecutive comments into blocks (sorted by start line), scores each block
//! by its placement relative to the closest target, and computes aggregate
//! metrics.
//!
//! Report building is routed through [`cf_gojson::GoValue`] using a
//! *map-origin* [`cf_gojson::GoMap`] so the encoder byte-sorts keys at encode
//! time (report-format contract, DESIGN §2.2). Nothing here uses `serde_json`.

use cf_gojson::{GoMap, GoValue, MapOrigin};
use cf_uast_node::Node;

use crate::traverse::{extract_entity_name, find_nodes_by_type};
use crate::types::{
    uast, CommentBlock, CommentConfig, CommentDetail, CommentMetrics, CommentReportItem,
    FunctionInfo, FunctionReportItem,
};

/// Scoring constants (report contract).
const SCORE_VALUE: f64 = 0.2;
const GAP_THRESHOLD_HIGH: i64 = 2;
const LEN_ARG_50: usize = 50;
const MAGIC_3: i64 = 3;
const MAGIC_999: i64 = 999;
const MAGIC_1000: i64 = 1000;
const UNKNOWN_NAME: &str = "unknown";

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
    pub fn new() -> Self {
        Analyzer
    }

    /// Returns the analyzer name (`"comments"`).
    pub fn name(&self) -> &'static str {
        "comments"
    }

    /// Returns the CLI flag (`"comments-analysis"`).
    pub fn flag(&self) -> &'static str {
        "comments-analysis"
    }

    /// Returns the default analysis configuration.
    pub fn default_config(&self) -> CommentConfig {
        CommentConfig::default_config()
    }

    /// Performs comment analysis.
    ///
    /// With no comments found, returns the empty result (all counts zero,
    /// `overall_score` 0.0):
    ///
    /// ```
    /// use cf_comments::Analyzer;
    /// use cf_gojson::GoValue;
    /// use cf_uast_node::Builder;
    ///
    /// let root = Builder::new().with_type("File").build();
    /// let report = Analyzer::new().analyze(Some(&root)).unwrap();
    /// let m = report.as_map().unwrap();
    /// assert_eq!(m.get("total_comments"), Some(&GoValue::Int(0)));
    /// assert_eq!(m.get("overall_score"), Some(&GoValue::Float(0.0)));
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`NilRootNode`] when `root` is `None`.
    pub fn analyze(&self, root: Option<&Node>) -> Result<GoValue, NilRootNode> {
        let root = root.ok_or(NilRootNode)?;

        let comments = self.find_comments(root);
        let functions = self.find_functions(root);

        if comments.is_empty() {
            return Ok(build_empty_result());
        }

        let config = self.default_config();
        let comment_details = self.analyze_comment_placement(&comments, &functions, &config);
        let metrics = self.calculate_metrics(comment_details, &functions);

        Ok(self.build_result(&metrics, &functions))
    }

    // --- node discovery -----------------------------------------------------

    fn find_comments<'a>(&self, root: &'a Node) -> Vec<&'a Node> {
        find_nodes_by_type(root, &[uast::COMMENT])
    }

    fn find_functions<'a>(&self, root: &'a Node) -> Vec<&'a Node> {
        find_nodes_by_type(
            root,
            &[
                uast::FUNCTION,
                uast::METHOD,
                uast::CLASS,
                uast::INTERFACE,
                uast::STRUCT,
            ],
        )
    }

    // --- block grouping (sorted by line) ------------------------------------

    fn analyze_comment_placement(
        &self,
        comments: &[&Node],
        functions: &[&Node],
        config: &CommentConfig,
    ) -> Vec<CommentDetail> {
        let blocks = self.group_comments_into_blocks(comments);
        self.analyze_comment_blocks(&blocks, functions, config)
    }

    fn group_comments_into_blocks(&self, comments: &[&Node]) -> Vec<CommentBlock> {
        if comments.is_empty() {
            return Vec::new();
        }
        let sorted = self.sort_comments_by_line(comments);
        self.create_comment_blocks(&sorted)
    }

    /// Sorts comments by start line. When either node lacks a position the
    /// comparator returns `Equal`, preserving input order — equivalent to the
    /// reference comparator's "not less than" answer for well-formed inputs
    /// (every comment node here carries a position).
    fn sort_comments_by_line<'a>(&self, comments: &[&'a Node]) -> Vec<&'a Node> {
        let mut sorted: Vec<&Node> = comments.to_vec();
        sorted.sort_by(|a, b| match (node_pos(a), node_pos(b)) {
            (Some((a_start, _)), Some((b_start, _))) => a_start.cmp(&b_start),
            _ => std::cmp::Ordering::Equal,
        });
        sorted
    }

    fn create_comment_blocks(&self, sorted: &[&Node]) -> Vec<CommentBlock> {
        let mut blocks: Vec<CommentBlock> = Vec::new();
        let mut current: Option<CommentBlock> = None;

        for &comment in sorted {
            let Some((start, end)) = node_pos(comment) else {
                continue;
            };
            let comment_start = start as i64;
            let comment_end = end as i64;

            let start_new = match &current {
                None => true,
                Some(cur) => cur.comments.is_empty() || comment_start > cur.end_line + 1,
            };

            if start_new {
                if let Some(cur) = current.take() {
                    if !cur.comments.is_empty() {
                        blocks.push(cur);
                    }
                }
                current = Some(CommentBlock {
                    comments: vec![comment.clone()],
                    start_line: comment_start,
                    end_line: comment_end,
                    full_text: node_token(comment).to_string(),
                });
            } else if let Some(cur) = current.as_mut() {
                cur.comments.push(comment.clone());
                cur.end_line = comment_end;
                cur.full_text.push('\n');
                cur.full_text.push_str(node_token(comment));
            }
        }

        if let Some(cur) = current.take() {
            if !cur.comments.is_empty() {
                blocks.push(cur);
            }
        }
        blocks
    }

    // --- block scoring ------------------------------------------------------

    fn analyze_comment_blocks(
        &self,
        blocks: &[CommentBlock],
        functions: &[&Node],
        config: &CommentConfig,
    ) -> Vec<CommentDetail> {
        let mut details = Vec::new();
        for block in blocks {
            details.extend(self.analyze_comment_block(block, functions, config));
        }
        details
    }

    fn analyze_comment_block(
        &self,
        block: &CommentBlock,
        functions: &[&Node],
        config: &CommentConfig,
    ) -> Vec<CommentDetail> {
        let block_detail = self.analyze_virtual_block(block, functions, config);
        self.create_comment_details(block, &block_detail)
    }

    fn create_comment_details(
        &self,
        block: &CommentBlock,
        block_detail: &CommentDetail,
    ) -> Vec<CommentDetail> {
        block
            .comments
            .iter()
            .map(|comment| CommentDetail {
                kind: node_type(comment).to_string(),
                token: node_token(comment).to_string(),
                score: block_detail.score,
                is_good: block_detail.is_good,
                target_type: block_detail.target_type.clone(),
                target_name: block_detail.target_name.clone(),
                position: block_detail.position.clone(),
                line_number: node_pos(comment).map_or(0, |(s, _)| s as i64),
            })
            .collect()
    }

    /// Scores a block using its (`start_line`, `end_line`) span as a single
    /// virtual comment.
    fn analyze_virtual_block(
        &self,
        block: &CommentBlock,
        functions: &[&Node],
        config: &CommentConfig,
    ) -> CommentDetail {
        let mut detail = CommentDetail {
            kind: uast::COMMENT.to_string(),
            token: block.full_text.clone(),
            score: 0.0,
            is_good: false,
            target_type: String::new(),
            target_name: String::new(),
            position: String::new(),
            line_number: block.start_line,
        };

        match self.find_closest_target(block.end_line, functions) {
            Some(target) => {
                detail.target_type = node_type(target).to_string();
                detail.target_name = self.extract_target_name(target);
                detail.position = determine_position(block.start_line, block.end_line, target);
                if is_comment_properly_placed(block.start_line, block.end_line, target) {
                    detail.score = config.reward_score;
                    detail.is_good = true;
                } else {
                    detail.score = self.penalty_score(target, config);
                    detail.is_good = false;
                }
            }
            None => {
                detail.score = -SCORE_VALUE;
                detail.is_good = false;
                detail.position = "unassociated".to_string();
            }
        }
        detail
    }

    fn penalty_score(&self, target: &Node, config: &CommentConfig) -> f64 {
        config
            .penalty_scores
            .get(node_type(target))
            .copied()
            .unwrap_or(-0.1)
    }

    /// Finds the closest function/class to a comment span (see
    /// [`calculate_distance`] for the asymmetric distance).
    fn find_closest_target<'a>(
        &self,
        comment_end: i64,
        functions: &[&'a Node],
    ) -> Option<&'a Node> {
        let mut closest: Option<&Node> = None;
        let mut min_distance: i64 = -1;
        for &function in functions {
            let distance = calculate_distance(comment_end, function);
            if min_distance == -1 || distance < min_distance {
                min_distance = distance;
                closest = Some(function);
            }
        }
        closest
    }

    fn extract_target_name(&self, target: &Node) -> String {
        extract_entity_name(target).unwrap_or_else(|| UNKNOWN_NAME.to_string())
    }

    // --- metrics ------------------------------------------------------------

    fn calculate_metrics(
        &self,
        details: Vec<CommentDetail>,
        functions: &[&Node],
    ) -> CommentMetrics {
        let total_comments = details.len() as i64;
        let mut good_comments = 0i64;
        let mut bad_comments = 0i64;
        for d in &details {
            if d.is_good {
                good_comments += 1;
            } else {
                bad_comments += 1;
            }
        }
        let overall_score = if total_comments > 0 {
            good_comments as f64 / total_comments as f64
        } else {
            0.0
        };

        let mut function_summary: std::collections::BTreeMap<String, FunctionInfo> =
            std::collections::BTreeMap::new();
        let mut documented_functions = 0i64;
        for &function in functions {
            let func_name = self.extract_target_name(function);
            let mut info = FunctionInfo {
                name: func_name.clone(),
                kind: node_type(function).to_string(),
                has_comment: false,
                comment_type: String::new(),
            };
            if has_good_comment(&func_name, &details) {
                info.has_comment = true;
                info.comment_type = comment_type_for(&func_name, &details);
                documented_functions += 1;
            }
            function_summary.insert(func_name, info);
        }

        CommentMetrics {
            total_comments,
            good_comments,
            bad_comments,
            overall_score,
            comment_details: details,
            function_summary,
            total_functions: functions.len() as i64,
            documented_functions,
        }
    }

    // --- report building (routed through cf-gojson) -------------------------

    fn build_result(&self, metrics: &CommentMetrics, functions: &[&Node]) -> GoValue {
        let comment_details_iface = self.build_comment_details_iface(&metrics.comment_details);
        let comments_table = self.build_comments_table(&metrics.comment_details);
        let functions_table = self.build_functions_table(functions, metrics);
        let function_summary_iface = self.build_function_summary_iface(metrics);

        // Dynamic report map => map-origin so cf-gojson byte-sorts keys.
        let mut m = GoMap::new(MapOrigin::Map);
        m.push("total_comments", GoValue::Int(metrics.total_comments));
        m.push("good_comments", GoValue::Int(metrics.good_comments));
        m.push("bad_comments", GoValue::Int(metrics.bad_comments));
        m.push("overall_score", GoValue::Float(metrics.overall_score));
        m.push("total_functions", GoValue::Int(metrics.total_functions));
        m.push(
            "documented_functions",
            GoValue::Int(metrics.documented_functions),
        );
        m.push(
            "good_comments_ratio",
            GoValue::Float(safe_div(
                metrics.good_comments as f64,
                metrics.total_comments as f64,
            )),
        );
        m.push(
            "documentation_coverage",
            GoValue::Float(safe_div(
                metrics.documented_functions as f64,
                metrics.total_functions as f64,
            )),
        );
        m.push(
            "total_comment_details",
            GoValue::Int(metrics.comment_details.len() as i64),
        );
        m.push("comment_details", comment_details_iface);
        m.push("comments", comments_table);
        m.push("functions", functions_table);
        m.push("function_summary", function_summary_iface);
        m.push("message", GoValue::Str(comment_message(metrics.overall_score)));
        GoValue::Map(m)
    }

    fn build_comment_details_iface(&self, details: &[CommentDetail]) -> GoValue {
        let items = details
            .iter()
            .map(|d| {
                let mut m = GoMap::new(MapOrigin::Map);
                m.push("type", GoValue::Str(d.kind.clone()));
                m.push("token", GoValue::Str(d.token.clone()));
                m.push("position", GoValue::Str(d.position.clone()));
                m.push("score", GoValue::Float(d.score));
                m.push("is_good", GoValue::Bool(d.is_good));
                m.push("target_type", GoValue::Str(d.target_type.clone()));
                m.push("target_name", GoValue::Str(d.target_name.clone()));
                m.push("line_number", GoValue::Int(d.line_number));
                GoValue::Map(m)
            })
            .collect();
        GoValue::Array(items)
    }

    fn build_comments_table(&self, details: &[CommentDetail]) -> GoValue {
        let items = details
            .iter()
            .map(|d| {
                let item = CommentReportItem {
                    line: d.line_number,
                    comment: truncate_comment_body(&d.token),
                    placement: d.position.clone(),
                    target: d.target_name.clone(),
                    assessment: comment_assessment(d.is_good),
                };
                let mut m = GoMap::new(MapOrigin::Map);
                m.push("line", GoValue::Int(item.line));
                m.push("comment", GoValue::Str(item.comment));
                m.push("placement", GoValue::Str(item.placement));
                m.push("target", GoValue::Str(item.target));
                m.push("assessment", GoValue::Str(item.assessment));
                GoValue::Map(m)
            })
            .collect();
        GoValue::Array(items)
    }

    fn build_functions_table(&self, functions: &[&Node], metrics: &CommentMetrics) -> GoValue {
        let items = functions
            .iter()
            .map(|&function| {
                let func_name = self.extract_target_name(function);
                let info = metrics.function_summary.get(&func_name);
                let (assessment, comment_type) = function_assessment(info);
                let func_type = function_type(function);
                let lines = function_line_count(function);
                let item = FunctionReportItem {
                    function: func_name,
                    kind: func_type,
                    lines,
                    comment: comment_type,
                    assessment,
                };
                let mut m = GoMap::new(MapOrigin::Map);
                m.push("function", GoValue::Str(item.function));
                m.push("type", GoValue::Str(item.kind));
                m.push("lines", GoValue::Int(item.lines));
                m.push("comment", GoValue::Str(item.comment));
                m.push("assessment", GoValue::Str(item.assessment));
                GoValue::Map(m)
            })
            .collect();
        GoValue::Array(items)
    }

    fn build_function_summary_iface(&self, metrics: &CommentMetrics) -> GoValue {
        let mut outer = GoMap::new(MapOrigin::Map);
        for (name, info) in &metrics.function_summary {
            let mut m = GoMap::new(MapOrigin::Map);
            m.push("name", GoValue::Str(info.name.clone()));
            m.push("type", GoValue::Str(info.kind.clone()));
            m.push("has_comment", GoValue::Bool(info.has_comment));
            m.push("comment_type", GoValue::Str(info.comment_type.clone()));
            outer.push(name.clone(), GoValue::Map(m));
        }
        GoValue::Map(outer)
    }
}

// --- free helpers ------------------------------------------------------------

fn has_good_comment(func_name: &str, details: &[CommentDetail]) -> bool {
    details
        .iter()
        .any(|d| d.target_name == func_name && d.is_good)
}

fn comment_type_for(func_name: &str, details: &[CommentDetail]) -> String {
    for d in details {
        if d.target_name == func_name && d.is_good {
            return d.kind.clone();
        }
    }
    String::new()
}

fn calculate_distance(comment_end_line: i64, target: &Node) -> i64 {
    let Some((target_start, _)) = node_pos(target) else {
        return MAGIC_999;
    };
    let target_line = target_start as i64;
    if comment_end_line < target_line {
        target_line - comment_end_line
    } else {
        MAGIC_1000 + (comment_end_line - target_line)
    }
}

fn is_comment_properly_placed(comment_start: i64, comment_end: i64, target: &Node) -> bool {
    let Some((target_start, _)) = node_pos(target) else {
        return false;
    };
    let target_line = target_start as i64;
    if comment_end >= target_line {
        return false;
    }
    let gap = target_line - comment_end;
    is_gap_acceptable(comment_start, comment_end, gap)
}

fn is_gap_acceptable(comment_start: i64, comment_end: i64, gap: i64) -> bool {
    if comment_start == comment_end {
        gap <= GAP_THRESHOLD_HIGH
    } else {
        gap <= MAGIC_3
    }
}

fn determine_position(comment_start: i64, comment_end: i64, target: &Node) -> String {
    let Some((target_start, _)) = node_pos(target) else {
        return UNKNOWN_NAME.to_string();
    };
    let target_line = target_start as i64;
    if comment_end < target_line {
        return "above".to_string();
    }
    if comment_start > target_line {
        return "below".to_string();
    }
    "inline".to_string()
}

fn comment_assessment(is_good: bool) -> String {
    if is_good {
        "✅ OK".to_string()
    } else {
        "❌ Not OK".to_string()
    }
}

fn truncate_comment_body(body: &str) -> String {
    // Truncation is by BYTE length (report contract): len > 50 => first 47
    // bytes + "...".
    if body.len() > LEN_ARG_50 {
        // Back off to the nearest char boundary <= 47 so we never split a
        // multi-byte sequence; comment tokens here are ASCII-dominant.
        let mut end = 47;
        while end > 0 && !body.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &body[..end])
    } else {
        body.to_string()
    }
}

fn function_assessment(info: Option<&FunctionInfo>) -> (String, String) {
    match info {
        Some(i) if i.has_comment => ("✅ Well Documented".to_string(), i.comment_type.clone()),
        _ => ("❌ No Comment".to_string(), "None".to_string()),
    }
}

fn function_type(function: &Node) -> String {
    let t = node_type(function);
    if t.is_empty() {
        "Unknown".to_string()
    } else {
        t.to_string()
    }
}

fn function_line_count(function: &Node) -> i64 {
    match node_pos(function) {
        Some((start, end)) => (end as i64) - (start as i64) + 1,
        None => 0,
    }
}

fn safe_div(a: f64, b: f64) -> f64 {
    if b == 0.0 {
        0.0
    } else {
        a / b
    }
}

fn comment_message(score: f64) -> String {
    // Threshold labeler with high=0.8, medium=0.6, low=0.4. The label strings
    // are part of the report contract.
    if score >= 0.8 {
        "Excellent comment quality and placement".to_string()
    } else if score >= 0.6 {
        "Good comment quality with room for improvement".to_string()
    } else if score >= 0.4 {
        "Fair comment quality".to_string()
    } else {
        "Poor comment quality".to_string()
    }
}

fn build_empty_result() -> GoValue {
    let mut m = GoMap::new(MapOrigin::Map);
    m.push("total_comments", GoValue::Int(0));
    m.push("good_comments", GoValue::Int(0));
    m.push("bad_comments", GoValue::Int(0));
    m.push("overall_score", GoValue::Float(0.0));
    m.push("total_functions", GoValue::Int(0));
    m.push("documented_functions", GoValue::Int(0));
    m.push("message", GoValue::Str("No comments found".to_string()));
    GoValue::Map(m)
}

// --- cf-uast-node accessors (the single point of API coupling) --------------

/// Returns the node type string.
fn node_type(n: &Node) -> &str {
    &n.node_type
}

/// Returns the node token string.
fn node_token(n: &Node) -> &str {
    &n.token
}

/// Returns `(start_line, end_line)` if the node carries a position.
///
/// `cf_uast_node::Positions` stores lines as `u64`; the analyzer works in
/// `i64` line space, so callers widen these directly.
fn node_pos(n: &Node) -> Option<(u64, u64)> {
    n.pos.as_ref().map(|p| (p.start_line, p.end_line))
}
