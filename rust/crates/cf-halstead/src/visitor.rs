//! Streaming visitor (`visitor.go`).
//!
//! The visitor drives a single-pass enter/exit traversal. On entering a function
//! node it pushes a context (with live CMS sketches); every operator/operand seen
//! while a context is active is counted into that context's maps and added to its
//! sketches. On exiting the function it finalizes the metrics: distinct/total
//! counts come from the maps (always exact); the CMS estimated totals are kept
//! only when the function reached [`crate::CMS_TOKEN_THRESHOLD`] tokens (otherwise
//! the sketches are dropped and the estimates stay 0 — the exact-only path).
//!
//! Unlike [`crate::report::build_result`]'s direct path, the visitor records one
//! entry **per function declaration**, so same-named functions are not deduped
//! (guarded by `visitor_dedup_test.go`). Per-function order here follows
//! declaration (pop) order.
//!
//! The visitor is generic over [`HalNode`]; the production wiring registers it
//! with the multi-analyzer traverser from `cf-analyze`/`cf-analyzers-common`. A
//! convenience [`Visitor::run`] is provided to traverse an owned tree directly
//! (used by tests and by callers that already hold the UAST root).

use cf_alg_cms::Sketch;
use cf_gojson::GoValue;

use crate::calculator::{HalsteadCounts, MetricsCalculator};
use crate::detector::{HalNode, OperatorOperandDetector};
use crate::formatter::ReportFormatter;
use crate::report::{
    build_empty_result, build_result, calculate_file_level_metrics, FunctionHalsteadMetrics,
};
use crate::{CMS_DELTA, CMS_EPSILON, CMS_TOKEN_THRESHOLD};

/// One in-flight function context (`halsteadContext`).
struct Context {
    metrics: FunctionHalsteadMetrics,
    operator_sketch: Option<Sketch>,
    operand_sketch: Option<Sketch>,
}

/// Streaming Halstead visitor (`Visitor`).
pub struct Visitor {
    metrics_calc: MetricsCalculator,
    detector: OperatorOperandDetector,
    /// Finalized per-function metrics, in declaration (pop) order.
    pub function_metrics: Vec<FunctionHalsteadMetrics>,
    contexts: Vec<Context>,
}

impl Default for Visitor {
    fn default() -> Self {
        Self::new()
    }
}

impl Visitor {
    /// Creates a new visitor.
    #[must_use]
    pub fn new() -> Self {
        Self {
            metrics_calc: MetricsCalculator::new(),
            detector: OperatorOperandDetector::new(),
            function_metrics: Vec::new(),
            contexts: Vec::new(),
        }
    }

    /// Whether a node begins a function (`isFunction`): a Function/Method type,
    /// or a node carrying *both* the Function and Declaration roles.
    fn is_function<N: HalNode>(node: &N) -> bool {
        matches!(node.node_type(), "Function" | "Method")
            || (node.has_any_role(&["Function"]) && node.has_any_role(&["Declaration"]))
    }

    /// Called when entering a node (`OnEnter`). `parent` is the node currently on
    /// top of the node stack before this node is pushed.
    pub fn on_enter<N: HalNode>(&mut self, node: &N, parent: Option<&N>) {
        if Self::is_function(node) {
            self.push_context(node);
        }
        if !self.contexts.is_empty() {
            self.process_node(node, parent);
        }
    }

    /// Called when exiting a node (`OnExit`).
    pub fn on_exit<N: HalNode>(&mut self, node: &N) {
        if Self::is_function(node) {
            self.pop_context();
        }
    }

    fn push_context<N: HalNode>(&mut self, function_node: &N) {
        // Extract a name: prefer a `name` prop / token on the function node, else
        // the first child name-identifier, else "anonymous". This mirrors
        // common.ExtractEntityName's effective behavior for the shapes the Go
        // tests build; the exact extractor lives in cf-analyzers-common.
        let name = extract_entity_name(function_node).unwrap_or_else(|| "anonymous".to_string());

        let metrics = FunctionHalsteadMetrics {
            name,
            ..Default::default()
        };

        let operator_sketch = Sketch::new(CMS_EPSILON, CMS_DELTA).ok();
        let operand_sketch = Sketch::new(CMS_EPSILON, CMS_DELTA).ok();

        self.contexts.push(Context {
            metrics,
            operator_sketch,
            operand_sketch,
        });
    }

    fn pop_context(&mut self) {
        let Some(mut ctx) = self.contexts.pop() else {
            return;
        };

        ctx.metrics.distinct_operators = ctx.metrics.operators.len() as i64;
        ctx.metrics.distinct_operands = ctx.metrics.operands.len() as i64;
        ctx.metrics.total_operators = self.metrics_calc.sum_map(&ctx.metrics.operators);
        ctx.metrics.total_operands = self.metrics_calc.sum_map(&ctx.metrics.operands);

        let total_tokens = ctx.metrics.total_operators + ctx.metrics.total_operands;
        if total_tokens >= CMS_TOKEN_THRESHOLD {
            if let (Some(op), Some(opnd)) = (&ctx.operator_sketch, &ctx.operand_sketch) {
                ctx.metrics.estimated_total_operators = op.total_count();
                ctx.metrics.estimated_total_operands = opnd.total_count();
                ctx.metrics.cms_active = true;
            }
        } else {
            // Below threshold: exact-only path, no estimates.
            ctx.metrics.cms_active = false;
            ctx.metrics.estimated_total_operators = 0;
            ctx.metrics.estimated_total_operands = 0;
        }

        // Finalize derived metrics.
        let d = self.metrics_calc.calculate(HalsteadCounts {
            distinct_operators: ctx.metrics.distinct_operators,
            distinct_operands: ctx.metrics.distinct_operands,
            total_operators: ctx.metrics.total_operators,
            total_operands: ctx.metrics.total_operands,
        });
        ctx.metrics.vocabulary = d.vocabulary;
        ctx.metrics.length = d.length;
        ctx.metrics.estimated_length = d.estimated_length;
        ctx.metrics.volume = d.volume;
        ctx.metrics.difficulty = d.difficulty;
        ctx.metrics.effort = d.effort;
        ctx.metrics.time_to_program = d.time_to_program;
        ctx.metrics.delivered_bugs = d.delivered_bugs;

        self.function_metrics.push(ctx.metrics);
    }

    fn process_node<N: HalNode>(&mut self, node: &N, parent: Option<&N>) {
        if self.record_operator(node) {
            return;
        }
        self.record_operand(node, parent);
    }

    fn record_operator<N: HalNode>(&mut self, node: &N) -> bool {
        if !self.detector.is_operator(node) {
            return false;
        }
        let operator = self.detector.operator_name(node);
        if operator.is_empty() {
            return true;
        }
        let ctx = self.contexts.last_mut().expect("active context");
        *ctx.metrics.operators.entry(operator.clone()).or_insert(0) += 1;
        if let Some(sketch) = &mut ctx.operator_sketch {
            sketch.add(operator.as_bytes(), 1);
        }
        true
    }

    fn record_operand<N: HalNode>(&mut self, node: &N, parent: Option<&N>) {
        if !self.detector.is_operand(node) {
            return;
        }
        // `should_count_operand` is private to the detector; reproduce its public
        // surface: skip declaration name-identifiers, require a non-empty name.
        if crate::detector::is_declaration_identifier_pub(node, parent) {
            return;
        }
        let operand = self.detector.operand_name(node);
        if operand.is_empty() {
            return;
        }
        let ctx = self.contexts.last_mut().expect("active context");
        *ctx.metrics.operands.entry(operand.clone()).or_insert(0) += 1;
        if let Some(sketch) = &mut ctx.operand_sketch {
            sketch.add(operand.as_bytes(), 1);
        }
    }

    /// Traverses an owned tree depth-first, invoking the enter/exit hooks. This
    /// is the in-crate equivalent of registering with the multi-analyzer
    /// traverser and calling `Traverse(root)`.
    pub fn run<N: HalNode>(&mut self, root: &N) {
        self.visit(root, None);
    }

    fn visit<N: HalNode>(&mut self, node: &N, parent: Option<&N>) {
        self.on_enter(node, parent);
        for child in node.children() {
            self.visit(child, Some(node));
        }
        self.on_exit(node);
    }

    /// Builds the collected report (`GetReport`): file-level aggregation plus the
    /// detailed function table and overall message, or the empty result when no
    /// functions were seen.
    #[must_use]
    pub fn get_report(self) -> GoValue {
        let formatter = ReportFormatter::new();
        if self.function_metrics.is_empty() {
            return build_empty_result("No functions found");
        }
        let file_metrics =
            calculate_file_level_metrics(&self.metrics_calc, self.function_metrics);
        let message = formatter.halstead_message(
            file_metrics.volume,
            file_metrics.difficulty,
            file_metrics.effort,
        );
        build_result(&formatter, &file_metrics, message)
    }
}

/// Best-effort entity-name extraction for the visitor's function nodes.
///
/// Mirrors the effective behavior of `common.ExtractEntityName` for the node
/// shapes used here: a `name` prop or token on the function, else the token /
/// `name` prop of a child carrying the `Name` role.
fn extract_entity_name<N: HalNode>(node: &N) -> Option<String> {
    if let Some(name) = node.prop("name") {
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    if !node.token().is_empty() {
        return Some(node.token().to_string());
    }
    for child in node.children() {
        if child.has_any_role(&["Name"]) {
            if !child.token().is_empty() {
                return Some(child.token().to_string());
            }
            if let Some(name) = child.prop("name") {
                if !name.is_empty() {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detector::test_support::TestNode;

    fn simple_function(name: &str) -> TestNode {
        TestNode::new("Function")
            .with_roles(&["Function", "Declaration"])
            .child(
                TestNode::new("Identifier")
                    .with_token(name)
                    .with_roles(&["Name"]),
            )
            .child(
                TestNode::new("BinaryOp")
                    .with_roles(&["Operator"])
                    .with_prop("operator", "+"),
            )
            .child(
                TestNode::new("Identifier")
                    .with_token("a")
                    .with_roles(&["Variable"]),
            )
            .child(
                TestNode::new("Identifier")
                    .with_token("b")
                    .with_roles(&["Variable"]),
            )
    }

    /// Ported from `TestVisitor_Basic`: one function with 1 operator, 2 operands.
    #[test]
    fn visitor_basic() {
        let root = TestNode::new("File").child(simple_function("simpleFunction"));
        let mut v = Visitor::new();
        v.run(&root);
        assert_eq!(v.function_metrics.len(), 1);
        let fnm = &v.function_metrics[0];
        assert_eq!(fnm.name, "simpleFunction");
        assert_eq!(fnm.operators.len(), 1);
        assert_eq!(fnm.operands.len(), 2);
        assert!(fnm.volume > 0.0);
    }

    /// Ported from `TestVisitor_CountsAllSameNameFunctions`: same-named functions
    /// must each be recorded, not deduped.
    #[test]
    fn counts_all_same_name_functions() {
        let dup_count = 5;
        let mut root = TestNode::new("File");
        for _ in 0..dup_count {
            root = root.child(
                TestNode::new("Function").with_roles(&["Function", "Declaration"]).child(
                    TestNode::new("Identifier").with_token("Read").with_roles(&["Name"]),
                ),
            );
        }
        let mut v = Visitor::new();
        v.run(&root);
        assert_eq!(v.function_metrics.len(), dup_count);

        let report = v.get_report();
        let GoValue::Object(m) = &report else {
            panic!("expected object")
        };
        assert_eq!(m.get("total_functions"), Some(&GoValue::Int(dup_count as i64)));
        let GoValue::Array(functions) = m.get("functions").expect("functions") else {
            panic!("expected functions array")
        };
        assert_eq!(functions.len(), dup_count);
    }

    /// Ported from `TestVisitor_CMSSketchPopulated_LargeFunction` /
    /// `_CMSTotalMatchesExact`.
    #[test]
    fn cms_large_function_matches_exact() {
        let mut function =
            TestNode::new("Function").with_roles(&["Function", "Declaration"]).child(
                TestNode::new("Identifier").with_token("big").with_roles(&["Name"]),
            );
        for i in 0..2000 {
            if i % 2 == 0 {
                function = function.child(
                    TestNode::new("BinaryOp")
                        .with_roles(&["Operator"])
                        .with_prop("operator", &format!("op{}", i % 10)),
                );
            } else {
                function = function.child(
                    TestNode::new("Identifier")
                        .with_token(&format!("var{}", i % 20))
                        .with_roles(&["Variable"]),
                );
            }
        }
        let root = TestNode::new("File").child(function);
        let mut v = Visitor::new();
        v.run(&root);
        assert_eq!(v.function_metrics.len(), 1);
        let fnm = &v.function_metrics[0];
        assert!(fnm.cms_active);
        assert_eq!(fnm.estimated_total_operators, fnm.total_operators);
        assert_eq!(fnm.estimated_total_operands, fnm.total_operands);
        assert!(fnm.estimated_total_operators > 0);
        assert!(fnm.estimated_total_operands > 0);
        // distinct counts come from the maps (exact).
        assert_eq!(fnm.distinct_operators, fnm.operators.len() as i64);
        assert_eq!(fnm.distinct_operands, fnm.operands.len() as i64);
    }

    /// Ported from `TestVisitor_CMSNotUsed_SmallFunction`.
    #[test]
    fn cms_small_function_inactive() {
        let mut function =
            TestNode::new("Function").with_roles(&["Function", "Declaration"]).child(
                TestNode::new("Identifier").with_token("small").with_roles(&["Name"]),
            );
        for i in 0..50 {
            if i % 2 == 0 {
                function = function.child(
                    TestNode::new("BinaryOp").with_roles(&["Operator"]).with_prop("operator", &format!("op{}", i % 10)),
                );
            } else {
                function = function.child(
                    TestNode::new("Identifier").with_token(&format!("var{}", i % 20)).with_roles(&["Variable"]),
                );
            }
        }
        let root = TestNode::new("File").child(function);
        let mut v = Visitor::new();
        v.run(&root);
        let fnm = &v.function_metrics[0];
        assert!(!fnm.cms_active);
        assert_eq!(fnm.estimated_total_operators, 0);
        assert_eq!(fnm.estimated_total_operands, 0);
    }
}
