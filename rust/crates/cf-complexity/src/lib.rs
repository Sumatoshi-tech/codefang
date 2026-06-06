//! Static complexity analyzer (cyclomatic / cognitive / nesting).
//!
//! Faithful port of the Go package `internal/analyzers/complexity`
//! (`complexity.go`, `cognitive_complexity.go`, `flow_helpers.go`). It walks a
//! UAST tree, computes per-function cyclomatic complexity, cognitive complexity
//! (SonarSource methodology), nesting depth, decision points, and lines of
//! code, and returns a deterministic result map matching Go.
//!
//! # Output shape (mirrors `(*Analyzer).Analyze` -> `analyze.Report`)
//!
//! On success the returned [`cf_gojson::GoValue`] object carries the keys
//! `analyzer_name`, `total_functions`, `average_complexity`, `max_complexity`,
//! `total_complexity`, `cognitive_complexity`, `nesting_depth`,
//! `decision_points`, `functions`, and `message`. The `functions` array holds
//! one object per function with the per-function metric keys plus the
//! assessment strings (`complexity_assessment`, `cognitive_assessment`,
//! `nesting_assessment`), exactly as `convertFunctionReportItems` builds them.
//!
//! For a nil root or a tree with no functions, the empty-result shape is
//! returned (`total_functions`, `average_complexity`, `max_complexity`,
//! `total_complexity`, `message`), mirroring `buildEmptyResult`.
//!
//! Because [`cf_gojson::GoValue`] objects are map-origin here, their keys
//! byte-sort on encode, exactly as Go's `encoding/json` orders `map[string]any`
//! keys. Serialization itself is owned by the report layer; this crate only
//! builds the value tree.
//!
//! # Differences from the framework path
//!
//! The Go analyzer's `FormatReportJSON`/`FormatReportYAML`/`FormatReportBinary`
//! derive a separate `ComputedMetrics` view; that rendering view and the
//! visitor/aggregator streaming path are not ported here (they depend on
//! not-yet-ported framework crates). See the crate todos.

pub mod gosort;
pub mod node;
pub mod report;

use cf_gojson::{GoMap, GoValue, MapOrigin};
use node::{uast, Node};

/// The complexity analyzer. Stateless, like Go's `Analyzer` struct (its Go
/// fields are traverser/extractor helpers with no configurable state).
#[derive(Debug, Default, Clone, Copy)]
pub struct Analyzer;

/// Per-function complexity metrics. Mirrors Go's `FunctionMetrics`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionMetrics {
    /// Function name (or `"anonymous"`).
    pub name: String,
    /// Cyclomatic complexity (1 + decision points).
    pub cyclomatic_complexity: i64,
    /// Cognitive complexity (SonarSource, nesting-weighted).
    pub cognitive_complexity: i64,
    /// Maximum nesting depth.
    pub nesting_depth: i64,
    /// Decision points (`max(cyclomatic-1, 0)`).
    pub decision_points: i64,
    /// Estimated lines of code.
    pub lines_of_code: i64,
}

const ANONYMOUS_FUNCTION_NAME: &str = "anonymous";

// Threshold constants, mirroring complexity.go.
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
    /// Analyzer name, matching Go's `(*Analyzer).Name`.
    #[must_use]
    pub fn name(&self) -> &'static str {
        "complexity"
    }

    /// CLI flag, matching Go's `(*Analyzer).Flag`.
    #[must_use]
    pub fn flag(&self) -> &'static str {
        "complexity-analysis"
    }

    /// Performs complexity analysis, mirroring `(*Analyzer).Analyze`.
    ///
    /// Returns the report object as a [`GoValue`].
    #[must_use]
    pub fn analyze(&self, root: Option<&Node>) -> GoValue {
        let root = match root {
            Some(r) => r,
            None => return build_empty_result("No AST provided"),
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

    /// Computes per-function metrics (in Go's deterministic sorted order)
    /// without building the report map. Useful for the quality analyzer and
    /// for direct testing.
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

/// Aggregated totals across all functions, mirroring Go's `totals` map.
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
        fo.insert("cyclomatic_complexity", GoValue::Int(fm.cyclomatic_complexity));
        fo.insert("cognitive_complexity", GoValue::Int(fm.cognitive_complexity));
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

/// Finds all function nodes, mirroring `findFunctions` + `isFunctionNode`.
///
/// Go collects nodes by type (`Function`, `Method`) and by role (`Function`),
/// deduplicates via a set, then keeps those satisfying `isFunctionNode`. The Go
/// set iteration order is nondeterministic, but the caller sorts the resulting
/// metrics deterministically, so we collect in a stable pre-order with identity
/// dedup; the final order is fixed by [`calculate_all_function_metrics`].
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

/// Mirrors `isFunctionNode`: a `Function`/`Method` type, OR both `Function` and
/// `Declaration` roles.
fn is_function_node(n: &Node) -> bool {
    n.has_any_type(&[uast::FUNCTION, uast::METHOD])
        || n.has_all_roles(&[node::role::FUNCTION, node::role::DECLARATION])
}

/// Mirrors `calculateAllFunctionMetrics`: per-function metrics + totals, with
/// the exact Go sort (cyclomatic desc, then cognitive desc, then name asc).
fn calculate_all_function_metrics(functions: &[&Node]) -> (Vec<FunctionMetrics>, Totals) {
    let mut metrics: Vec<FunctionMetrics> = Vec::with_capacity(functions.len());
    let mut totals = Totals::default();

    for fn_node in functions {
        let m = calculate_function_metrics(fn_node);
        update_totals(&mut totals, &m);
        metrics.push(m);
    }

    metrics.sort_by(|left, right| {
        if left.cyclomatic_complexity != right.cyclomatic_complexity {
            return right.cyclomatic_complexity.cmp(&left.cyclomatic_complexity);
        }
        if left.cognitive_complexity != right.cognitive_complexity {
            return right.cognitive_complexity.cmp(&left.cognitive_complexity);
        }
        left.name.cmp(&right.name)
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

/// Mirrors `calculateFunctionMetrics`.
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

// --- Cyclomatic complexity (mirrors calculateCyclomaticComplexity) ---

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

/// Builds the source context from a function node (Go `newFunctionSourceContext`).
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

/// Mirrors `isDecisionPointWithSource`: like [`is_decision_point`] but recovers a
/// `BinaryOp`'s operator from the function source slice when token/props omit it.
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

/// Mirrors `isDecisionPoint` (the AST-metadata variant used by the analyzer's
/// own tests): reads the `BinaryOp` operator only from `Props["operator"]`.
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

// --- Cognitive complexity (faithful port of CognitiveComplexityCalculator) ---
//
// Ports `cognitive_complexity.go` (the SonarSource / gocognit model). The Go
// calculator walks the function's children with a `walkNode` recursion that
// tracks structural nesting, recovers binary operators (from token, props, or
// the function source slice via `functionSourceContext`), and counts:
//   * a `nesting + 1` increment for each `if` (non-else-if), loop, switch, try,
//     catch, match (structural + nesting penalty);
//   * a flat `+1` for an `else if` and for an `else` block;
//   * logical-operator *sequence* increments (`addLogicalSequenceComplexity`):
//     +1 for the first run of logical operators in an if condition, +1 for each
//     change of operator kind along the flattened operator stream;
//   * `+1` for a recursive call (call whose name equals the function name);
//   * lambda bodies raise nesting by one.

/// Holds the function's original source bytes and its start offset, so binary
/// operators can be recovered from the source when the UAST omits them. Mirrors
/// Go's `functionSourceContext`.
struct FunctionSourceContext<'a> {
    source: &'a [u8],
    start_offset: u32,
}

fn calculate_cognitive_complexity(fn_node: &Node) -> i64 {
    let ctx = function_source_context(fn_node);
    let function_name = extract_cognitive_function_name(fn_node);

    let mut complexity: i64 = 0;
    for (idx, child) in fn_node.children.iter().enumerate() {
        walk_node(child, fn_node, idx, 0, &ctx, &function_name, &mut complexity);
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
            process_if_node(curr, parent, child_idx, nesting, ctx, function_name, complexity);
            return;
        }
        uast::LOOP | uast::SWITCH | uast::TRY | uast::CATCH | uast::MATCH => {
            *complexity += nesting + 1;
            for (idx, child) in curr.children.iter().enumerate() {
                walk_node(child, curr, idx, nesting + 1, ctx, function_name, complexity);
            }
            return;
        }
        uast::LAMBDA => {
            for (idx, child) in curr.children.iter().enumerate() {
                walk_node(child, curr, idx, nesting + 1, ctx, function_name, complexity);
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
        walk_node(&if_node.children[0], if_node, 0, nesting, ctx, function_name, complexity);
    }

    if if_node.children.len() > 1 {
        walk_node(&if_node.children[1], if_node, 1, nesting + 1, ctx, function_name, complexity);
    }

    for idx in 2..if_node.children.len() {
        let child = &if_node.children[idx];
        match child.node_type.as_str() {
            uast::IF => {
                walk_node(child, if_node, idx, nesting, ctx, function_name, complexity);
            }
            uast::BLOCK => {
                // Sonar/gocognit: an `else` branch adds one structural increment.
                *complexity += 1;
                walk_node(child, if_node, idx, nesting, ctx, function_name, complexity);
            }
            _ => {
                walk_node(child, if_node, idx, nesting, ctx, function_name, complexity);
            }
        }
    }
}

/// Mirrors `addLogicalSequenceComplexity`: +1 for the first logical-operator run
/// in the condition, +1 for each change of operator kind along the flattened
/// left-to-right operator stream.
fn add_logical_sequence_complexity(
    expr: &Node,
    ctx: &FunctionSourceContext,
    complexity: &mut i64,
) {
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
            last_op = op.clone();
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
    /// Mirrors `functionSourceContext.binaryOperator`: token, then props, then a
    /// best-effort recovery from the source slice between the operands.
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

/// Mirrors `extractFunctionName` for the cognitive calculator: name from the
/// entity-name helpers/props, used only for recursive-call detection. We use the
/// shared prop-based extraction (the `common.ExtractEntityName` fast path
/// resolves to the same name-prop / name-role token for the analyzer's inputs).
fn extract_cognitive_function_name(fn_node: &Node) -> String {
    let n = extract_function_name(fn_node);
    if n == ANONYMOUS_FUNCTION_NAME {
        String::new()
    } else {
        n
    }
}

fn is_recursive_call(call_node: &Node, function_name: &str) -> bool {
    if function_name.is_empty() {
        return false;
    }
    let call_name = extract_call_name(call_node);
    !call_name.is_empty() && call_name == function_name
}

/// Mirrors `extractCallName`.
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

/// Mirrors `normalizeOperatorText`: trim, strip all whitespace, then keep only
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

// --- Nesting depth (mirrors calculateNestingDepth) ---

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

/// Mirrors `isNestingNode`: `If`, `Loop`, `Switch`, `Try`, `Catch`.
fn is_nesting_node(target: &Node) -> bool {
    matches!(
        target.node_type.as_str(),
        uast::IF | uast::LOOP | uast::SWITCH | uast::TRY | uast::CATCH
    )
}

// --- Lines of code (mirrors estimateLinesOfCode) ---

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

// --- Name extraction (mirrors extractFunctionName, simplified) ---
//
// The Go path consults common.ExtractEntityName and a DataExtractor before
// falling back to props and to a Name-role child token. Those helpers are not
// yet ported (cf-analyzers-common); this reproduces the prop-based and
// Name-role-child branches plus the "anonymous" fallback, which covers the
// analyzer's own tests. See crate todos.
fn extract_function_name(fn_node: &Node) -> String {
    if let Some(n) = extract_name_from_props(fn_node) {
        return n;
    }
    // Name-role child token (mirrors the FindNodesByRoles(RoleName) fallback).
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

// --- flow_helpers.go ---

/// Mirrors `isDefaultCase`.
fn is_default_case(n: &Node) -> bool {
    if n.node_type != uast::CASE {
        return false;
    }
    if n.token.trim().starts_with("default") {
        return true;
    }
    n.children
        .iter()
        .any(|c| c.token.trim().starts_with("default"))
}

/// Mirrors `isElseIfNode`: an `If` nested as a non-first child of another `If`.
fn is_else_if_node(parent: Option<&Node>, curr: &Node, child_idx: usize) -> bool {
    let parent = match parent {
        Some(p) => p,
        None => return false,
    };
    if parent.node_type != uast::IF || curr.node_type != uast::IF {
        return false;
    }
    child_idx > 0
}

/// Mirrors `isLogicalOperatorToken` (flow_helpers.go): also accepts the
/// uppercase `AND`/`OR` forms used by some languages.
fn is_logical_operator_token(op: &str) -> bool {
    matches!(op.trim(), "&&" | "||" | "and" | "or" | "AND" | "OR")
}

/// Mirrors `isElseIfNode` for the cognitive calculator: an `If` nested as the
/// third-or-later child (index >= 2) of another `If` (the else-if slot).
fn is_else_if_cognitive(parent: &Node, child: &Node, child_idx: usize) -> bool {
    parent.node_type == uast::IF && child.node_type == uast::IF && child_idx >= 2
}

// --- Assessment / message helpers (mirror complexity.go) ---

fn get_complexity_level(complexity: i64) -> &'static str {
    if complexity <= CYCLOMATIC_GREEN {
        "green"
    } else if complexity <= CYCLOMATIC_YELLOW {
        "yellow"
    } else {
        "red"
    }
}

fn get_complexity_assessment(complexity: i64) -> &'static str {
    match get_complexity_level(complexity) {
        "green" => "🟢 Simple",
        "yellow" => "🟡 Moderate",
        "red" => "🔴 Complex",
        _ => "⚪ Unknown",
    }
}

fn get_cognitive_assessment(complexity: i64) -> &'static str {
    if complexity <= COMPLEXITY_THRESHOLD_HIGH {
        "🟢 Low"
    } else if complexity <= MAGIC10 {
        "🟡 Medium"
    } else {
        "🔴 High"
    }
}

fn get_nesting_assessment(depth: i64) -> &'static str {
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

    /// Mirrors `TestAnalyzer_Basic` (name).
    #[test]
    fn analyzer_basic_name() {
        assert_eq!(Analyzer.name(), "complexity");
    }

    /// Mirrors `TestAnalyzer_MetadataAndFormatting` flag check.
    #[test]
    fn analyzer_flag() {
        assert_eq!(Analyzer.flag(), "complexity-analysis");
    }

    /// Mirrors `TestAnalyzer_NilRoot`.
    #[test]
    fn nil_root_total_functions_zero() {
        let result = Analyzer.analyze(None);
        assert_eq!(as_int(obj_get(&result, "total_functions")), 0);
    }

    /// Mirrors `TestAnalyzer_SimpleFunction`.
    #[test]
    fn simple_function() {
        let mut function_node =
            Node::new(uast::FUNCTION).with_roles(vec![crate::node::role::FUNCTION, crate::node::role::DECLARATION]);
        let name_node =
            Node::with_token(uast::IDENTIFIER, "simpleFunction").with_roles(vec![crate::node::role::NAME]);
        function_node.add_child(name_node);

        let mut root = Node::new(uast::FILE);
        root.add_child(function_node);

        let result = Analyzer.analyze(Some(&root));
        assert_eq!(as_int(obj_get(&result, "total_functions")), 1);
        assert_eq!(as_int(obj_get(&result, "total_complexity")), 1);
    }

    /// Mirrors `TestAnalyzer_ExtractFunctionName`.
    #[test]
    fn extract_function_name_with_and_without_name() {
        let mut function_node = Node::new(uast::FUNCTION);
        let name_node =
            Node::with_token(uast::IDENTIFIER, "testFunction").with_roles(vec![crate::node::role::NAME]);
        function_node.add_child(name_node);
        assert_eq!(extract_function_name(&function_node), "testFunction");

        let anon = Node::new(uast::FUNCTION);
        assert_eq!(extract_function_name(&anon), "anonymous");
    }

    /// Mirrors `TestAnalyzer_IsDecisionPoint`.
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

    /// Mirrors `TestAnalyzer_WithIfStatement` (total_complexity == 2).
    #[test]
    fn with_if_statement() {
        let mut function_node =
            Node::new(uast::FUNCTION).with_roles(vec![crate::node::role::FUNCTION, crate::node::role::DECLARATION]);
        function_node.add_child(
            Node::with_token(uast::IDENTIFIER, "testFunction").with_roles(vec![crate::node::role::NAME]),
        );
        function_node.add_child(Node::new(uast::IF).with_roles(vec![crate::node::role::CONDITION]));

        let mut root = Node::new(uast::FILE);
        root.add_child(function_node);

        let result = Analyzer.analyze(Some(&root));
        assert_eq!(as_int(obj_get(&result, "total_complexity")), 2);
    }

    /// Mirrors `TestCognitiveComplexityCalculator_NestedStructures`. The Go
    /// `processIfNode` walks an `If`'s first child (its condition slot) at the
    /// SAME nesting level, so a nested `If` placed at index 0 receives no nesting
    /// penalty: outer `if` (+1) + inner `if` at nesting 0 (+1) = 2. Verified
    /// against the Go `CognitiveComplexityCalculator` for this exact tree.
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
    /// body, reproducing the SonarSource model exactly as Go does. The loop sits
    /// in the outer-if's first-child (condition) slot, walked at nesting 0; its
    /// inner `if` is in the loop's first-child slot, walked at nesting 1.
    /// cognitive = if(+1) + loop(+1) + inner-if(nesting 1 → +2) = 4. Verified
    /// against the Go `CognitiveComplexityCalculator`.
    #[test]
    fn nested_if_loop_if_metrics() {
        let inner_if = Node::new(uast::IF);
        let loop_node = Node::new(uast::LOOP).with_children(vec![inner_if]);
        let outer_if = Node::new(uast::IF).with_children(vec![loop_node]);
        let func = Node::new(uast::FUNCTION)
            .with_roles(vec![crate::node::role::FUNCTION, crate::node::role::DECLARATION])
            .with_children(vec![outer_if]);

        // cyclomatic: 1 + if + loop + if = 4
        assert_eq!(calculate_cyclomatic_complexity(&func), 4);
        // cognitive: if(+1) + loop(+1) + inner-if@nesting1(+2) = 4
        assert_eq!(calculate_cognitive_complexity(&func), 4);
        // nesting: if->loop->if = 3
        assert_eq!(calculate_nesting_depth(&func), 3);
    }

    /// Else-if chains are not counted as additional nesting (mirrors
    /// `isElseIfNode`).
    #[test]
    fn else_if_does_not_increase_nesting() {
        // outer if with child0 = block, child1 = else-if; the else-if must not
        // add nesting.
        let else_if = Node::new(uast::IF);
        let block = Node::new(uast::BLOCK);
        let outer = Node::new(uast::IF).with_children(vec![block, else_if]);
        let func = Node::new(uast::FUNCTION)
            .with_roles(vec![crate::node::role::FUNCTION])
            .with_children(vec![outer]);
        // Only the outer if nests => depth 1.
        assert_eq!(calculate_nesting_depth(&func), 1);
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

    /// The success result carries the Go report keys, including assessments.
    #[test]
    fn result_shape_has_go_keys_and_assessments() {
        let mut func =
            Node::new(uast::FUNCTION).with_roles(vec![crate::node::role::FUNCTION, crate::node::role::DECLARATION]);
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
    fn functions_sorted_by_go_predicate() {
        // aaa: cyclomatic 1; bbb: cyclomatic 2 (one if).
        let func_a = Node::new(uast::FUNCTION)
            .with_roles(vec![crate::node::role::FUNCTION, crate::node::role::DECLARATION])
            .with_prop("name", "aaa");
        let func_b = Node::new(uast::FUNCTION)
            .with_roles(vec![crate::node::role::FUNCTION, crate::node::role::DECLARATION])
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
