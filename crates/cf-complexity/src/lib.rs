//! Static complexity analyzer (cyclomatic / cognitive / nesting).
//!
//! Walks a UAST tree, computes per-function cyclomatic complexity, cognitive
//! complexity (`SonarSource` methodology), nesting depth, decision points,
//! and lines of code, and returns a deterministic result map.
//!
//! # Output shape
//!
//! On success the returned [`cf_gojson::GoValue`] object carries the keys
//! `analyzer_name`, `total_functions`, `average_complexity`, `max_complexity`,
//! `total_complexity`, `cognitive_complexity`, `nesting_depth`,
//! `decision_points`, `functions`, and `message`. The `functions` array holds
//! one object per function with the per-function metric keys plus the
//! assessment strings (`complexity_assessment`, `cognitive_assessment`,
//! `nesting_assessment`).
//!
//! For a missing root or a tree with no functions, the empty-result shape is
//! returned (`total_functions`, `average_complexity`, `max_complexity`,
//! `total_complexity`, `message`).
//!
//! Because [`cf_gojson::GoValue`] objects are map-origin here, their keys
//! byte-sort on encode (report-format contract). Serialization itself is owned
//! by the report layer; this crate only builds the value tree. The static
//! pipeline's aggregated rendering view lives in [`report`].
//!
//! Compatibility: output bytes are pinned against the reference implementation
//! by the differential gate in `tests/compat`.
//!
//! # Example
//!
//! Build a tiny UAST with one function containing an `if`, then read the
//! aggregate scalars off the returned report value:
//!
//! ```
//! use cf_complexity::Analyzer;
//! use cf_complexity::node::{uast, role, Node};
//! use cf_gojson::GoValue;
//!
//! // function foo() { if (...) { ... } }
//! let func = Node::new(uast::FUNCTION)
//!     .with_roles(vec![role::FUNCTION, role::DECLARATION])
//!     .with_prop("name", "foo")
//!     .with_children(vec![Node::new(uast::IF)]);
//! let root = Node::new(uast::FILE).with_children(vec![func]);
//!
//! let report = Analyzer.analyze(Some(&root));
//! let m = report.as_map().unwrap();
//! assert_eq!(m.get("total_functions"), Some(&GoValue::Int(1)));
//! // cyclomatic = 1 (base) + 1 (the `if`) = 2.
//! assert_eq!(m.get("total_complexity"), Some(&GoValue::Int(2)));
//! assert_eq!(m.get("analyzer_name").unwrap().as_str(), Some("complexity"));
//! ```
//!
//! A missing root (or a tree with no functions) yields the empty-result shape:
//!
//! ```
//! use cf_complexity::Analyzer;
//! use cf_gojson::GoValue;
//!
//! let report = Analyzer.analyze(None);
//! let m = report.as_map().unwrap();
//! assert_eq!(m.get("total_functions"), Some(&GoValue::Int(0)));
//! assert_eq!(m.get("message").unwrap().as_str(), Some("No AST provided"));
//! // The empty shape omits the `functions` array and `analyzer_name`.
//! assert!(m.get("functions").is_none());
//! ```

// Metric counts (function counts, line counts, complexity sums) are far below
// the f64 mantissa / i64 range, and the int->float divisions are part of the
// frozen report math; the pedantic cast lints add noise without value here.
#![allow(clippy::cast_possible_wrap, clippy::cast_precision_loss)]

pub mod gosort;
pub mod node;
pub mod report;

use cf_gojson::{GoMap, GoValue, MapOrigin};
use node::{uast, Node};

/// The complexity analyzer. Stateless: all configuration is fixed by the
/// report contract.
#[derive(Debug, Default, Clone, Copy)]
pub struct Analyzer;

/// Per-function complexity metrics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionMetrics {
    /// Function name (or `"anonymous"`).
    pub name: String,
    /// Cyclomatic complexity (1 + decision points).
    pub cyclomatic_complexity: i64,
    /// Cognitive complexity (`SonarSource`, nesting-weighted).
    pub cognitive_complexity: i64,
    /// Maximum nesting depth.
    pub nesting_depth: i64,
    /// Decision points (`max(cyclomatic-1, 0)`).
    pub decision_points: i64,
    /// Estimated lines of code.
    pub lines_of_code: i64,
}

const ANONYMOUS_FUNCTION_NAME: &str = "anonymous";

// Assessment thresholds (report contract).
const CYCLOMATIC_GREEN: i64 = 1;
const CYCLOMATIC_YELLOW: i64 = 5;
const COMPLEXITY_THRESHOLD_HIGH: i64 = 5; // cognitive "low" boundary
const MAGIC10: i64 = 10; // cognitive "medium" boundary
const DEPTH_THRESHOLD_HIGH: i64 = 3; // nesting "shallow" boundary
const MAGIC5: i64 = 5; // nesting "moderate" boundary
const AVG_COMPLEXITY_THRESHOLD_HIGH: f64 = 3.0;
const MAGIC7P0: f64 = 7.0;

const MSG_EXCELLENT: &str = "Excellent complexity - functions are simple and maintainable";
const MSG_GOOD: &str = "Good complexity - functions are reasonably maintainable";
const MSG_FAIR: &str = "Fair complexity - some functions could be simplified";
const MSG_HIGH: &str = "High complexity - functions are complex and should be refactored";

impl Analyzer {
    /// Analyzer name.
    #[must_use]
    pub fn name(&self) -> &'static str {
        "complexity"
    }

    /// CLI flag.
    #[must_use]
    pub fn flag(&self) -> &'static str {
        "complexity-analysis"
    }

    /// Performs complexity analysis and returns the report object as a
    /// [`GoValue`].
    #[must_use]
    pub fn analyze(&self, root: Option<&Node>) -> GoValue {
        let Some(root) = root else {
            return build_empty_result("No AST provided");
        };

        let functions = find_functions(root);
        if functions.is_empty() {
            return build_empty_result("No functions found");
        }

        let (function_metrics, totals) = calculate_all_function_metrics(&functions);
        let avg_complexity = calculate_average_complexity(&totals, function_metrics.len());
        let message = get_complexity_message(avg_complexity);

        build_result(
            function_metrics.len(),
            avg_complexity,
            &totals,
            &function_metrics,
            message,
        )
    }

    /// Computes per-function metrics (in the deterministic report sort order)
    /// without building the report map. Useful for the quality analyzer and
    /// for direct testing.
    ///
    /// The result is sorted by the report predicate: cyclomatic descending,
    /// then cognitive descending, then name ascending. A `None` root yields an
    /// empty vector.
    ///
    /// ```
    /// use cf_complexity::Analyzer;
    /// use cf_complexity::node::{uast, role, Node};
    ///
    /// // `simple` has cyclomatic 1; `branchy` has an `if` (cyclomatic 2).
    /// let simple = Node::new(uast::FUNCTION)
    ///     .with_roles(vec![role::FUNCTION, role::DECLARATION])
    ///     .with_prop("name", "simple");
    /// let branchy = Node::new(uast::FUNCTION)
    ///     .with_roles(vec![role::FUNCTION, role::DECLARATION])
    ///     .with_prop("name", "branchy")
    ///     .with_children(vec![Node::new(uast::IF)]);
    /// let root = Node::new(uast::FILE).with_children(vec![simple, branchy]);
    ///
    /// let metrics = Analyzer.function_metrics(Some(&root));
    /// // Higher cyclomatic first.
    /// assert_eq!(metrics[0].name, "branchy");
    /// assert_eq!(metrics[0].cyclomatic_complexity, 2);
    /// assert_eq!(metrics[1].name, "simple");
    /// assert_eq!(metrics[1].cyclomatic_complexity, 1);
    ///
    /// assert!(Analyzer.function_metrics(None).is_empty());
    /// ```
    #[must_use]
    pub fn function_metrics(&self, root: Option<&Node>) -> Vec<FunctionMetrics> {
        match root {
            None => Vec::new(),
            Some(r) => {
                let functions = find_functions(r);
                calculate_all_function_metrics(&functions).0
            }
        }
    }
}

/// Aggregated totals across all functions.
#[derive(Debug, Default, Clone, Copy)]
struct Totals {
    cyclomatic: i64,
    cognitive: i64,
    nesting: i64,
    decisions: i64,
    max: i64,
}

fn build_empty_result(message: &str) -> GoValue {
    let mut m = GoMap::new(MapOrigin::Map);
    m.insert("total_functions", GoValue::Int(0));
    m.insert("average_complexity", GoValue::Float(0.0));
    m.insert("max_complexity", GoValue::Int(0));
    m.insert("total_complexity", GoValue::Int(0));
    m.insert("message", GoValue::Str(message.to_string()));
    GoValue::Map(m)
}

fn build_result(
    function_count: usize,
    avg_complexity: f64,
    totals: &Totals,
    function_metrics: &[FunctionMetrics],
    message: &str,
) -> GoValue {
    let mut functions_arr: Vec<GoValue> = Vec::with_capacity(function_metrics.len());
    for fm in function_metrics {
        let mut fo = GoMap::new(MapOrigin::Map);
        fo.insert("name", GoValue::Str(fm.name.clone()));
        fo.insert(
            "cyclomatic_complexity",
            GoValue::Int(fm.cyclomatic_complexity),
        );
        fo.insert(
            "cognitive_complexity",
            GoValue::Int(fm.cognitive_complexity),
        );
        fo.insert("nesting_depth", GoValue::Int(fm.nesting_depth));
        fo.insert("lines_of_code", GoValue::Int(fm.lines_of_code));
        fo.insert(
            "complexity_assessment",
            GoValue::Str(get_complexity_assessment(fm.cyclomatic_complexity).to_string()),
        );
        fo.insert(
            "cognitive_assessment",
            GoValue::Str(get_cognitive_assessment(fm.cognitive_complexity).to_string()),
        );
        fo.insert(
            "nesting_assessment",
            GoValue::Str(get_nesting_assessment(fm.nesting_depth).to_string()),
        );
        functions_arr.push(GoValue::Map(fo));
    }

    let mut m = GoMap::new(MapOrigin::Map);
    m.insert("analyzer_name", GoValue::Str("complexity".to_string()));
    m.insert("total_functions", GoValue::Int(function_count as i64));
    m.insert("average_complexity", GoValue::Float(avg_complexity));
    m.insert("max_complexity", GoValue::Int(totals.max));
    m.insert("total_complexity", GoValue::Int(totals.cyclomatic));
    m.insert("cognitive_complexity", GoValue::Int(totals.cognitive));
    m.insert("nesting_depth", GoValue::Int(totals.nesting));
    m.insert("decision_points", GoValue::Int(totals.decisions));
    m.insert("functions", GoValue::Array(functions_arr));
    m.insert("message", GoValue::Str(message.to_string()));
    GoValue::Map(m)
}

/// Finds all function nodes.
///
/// Nodes are collected by type (`Function`, `Method`) and by role (`Function`),
/// deduplicated by identity, then filtered through [`is_function_node`]. The
/// reference implementation's dedup-set iteration order is nondeterministic,
/// but the caller sorts the resulting metrics deterministically, so collecting
/// in a stable pre-order is equivalent; the final order is fixed by
/// [`calculate_all_function_metrics`].
fn find_functions(root: &Node) -> Vec<&Node> {
    let mut by_type: Vec<&Node> = Vec::new();
    root.find_nodes_by_type(&[uast::FUNCTION, uast::METHOD], &mut by_type);
    let mut by_role: Vec<&Node> = Vec::new();
    root.find_nodes_by_roles(&[node::role::FUNCTION], &mut by_role);

    let mut seen: Vec<*const Node> = Vec::new();
    let mut functions: Vec<&Node> = Vec::new();
    for n in by_type.into_iter().chain(by_role) {
        let ptr = std::ptr::from_ref::<Node>(n);
        if seen.contains(&ptr) {
            continue;
        }
        seen.push(ptr);
        if is_function_node(n) {
            functions.push(n);
        }
    }
    functions
}

/// A function node is a `Function`/`Method` type, OR carries both the
/// `Function` and `Declaration` roles.
fn is_function_node(n: &Node) -> bool {
    n.has_any_type(&[uast::FUNCTION, uast::METHOD])
        || n.has_all_roles(&[node::role::FUNCTION, node::role::DECLARATION])
}

/// Computes per-function metrics plus totals, sorted by the report contract's
/// exact predicate (cyclomatic desc, then cognitive desc, then name asc).
fn calculate_all_function_metrics(functions: &[&Node]) -> (Vec<FunctionMetrics>, Totals) {
    let mut metrics: Vec<FunctionMetrics> = Vec::with_capacity(functions.len());
    let mut totals = Totals::default();

    for fn_node in functions {
        let m = calculate_function_metrics(fn_node);
        update_totals(&mut totals, &m);
        metrics.push(m);
    }

    // The report contract is pinned to an UNSTABLE sort (pdqsort), not a stable
    // one. For functions that tie on every key (same cyclomatic, cognitive, and
    // name — e.g. several identically-named methods in one file), a stable
    // `sort_by` would preserve input order while pdqsort permutes them.
    // Reproduce the exact element movement with the shared pdqsort port so tie
    // ordering matches byte-for-byte in formats whose final sort key (e.g. the
    // YAML `function_complexity` cyclomatic-only sort) does not otherwise break
    // those ties.
    gosort::go_sort_slice(&mut metrics, |left, right| {
        if left.cyclomatic_complexity != right.cyclomatic_complexity {
            return left.cyclomatic_complexity > right.cyclomatic_complexity;
        }
        if left.cognitive_complexity != right.cognitive_complexity {
            return left.cognitive_complexity > right.cognitive_complexity;
        }
        left.name < right.name
    });

    (metrics, totals)
}

fn update_totals(totals: &mut Totals, m: &FunctionMetrics) {
    totals.cyclomatic += m.cyclomatic_complexity;
    totals.cognitive += m.cognitive_complexity;
    totals.nesting += m.nesting_depth;
    totals.decisions += m.decision_points;
    if m.cyclomatic_complexity > totals.max {
        totals.max = m.cyclomatic_complexity;
    }
}

fn calculate_function_metrics(fn_node: &Node) -> FunctionMetrics {
    let name = extract_function_name(fn_node);
    let cyclomatic = calculate_cyclomatic_complexity(fn_node);
    FunctionMetrics {
        name,
        cyclomatic_complexity: cyclomatic,
        cognitive_complexity: calculate_cognitive_complexity(fn_node),
        nesting_depth: calculate_nesting_depth(fn_node),
        decision_points: (cyclomatic - 1).max(0),
        lines_of_code: estimate_lines_of_code(fn_node),
    }
}

fn calculate_average_complexity(totals: &Totals, function_count: usize) -> f64 {
    if function_count == 0 {
        return 0.0;
    }
    totals.cyclomatic as f64 / function_count as f64
}

// --- Cyclomatic complexity ---

fn calculate_cyclomatic_complexity(fn_node: &Node) -> i64 {
    let ctx = function_source_context(fn_node);
    let mut complexity: i64 = 1;
    fn_node.visit_pre_order(&mut |n| {
        if std::ptr::eq(n, fn_node) {
            return;
        }
        if is_decision_point_with_source(n, &ctx) {
            complexity += 1;
        }
    });
    complexity
}

/// Builds the source context from a function node.
fn function_source_context(fn_node: &Node) -> FunctionSourceContext<'_> {
    match &fn_node.pos {
        Some(pos) if !fn_node.token.is_empty() => FunctionSourceContext {
            source: fn_node.token.as_bytes(),
            start_offset: pos.start_offset,
        },
        _ => FunctionSourceContext {
            source: &[],
            start_offset: 0,
        },
    }
}

/// Like [`is_decision_point`] but recovers a `BinaryOp`'s operator from the
/// function source slice when token/props omit it.
fn is_decision_point_with_source(target: &Node, ctx: &FunctionSourceContext) -> bool {
    match target.node_type.as_str() {
        uast::IF | uast::LOOP | uast::CATCH => true,
        uast::CASE => !is_default_case(target),
        uast::BINARY_OP => {
            let op = ctx.binary_operator(target);
            !op.is_empty() && is_logical_operator_token(&op)
        }
        _ => false,
    }
}

/// The AST-metadata variant of the decision-point predicate: reads the
/// `BinaryOp` operator only from the `operator` prop. Production code uses
/// [`is_decision_point_with_source`]; this variant exists for the unit tests
/// that exercise the prop-only classification.
#[cfg(test)]
fn is_decision_point(target: &Node) -> bool {
    match target.node_type.as_str() {
        uast::IF | uast::LOOP | uast::CATCH => true,
        uast::CASE => !is_default_case(target),
        uast::BINARY_OP => {
            let op = target.prop("operator").unwrap_or("");
            if op.is_empty() {
                return false;
            }
            is_logical_operator_token(op)
        }
        _ => false,
    }
}

// --- Cognitive complexity (SonarSource / gocognit model) ---
//
// The calculator walks the function's children with a `walk_node` recursion
// that tracks structural nesting, recovers binary operators (from token,
// props, or the function source slice via the source context), and counts:
//   * a `nesting + 1` increment for each `if` (non-else-if), loop, switch,
//     try, catch, match (structural + nesting penalty);
//   * a flat `+1` for an `else if` and for an `else` block;
//   * logical-operator *sequence* increments
//     (`add_logical_sequence_complexity`): +1 for the first run of logical
//     operators in an if condition, +1 for each change of operator kind along
//     the flattened operator stream;
//   * `+1` for a recursive call (call whose name equals the function name);
//   * lambda bodies raise nesting by one.

/// Holds the function's original source bytes and its start offset, so binary
/// operators can be recovered from the source when the UAST omits them.
struct FunctionSourceContext<'a> {
    source: &'a [u8],
    start_offset: u32,
}

fn calculate_cognitive_complexity(fn_node: &Node) -> i64 {
    let ctx = function_source_context(fn_node);
    let function_name = extract_cognitive_function_name(fn_node);

    let mut complexity: i64 = 0;
    for (idx, child) in fn_node.children.iter().enumerate() {
        walk_node(
            child,
            fn_node,
            idx,
            0,
            &ctx,
            &function_name,
            &mut complexity,
        );
    }
    complexity
}

#[allow(clippy::too_many_arguments)]
fn walk_node(
    curr: &Node,
    parent: &Node,
    child_idx: usize,
    nesting: i64,
    ctx: &FunctionSourceContext,
    function_name: &str,
    complexity: &mut i64,
) {
    match curr.node_type.as_str() {
        uast::IF => {
            process_if_node(
                curr,
                parent,
                child_idx,
                nesting,
                ctx,
                function_name,
                complexity,
            );
            return;
        }
        uast::LOOP | uast::SWITCH | uast::TRY | uast::CATCH | uast::MATCH => {
            *complexity += nesting + 1;
            for (idx, child) in curr.children.iter().enumerate() {
                walk_node(
                    child,
                    curr,
                    idx,
                    nesting + 1,
                    ctx,
                    function_name,
                    complexity,
                );
            }
            return;
        }
        uast::LAMBDA => {
            for (idx, child) in curr.children.iter().enumerate() {
                walk_node(
                    child,
                    curr,
                    idx,
                    nesting + 1,
                    ctx,
                    function_name,
                    complexity,
                );
            }
            return;
        }
        uast::CALL => {
            if is_recursive_call(curr, function_name) {
                *complexity += 1;
            }
        }
        _ => {}
    }

    for (idx, child) in curr.children.iter().enumerate() {
        walk_node(child, curr, idx, nesting, ctx, function_name, complexity);
    }
}

#[allow(clippy::too_many_arguments)]
fn process_if_node(
    if_node: &Node,
    parent: &Node,
    child_idx: usize,
    nesting: i64,
    ctx: &FunctionSourceContext,
    function_name: &str,
    complexity: &mut i64,
) {
    if is_else_if_cognitive(parent, if_node, child_idx) {
        *complexity += 1;
    } else {
        *complexity += nesting + 1;
    }

    if !if_node.children.is_empty() {
        add_logical_sequence_complexity(&if_node.children[0], ctx, complexity);
        walk_node(
            &if_node.children[0],
            if_node,
            0,
            nesting,
            ctx,
            function_name,
            complexity,
        );
    }

    if if_node.children.len() > 1 {
        walk_node(
            &if_node.children[1],
            if_node,
            1,
            nesting + 1,
            ctx,
            function_name,
            complexity,
        );
    }

    for idx in 2..if_node.children.len() {
        let child = &if_node.children[idx];
        // Sonar/gocognit: an `else` branch (a Block in the else slot) adds one
        // structural increment; an `else if` is handled by the nested walk.
        if child.node_type == uast::BLOCK {
            *complexity += 1;
        }
        walk_node(child, if_node, idx, nesting, ctx, function_name, complexity);
    }
}

/// Adds the logical-sequence increments: +1 for the first logical-operator run
/// in the condition, +1 for each change of operator kind along the flattened
/// left-to-right operator stream.
fn add_logical_sequence_complexity(expr: &Node, ctx: &FunctionSourceContext, complexity: &mut i64) {
    let mut operators: Vec<String> = Vec::new();
    collect_logical_operators(expr, ctx, &mut operators);
    if operators.is_empty() {
        return;
    }
    *complexity += 1;
    let mut last_op = operators[0].clone();
    for op in &operators[1..] {
        if *op != last_op {
            *complexity += 1;
            last_op.clone_from(op);
        }
    }
}

fn collect_logical_operators(
    curr: &Node,
    ctx: &FunctionSourceContext,
    operators: &mut Vec<String>,
) {
    if curr.node_type == uast::BINARY_OP && curr.children.len() >= 2 {
        collect_logical_operators(&curr.children[0], ctx, operators);
        let op = ctx.binary_operator(curr);
        if is_logical_operator_token(&op) {
            operators.push(op);
        }
        collect_logical_operators(&curr.children[1], ctx, operators);
        return;
    }
    for child in &curr.children {
        collect_logical_operators(child, ctx, operators);
    }
}

impl FunctionSourceContext<'_> {
    /// Recovers a `BinaryOp`'s operator: token, then props, then a best-effort
    /// recovery from the source slice between the operands.
    fn binary_operator(&self, n: &Node) -> String {
        if !n.token.is_empty() {
            let op = normalize_operator_text(&n.token);
            if !op.is_empty() {
                return op;
            }
        }
        if let Some(p) = n.prop("operator") {
            let op = normalize_operator_text(p);
            if !op.is_empty() {
                return op;
            }
        }
        self.binary_operator_from_offsets(n)
    }

    fn binary_operator_from_offsets(&self, n: &Node) -> String {
        if self.source.is_empty() || n.children.len() < 2 {
            return String::new();
        }
        let (left, right) = (&n.children[0], &n.children[1]);
        let (Some(lp), Some(rp)) = (&left.pos, &right.pos) else {
            return String::new();
        };
        if rp.start_offset <= lp.end_offset || lp.end_offset < self.start_offset {
            return String::new();
        }
        let start = (lp.end_offset - self.start_offset) as usize;
        let end = (rp.start_offset - self.start_offset) as usize;
        if start >= end || end > self.source.len() {
            return String::new();
        }
        let segment = String::from_utf8_lossy(&self.source[start..end]);
        normalize_operator_text(&segment)
    }
}

/// Extracts the function name for recursive-call detection only.
///
/// This is deliberately NARROWER than [`extract_function_name`]: it consults
/// only the entity-name chain (`name` prop -> token -> first child's token ->
/// first child's `name` prop) plus a final `name`-prop check, and returns `""`
/// otherwise. Critically it does NOT consult a `Name`-role child token. Using
/// the broader extraction here would surface a name for functions the report
/// contract treats as unnamed, which over-counts recursive calls and inflates
/// the cognitive total.
fn extract_cognitive_function_name(fn_node: &Node) -> String {
    if let Some(n) = extract_entity_name(fn_node) {
        if !n.is_empty() {
            return n;
        }
    }
    if let Some(v) = fn_node.prop("name") {
        if !v.is_empty() {
            return v.to_string();
        }
    }
    String::new()
}

fn is_recursive_call(call_node: &Node, function_name: &str) -> bool {
    if function_name.is_empty() {
        return false;
    }
    let call_name = extract_call_name(call_node);
    !call_name.is_empty() && call_name == function_name
}

/// Extracts a call's target name: `name` prop, then a `Name`-role child token,
/// then the first child's token.
fn extract_call_name(call_node: &Node) -> String {
    if let Some(name) = call_node.prop("name") {
        if !name.is_empty() {
            return name.to_string();
        }
    }
    for child in &call_node.children {
        if child.has_any_role(&[node::role::NAME]) && !child.token.is_empty() {
            return child.token.clone();
        }
    }
    if let Some(first) = call_node.children.first() {
        if !first.token.is_empty() {
            return first.token.clone();
        }
    }
    String::new()
}

/// Normalizes raw operator text: trim, strip all whitespace, then keep only
/// the recognized logical/comparison operators.
fn normalize_operator_text(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    let compact: String = raw.trim().chars().filter(|c| !c.is_whitespace()).collect();
    match compact.as_str() {
        "&&" | "||" | "and" | "or" | "AND" | "OR" | "<" | ">" | "<=" | ">=" | "==" | "!=" => {
            compact
        }
        _ => String::new(),
    }
}

// --- Nesting depth ---

fn calculate_nesting_depth(fn_node: &Node) -> i64 {
    let mut max_depth: i64 = 0;
    for (idx, child) in fn_node.children.iter().enumerate() {
        walk_nesting(child, 0, Some(fn_node), idx, &mut max_depth);
    }
    max_depth
}

fn walk_nesting(
    curr: &Node,
    depth: i64,
    parent: Option<&Node>,
    child_idx: usize,
    max_depth: &mut i64,
) {
    let mut current_depth = depth;
    if is_nesting_node(curr) && !is_else_if_node(parent, curr, child_idx) {
        current_depth += 1;
        if current_depth > *max_depth {
            *max_depth = current_depth;
        }
    }
    for (idx, child) in curr.children.iter().enumerate() {
        walk_nesting(child, current_depth, Some(curr), idx, max_depth);
    }
}

/// Nesting nodes: `If`, `Loop`, `Switch`, `Try`, `Catch`.
fn is_nesting_node(target: &Node) -> bool {
    matches!(
        target.node_type.as_str(),
        uast::IF | uast::LOOP | uast::SWITCH | uast::TRY | uast::CATCH
    )
}

// --- Lines of code ---

fn estimate_lines_of_code(fn_node: &Node) -> i64 {
    if let Some(pos) = &fn_node.pos {
        if pos.end_line >= pos.start_line {
            return i64::from(pos.end_line - pos.start_line) + 1;
        }
    }
    let mut loc: i64 = 0;
    fn_node.visit_pre_order(&mut |n| {
        if !n.token.is_empty() {
            let lines = n.token.matches('\n').count() as i64 + 1;
            loc += lines;
        }
    });
    loc
}

// --- Name extraction ---

/// Extracts the display name for a function node.
///
/// The FIRST branch is the entity-name chain (`name` prop -> token -> first
/// child's token -> first child's `name` prop). For anonymous functions whose
/// props lack `name`, this surfaces the first child's token (the full
/// parameter/receiver signature, e.g. `"(action cgotesting.Action)"` or
/// `"()"`) — reference-implementation behavior, pinned by the differential
/// gate. Then the prop fallbacks, then a `Name`-role child token, then
/// `"anonymous"`.
fn extract_function_name(fn_node: &Node) -> String {
    if let Some(n) = extract_entity_name(fn_node) {
        if !n.is_empty() {
            return n;
        }
    }
    if let Some(n) = extract_name_from_props(fn_node) {
        return n;
    }
    // Name-role child token fallback.
    let mut name_nodes: Vec<&Node> = Vec::new();
    fn_node.find_nodes_by_roles(&[node::role::NAME], &mut name_nodes);
    if let Some(first) = name_nodes.first() {
        let token = first.token.trim();
        if !token.is_empty() {
            return token.to_string();
        }
    }
    ANONYMOUS_FUNCTION_NAME.to_string()
}

/// The shared entity-name chain: `name` prop -> own token -> first child's
/// token -> first child's `name` prop. Each branch returns `Some(value)` when
/// the source is present (even empty), so the caller's `!is_empty()` guard
/// preserves the "present but empty means stop" semantics. Note: unlike
/// [`extract_name_from_props`], the prop lookup here is NOT trimmed and does
/// not consider `function_name`/`method_name`.
fn extract_entity_name(n: &Node) -> Option<String> {
    if let Some(v) = n.prop("name") {
        return Some(v.to_string());
    }
    if !n.token.is_empty() {
        return Some(n.token.clone());
    }
    if let Some(child) = n.children.first() {
        if !child.token.is_empty() {
            return Some(child.token.clone());
        }
        if let Some(v) = child.prop("name") {
            return Some(v.to_string());
        }
    }
    None
}

fn extract_name_from_props(fn_node: &Node) -> Option<String> {
    for key in ["name", "function_name", "method_name"] {
        if let Some(v) = fn_node.prop(key) {
            let trimmed = v.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

// --- Flow helpers ---

/// True for a switch `default` case: trim + LOWERCASE the case node's OWN
/// token, then test for the `default` prefix. It must NOT inspect children: a
/// non-default `case X:` whose body begins with an identifier like
/// `defaultedPod` would otherwise be misclassified as the default case and
/// dropped from the cyclomatic decision count (an off-by-1 observed on real
/// input before this was pinned).
fn is_default_case(n: &Node) -> bool {
    n.token.trim().to_lowercase().starts_with("default")
}

/// True if `curr` is the else-if continuation of a parent `If`.
///
/// An `If` is an else-if continuation only at child index >= 2 (after
/// [condition(0), then-block(1)]). Using `> 0` would wrongly treat a braceless
/// nested `if (a) if (b)` (inner `If` at index 1) as an else-if, skipping its
/// nesting increment and under-counting `nesting_depth` by 1 on C-style code.
/// Matches [`is_else_if_cognitive`].
fn is_else_if_node(parent: Option<&Node>, curr: &Node, child_idx: usize) -> bool {
    let Some(parent) = parent else {
        return false;
    };
    if parent.node_type != uast::IF || curr.node_type != uast::IF {
        return false;
    }
    child_idx >= 2
}

/// Logical operators, including the uppercase `AND`/`OR` forms used by some
/// languages.
fn is_logical_operator_token(op: &str) -> bool {
    matches!(op.trim(), "&&" | "||" | "and" | "or" | "AND" | "OR")
}

/// The else-if predicate for the cognitive calculator: an `If` nested as the
/// third-or-later child (index >= 2) of another `If` (the else-if slot).
fn is_else_if_cognitive(parent: &Node, child: &Node, child_idx: usize) -> bool {
    parent.node_type == uast::IF && child.node_type == uast::IF && child_idx >= 2
}

// --- Assessment / message helpers ---

fn get_complexity_level(complexity: i64) -> &'static str {
    if complexity <= CYCLOMATIC_GREEN {
        "green"
    } else if complexity <= CYCLOMATIC_YELLOW {
        "yellow"
    } else {
        "red"
    }
}

/// Cyclomatic-complexity assessment label — exported for the aggregated
/// raw-report builder used by `--format plot` / report.json.
///
/// `<= 1` is simple, `<= 5` is moderate, otherwise complex:
///
/// ```
/// use cf_complexity::get_complexity_assessment;
/// assert_eq!(get_complexity_assessment(1), "🟢 Simple");
/// assert_eq!(get_complexity_assessment(5), "🟡 Moderate");
/// assert_eq!(get_complexity_assessment(6), "🔴 Complex");
/// ```
#[must_use]
pub fn get_complexity_assessment(complexity: i64) -> &'static str {
    match get_complexity_level(complexity) {
        "green" => "🟢 Simple",
        "yellow" => "🟡 Moderate",
        "red" => "🔴 Complex",
        _ => "⚪ Unknown",
    }
}

/// Cognitive-complexity assessment label — exported for the aggregated
/// raw-report builder.
///
/// `<= 5` is low, `<= 10` is medium, otherwise high:
///
/// ```
/// use cf_complexity::get_cognitive_assessment;
/// assert_eq!(get_cognitive_assessment(5), "🟢 Low");
/// assert_eq!(get_cognitive_assessment(10), "🟡 Medium");
/// assert_eq!(get_cognitive_assessment(11), "🔴 High");
/// ```
#[must_use]
pub fn get_cognitive_assessment(complexity: i64) -> &'static str {
    if complexity <= COMPLEXITY_THRESHOLD_HIGH {
        "🟢 Low"
    } else if complexity <= MAGIC10 {
        "🟡 Medium"
    } else {
        "🔴 High"
    }
}

/// Nesting-depth assessment label — exported for the aggregated raw-report
/// builder.
///
/// `<= 3` is shallow, `<= 5` is moderate, otherwise deep:
///
/// ```
/// use cf_complexity::get_nesting_assessment;
/// assert_eq!(get_nesting_assessment(3), "🟢 Shallow");
/// assert_eq!(get_nesting_assessment(5), "🟡 Moderate");
/// assert_eq!(get_nesting_assessment(6), "🔴 Deep");
/// ```
#[must_use]
pub fn get_nesting_assessment(depth: i64) -> &'static str {
    if depth <= DEPTH_THRESHOLD_HIGH {
        "🟢 Shallow"
    } else if depth <= MAGIC5 {
        "🟡 Moderate"
    } else {
        "🔴 Deep"
    }
}

fn get_complexity_message(avg_complexity: f64) -> &'static str {
    if avg_complexity <= 1.0 {
        MSG_EXCELLENT
    } else if avg_complexity <= AVG_COMPLEXITY_THRESHOLD_HIGH {
        MSG_GOOD
    } else if avg_complexity <= MAGIC7P0 {
        MSG_FAIR
    } else {
        MSG_HIGH
    }
}

#[cfg(test)]
mod tests {
    use super::{
        calculate_cognitive_complexity, calculate_cyclomatic_complexity, calculate_nesting_depth,
        extract_function_name, is_decision_point, Analyzer,
    };
    use crate::node::{uast, Node};
    use cf_gojson::GoValue;

    fn obj_get<'a>(v: &'a GoValue, key: &str) -> &'a GoValue {
        v.as_map()
            .and_then(|m| m.get(key))
            .unwrap_or_else(|| panic!("missing key {key}"))
    }

    fn as_int(v: &GoValue) -> i64 {
        match v {
            GoValue::Int(i) => *i,
            other => panic!("not an int: {other:?}"),
        }
    }

    fn as_array(v: &GoValue) -> &Vec<GoValue> {
        match v {
            GoValue::Array(a) => a,
            _ => panic!("not an array"),
        }
    }

    #[test]
    fn analyzer_basic_name() {
        assert_eq!(Analyzer.name(), "complexity");
    }

    #[test]
    fn analyzer_flag() {
        assert_eq!(Analyzer.flag(), "complexity-analysis");
    }

    #[test]
    fn nil_root_total_functions_zero() {
        let result = Analyzer.analyze(None);
        assert_eq!(as_int(obj_get(&result, "total_functions")), 0);
    }

    #[test]
    fn simple_function() {
        let mut function_node = Node::new(uast::FUNCTION).with_roles(vec![
            crate::node::role::FUNCTION,
            crate::node::role::DECLARATION,
        ]);
        let name_node = Node::with_token(uast::IDENTIFIER, "simpleFunction")
            .with_roles(vec![crate::node::role::NAME]);
        function_node.add_child(name_node);

        let mut root = Node::new(uast::FILE);
        root.add_child(function_node);

        let result = Analyzer.analyze(Some(&root));
        assert_eq!(as_int(obj_get(&result, "total_functions")), 1);
        assert_eq!(as_int(obj_get(&result, "total_complexity")), 1);
    }

    #[test]
    fn extract_function_name_with_and_without_name() {
        let mut function_node = Node::new(uast::FUNCTION);
        let name_node = Node::with_token(uast::IDENTIFIER, "testFunction")
            .with_roles(vec![crate::node::role::NAME]);
        function_node.add_child(name_node);
        assert_eq!(extract_function_name(&function_node), "testFunction");

        let anon = Node::new(uast::FUNCTION);
        assert_eq!(extract_function_name(&anon), "anonymous");
    }

    #[test]
    fn is_decision_point_classification() {
        for t in [uast::IF, uast::LOOP, uast::CASE, uast::CATCH] {
            assert!(
                is_decision_point(&Node::new(t)),
                "{t} should be a decision point"
            );
        }
        let default_case = Node::with_token(uast::CASE, "default:\n\treturn 0");
        assert!(!is_decision_point(&default_case));

        assert!(!is_decision_point(&Node::new(uast::IDENTIFIER)));

        let logical = Node::new(uast::BINARY_OP).with_prop("operator", "&&");
        assert!(is_decision_point(&logical));

        let arithmetic = Node::new(uast::BINARY_OP).with_prop("operator", "+");
        assert!(!is_decision_point(&arithmetic));
    }

    #[test]
    fn with_if_statement() {
        let mut function_node = Node::new(uast::FUNCTION).with_roles(vec![
            crate::node::role::FUNCTION,
            crate::node::role::DECLARATION,
        ]);
        function_node.add_child(
            Node::with_token(uast::IDENTIFIER, "testFunction")
                .with_roles(vec![crate::node::role::NAME]),
        );
        function_node.add_child(Node::new(uast::IF).with_roles(vec![crate::node::role::CONDITION]));

        let mut root = Node::new(uast::FILE);
        root.add_child(function_node);

        let result = Analyzer.analyze(Some(&root));
        assert_eq!(as_int(obj_get(&result, "total_complexity")), 2);
    }

    /// The if-processing walks an `If`'s first child (its condition slot) at
    /// the SAME nesting level, so a nested `If` placed at index 0 receives no
    /// nesting penalty: outer `if` (+1) + inner `if` at nesting 0 (+1) = 2.
    /// Verified against the reference cognitive-complexity calculator for this
    /// exact tree.
    #[test]
    fn cognitive_nested_ifs() {
        let inner_if = Node::new(uast::IF).with_roles(vec![crate::node::role::CONDITION]);
        let outer_if = Node::new(uast::IF)
            .with_roles(vec![crate::node::role::CONDITION])
            .with_children(vec![inner_if]);
        let function_node = Node::new(uast::FUNCTION)
            .with_roles(vec![crate::node::role::FUNCTION])
            .with_children(vec![outer_if]);

        assert_eq!(calculate_cognitive_complexity(&function_node), 2);
    }

    /// Cyclomatic/cognitive/nesting parity for the canonical `if{loop{if}}`
    /// body, reproducing the SonarSource model. The loop sits in the outer-if's
    /// first-child (condition) slot, walked at nesting 0; its inner `if` is in
    /// the loop's first-child slot, walked at nesting 1.
    /// cognitive = if(+1) + loop(+1) + inner-if(nesting 1 → +2) = 4. Verified
    /// against the reference cognitive-complexity calculator.
    #[test]
    fn nested_if_loop_if_metrics() {
        let inner_if = Node::new(uast::IF);
        let loop_node = Node::new(uast::LOOP).with_children(vec![inner_if]);
        let outer_if = Node::new(uast::IF).with_children(vec![loop_node]);
        let func = Node::new(uast::FUNCTION)
            .with_roles(vec![
                crate::node::role::FUNCTION,
                crate::node::role::DECLARATION,
            ])
            .with_children(vec![outer_if]);

        // cyclomatic: 1 + if + loop + if = 4
        assert_eq!(calculate_cyclomatic_complexity(&func), 4);
        // cognitive: if(+1) + loop(+1) + inner-if@nesting1(+2) = 4
        assert_eq!(calculate_cognitive_complexity(&func), 4);
        // nesting: if->loop->if = 3
        assert_eq!(calculate_nesting_depth(&func), 3);
    }

    /// Else-if chains are not counted as additional nesting: a child `If` is an
    /// else-if continuation only at child index >= 2, after
    /// [condition(0), then-block(1)].
    #[test]
    fn else_if_does_not_increase_nesting() {
        // outer if with child0 = condition, child1 = then-block, child2 =
        // else-if; the else-if (index 2) must not add nesting.
        let cond = Node::new(uast::IDENTIFIER);
        let block = Node::new(uast::BLOCK);
        let else_if = Node::new(uast::IF);
        let outer = Node::new(uast::IF).with_children(vec![cond, block, else_if]);
        let func = Node::new(uast::FUNCTION)
            .with_roles(vec![crate::node::role::FUNCTION])
            .with_children(vec![outer]);
        // Only the outer if nests => depth 1.
        assert_eq!(calculate_nesting_depth(&func), 1);

        // Conversely, an inner `If` at index 1 (braceless `if (a) if (b)`) is
        // NOT an else-if continuation and DOES nest => depth 2.
        let inner = Node::new(uast::IF);
        let cond2 = Node::new(uast::IDENTIFIER);
        let outer2 = Node::new(uast::IF).with_children(vec![cond2, inner]);
        let func2 = Node::new(uast::FUNCTION)
            .with_roles(vec![crate::node::role::FUNCTION])
            .with_children(vec![outer2]);
        assert_eq!(calculate_nesting_depth(&func2), 2);
    }

    /// Switch counts as a nesting node and a cognitive increment (loop-like).
    #[test]
    fn switch_is_nesting_and_cognitive() {
        let sw = Node::new(uast::SWITCH);
        let func = Node::new(uast::FUNCTION)
            .with_roles(vec![crate::node::role::FUNCTION])
            .with_children(vec![sw]);
        assert_eq!(calculate_nesting_depth(&func), 1);
        assert_eq!(calculate_cognitive_complexity(&func), 1);
    }

    /// The success result carries the report-contract keys, including
    /// assessments.
    #[test]
    fn result_shape_has_report_keys_and_assessments() {
        let mut func = Node::new(uast::FUNCTION).with_roles(vec![
            crate::node::role::FUNCTION,
            crate::node::role::DECLARATION,
        ]);
        func.add_child(Node::new(uast::IF));
        let mut root = Node::new(uast::FILE);
        root.add_child(func);

        let result = Analyzer.analyze(Some(&root));
        for key in [
            "analyzer_name",
            "total_functions",
            "average_complexity",
            "max_complexity",
            "total_complexity",
            "cognitive_complexity",
            "nesting_depth",
            "decision_points",
            "functions",
            "message",
        ] {
            assert!(result.as_map().unwrap().contains_key(key), "missing {key}");
        }
        let functions = as_array(obj_get(&result, "functions"));
        assert_eq!(functions.len(), 1);
        let f0 = &functions[0];
        assert!(f0.as_map().unwrap().contains_key("complexity_assessment"));
        assert!(f0.as_map().unwrap().contains_key("cognitive_assessment"));
        assert!(f0.as_map().unwrap().contains_key("nesting_assessment"));
    }

    /// Sort order: cyclomatic desc, then cognitive desc, then name asc.
    #[test]
    fn functions_sorted_by_report_predicate() {
        // aaa: cyclomatic 1; bbb: cyclomatic 2 (one if).
        let func_a = Node::new(uast::FUNCTION)
            .with_roles(vec![
                crate::node::role::FUNCTION,
                crate::node::role::DECLARATION,
            ])
            .with_prop("name", "aaa");
        let func_b = Node::new(uast::FUNCTION)
            .with_roles(vec![
                crate::node::role::FUNCTION,
                crate::node::role::DECLARATION,
            ])
            .with_prop("name", "bbb")
            .with_children(vec![Node::new(uast::IF)]);
        let root = Node::new(uast::FILE).with_children(vec![func_a, func_b]);

        let result = Analyzer.analyze(Some(&root));
        let functions = as_array(obj_get(&result, "functions"));
        // Higher cyclomatic (bbb) comes first.
        assert_eq!(
            functions[0].as_map().unwrap().get("name").unwrap().as_str(),
            Some("bbb")
        );
    }
}
