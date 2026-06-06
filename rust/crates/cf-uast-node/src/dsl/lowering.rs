//! Lowering DSL AST nodes to executable query functions. Ported from
//! `lowering.go`. The Go code uses a registry of `DSLNodeLowerer`
//! implementations keyed by `DSLNodeType`; Rust uses a single match over the
//! [`DslNode`] enum, which is the direct equivalent of Go's type switch.

use crate::dsl::field_access::FieldAccessExecutor;
use crate::dsl::operators::{
    apply_builtin, apply_filter, apply_map, apply_reduce, compare_values, extract_string_value,
};
use crate::dsl::runtime::{DslError, QueryFn};
use crate::node::Node;
use crate::types::{DslLiteral, DslNode};

/// Lowers a DSL AST node into a [`QueryFn`]. Mirrors `LowerDSL` / the registry's
/// `Lower`. Unknown node kinds (only `Comparison` appears as a bare AST root,
/// which Go never lowers directly) return an error.
pub(crate) fn lower_node(ast: &DslNode) -> Result<QueryFn, DslError> {
    match ast {
        DslNode::Map(expr) => {
            let field_fn = lower_map_expr(expr);
            Ok(Box::new(move |nodes: Vec<Node>| apply_map(&nodes, field_fn.as_ref())))
        }
        DslNode::Filter(expr) => {
            let predicate = lower_filter_expr(expr);
            Ok(Box::new(move |nodes: Vec<Node>| apply_filter(&nodes, predicate.as_ref())))
        }
        DslNode::Reduce(expr) => {
            // Go's ReduceNodeLowerer is identity; preserve that, but still allow
            // a call-expr reduce body to run as a fold over the whole slice.
            let body = lower_map_expr(expr);
            Ok(Box::new(move |nodes: Vec<Node>| apply_reduce(&nodes, body.as_ref())))
        }
        DslNode::Field(_) => {
            let f = lower_field_access(ast);
            Ok(Box::new(move |nodes: Vec<Node>| f(&nodes)))
        }
        DslNode::Call { name, args } => {
            let name = name.clone();
            let args = args.clone();
            Ok(Box::new(move |nodes: Vec<Node>| apply_builtin(&name, &nodes, &args)))
        }
        DslNode::Pipeline(stages) => {
            let stages = stages.clone();
            Ok(Box::new(move |nodes: Vec<Node>| apply_pipeline_stages(&stages, nodes)))
        }
        // RMap/RFilter lower to identity in Go.
        DslNode::RMap(_) | DslNode::RFilter(_) => Ok(Box::new(|nodes: Vec<Node>| nodes)),
        DslNode::Literal(_) | DslNode::Comparison { .. } => {
            Err(DslError::UnknownNodeType(ast.type_name().to_string()))
        }
    }
}

/// Applies pipeline stages sequentially. Mirrors `applyPipelineStages` (a stage
/// that fails to lower is skipped, matching Go's `if err != nil { continue }`).
fn apply_pipeline_stages(stages: &[DslNode], nodes: Vec<Node>) -> Vec<Node> {
    let mut current = nodes;
    for stage in stages {
        if let Ok(stage_fn) = lower_node(stage) {
            current = stage_fn(current);
        }
    }
    current
}

/// Lowers the expression inside a `map(...)`. Mirrors `lowerMapExpr`: a field
/// node lowers to field access, a call lowers to a builtin, anything else is
/// identity.
fn lower_map_expr(expr: &DslNode) -> Box<dyn Fn(&[Node]) -> Vec<Node>> {
    match expr {
        DslNode::Field(_) => {
            let f = lower_field_access(expr);
            Box::new(move |nodes: &[Node]| f(nodes))
        }
        DslNode::Call { name, args } => {
            let name = name.clone();
            let args = args.clone();
            Box::new(move |nodes: &[Node]| apply_builtin(&name, nodes, &args))
        }
        _ => Box::new(|nodes: &[Node]| nodes.to_vec()),
    }
}

/// Lowers field access into a function over a node slice. Mirrors
/// `lowerFieldAccess`: applies each field name in sequence (chained `.a.b`).
fn lower_field_access(expr: &DslNode) -> Box<dyn Fn(&[Node]) -> Vec<Node>> {
    let fields = match expr {
        DslNode::Field(fields) => fields.clone(),
        _ => Vec::new(),
    };
    Box::new(move |nodes: &[Node]| {
        let executor = FieldAccessExecutor::new();
        let mut current = nodes.to_vec();
        for field in &fields {
            let mut next = Vec::new();
            for n in &current {
                next.extend(executor.execute(n, field));
            }
            current = next;
        }
        current
    })
}

/// Lowers the expression inside a `filter(...)` into a predicate. Mirrors
/// `lowerFilterExpr`: a comparison becomes a value comparison; a field access
/// becomes a "field yields at least one node" existence check; anything else is
/// the always-true predicate.
fn lower_filter_expr(expr: &DslNode) -> Box<dyn Fn(&Node) -> bool> {
    match expr {
        // `FieldAccess has Value` (Go `Membership` → CallNode{"has"}). Mirrors
        // `CheckMembership`: the left field access yields a node set (one literal
        // per role for `.roles`), and membership is true iff any left value's
        // token equals the right literal's token (`checkGeneralMembership`). The
        // `checkRolesMembership` branch only triggers when the left side is a
        // single `[a b]`-bracketed literal, which the single-field `.roles`
        // access never produces (it yields one literal per role).
        DslNode::Comparison { lhs, op, rhs } if op == "has" => {
            let left_fn = lower_field_access(lhs);
            let rhs_val = literal_or_field_value(rhs);
            Box::new(move |n: &Node| {
                check_membership(&left_fn, &rhs_val, n)
            })
        }
        DslNode::Comparison { lhs, op, rhs } => {
            let field = field_name_of(lhs);
            let op = op.clone();
            let rhs_val = literal_or_field_value(rhs);
            Box::new(move |n: &Node| {
                let left = extract_string_value(n, &field);
                compare_values(&left, &rhs_val, &op)
            })
        }
        DslNode::Field(fields) => {
            let field = fields.first().cloned().unwrap_or_default();
            Box::new(move |n: &Node| !extract_string_value(n, &field).is_empty())
        }
        _ => Box::new(|_n: &Node| true),
    }
}

/// Evaluates `<field> has <rhs>` for one node. Mirrors Go's
/// `FieldAccessManager.CheckMembership`: `leftFunc(node)` is the field-access
/// node set, `rightFunc(nil)` is the single literal `rhs`. Empty on either side
/// is false. If the left side is a single `[...]`-bracketed literal, the roles
/// are extracted and matched (`checkRolesMembership`); otherwise any token
/// equality wins (`checkGeneralMembership`).
fn check_membership(left_fn: &dyn Fn(&[Node]) -> Vec<Node>, rhs_val: &str, node: &Node) -> bool {
    let left_vals = left_fn(std::slice::from_ref(node));
    if left_vals.is_empty() || rhs_val.is_empty() {
        return false;
    }

    // `isRolesMembership`: a single literal whose token is `[ ... ]` (len > 2).
    if left_vals.len() == 1 {
        let tok = &left_vals[0].token;
        let bytes = tok.as_bytes();
        if bytes.len() > 2 && bytes[0] == b'[' && bytes[bytes.len() - 1] == b']' {
            // `extractRoles`: strip brackets, split on ASCII whitespace.
            let content = &tok[1..tok.len() - 1];
            return content.split_ascii_whitespace().any(|r| r == rhs_val);
        }
    }

    // `checkGeneralMembership`: any left token equals the right token.
    left_vals.iter().any(|lv| lv.token == rhs_val)
}

/// Extracts the leading field name from a field-access (or empty).
fn field_name_of(node: &DslNode) -> String {
    match node {
        DslNode::Field(fields) => fields.first().cloned().unwrap_or_default(),
        _ => String::new(),
    }
}

/// Resolves the right-hand side of a comparison to a comparable string: a
/// literal yields its text, a field access yields its leading field name (so
/// `.a == .b` compares field *names*, matching Go's string-based comparator).
fn literal_or_field_value(node: &DslNode) -> String {
    match node {
        DslNode::Literal(DslLiteral::Str(s)) => s.clone(),
        DslNode::Literal(DslLiteral::Number(s)) => s.clone(),
        DslNode::Literal(DslLiteral::Bool(b)) => b.to_string(),
        DslNode::Field(fields) => fields.first().cloned().unwrap_or_default(),
        _ => String::new(),
    }
}
