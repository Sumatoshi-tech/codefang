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

pub mod node;

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
    let mut complexity: i64 = 1;
    fn_node.visit_pre_order(&mut |n| {
        if std::ptr::eq(n, fn_node) {
            return;
        }
        if is_decision_point(n) {
            complexity += 1;
        }
    });
    complexity
}

/// Mirrors `isDecisionPoint` (the AST-metadata variant used by tests and by
/// `isDecisionPointWithSource` once the source operator is read from
/// `Props["operator"]`).
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

// --- Cognitive complexity (mirrors CognitiveComplexityCalculator) ---

fn calculate_cognitive_complexity(fn_node: &Node) -> i64 {
    let mut complexity: i64 = 0;
    let mut nesting_level: i64 = 0;
    calculate_recursive(fn_node, &mut complexity, &mut nesting_level);
    complexity
}

fn calculate_recursive(n: &Node, complexity: &mut i64, nesting_level: &mut i64) {
    let nesting_change = process_node(n, complexity, *nesting_level);
    if nesting_change {
        *nesting_level += 1;
    }
    for child in &n.children {
        calculate_recursive(child, complexity, nesting_level);
    }
    if nesting_change {
        *nesting_level -= 1;
    }
}

/// Mirrors `processNode`: returns whether this node increases the nesting level.
fn process_node(n: &Node, complexity: &mut i64, nesting_level: i64) -> bool {
    match n.node_type.as_str() {
        // handleIfNode and the loop/switch arm both add `1 + nestingLevel`.
        uast::IF | uast::LOOP | uast::SWITCH => {
            *complexity += 1 + nesting_level;
            true
        }
        uast::CATCH => {
            *complexity += 1;
            true
        }
        uast::BINARY_OP => {
            handle_binary_op(n, complexity);
            false
        }
        _ => false,
    }
}

fn handle_binary_op(n: &Node, complexity: &mut i64) {
    let op = n.prop("operator").unwrap_or("");
    if op.is_empty() {
        return;
    }
    if is_logical_operator_token(op) {
        *complexity += 1;
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

/// Mirrors `isLogicalOperatorToken`.
fn is_logical_operator_token(op: &str) -> bool {
    matches!(op.trim(), "&&" | "||" | "and" | "or")
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

    /// Mirrors `TestCognitiveComplexityCalculator_NestedStructures`
    /// (nested ifs => cognitive >= 2).
    #[test]
    fn cognitive_nested_ifs() {
        let inner_if = Node::new(uast::IF).with_roles(vec![crate::node::role::CONDITION]);
        let outer_if = Node::new(uast::IF)
            .with_roles(vec![crate::node::role::CONDITION])
            .with_children(vec![inner_if]);
        let function_node = Node::new(uast::FUNCTION)
            .with_roles(vec![crate::node::role::FUNCTION])
            .with_children(vec![outer_if]);

        // if@0 -> +1, if(nested) -> +2 => 3 (>= 2).
        assert_eq!(calculate_cognitive_complexity(&function_node), 3);
    }

    /// Cyclomatic/cognitive/nesting parity for the canonical `if{loop{if}}`
    /// body, reproducing the SonarSource nesting weights from the Go
    /// calculator.
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
        // cognitive: if(1+0) + loop(1+1) + if(1+2) = 1+2+3 = 6
        assert_eq!(calculate_cognitive_complexity(&func), 6);
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
