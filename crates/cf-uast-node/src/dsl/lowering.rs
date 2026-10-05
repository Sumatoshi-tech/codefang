//! Lowering DSL AST nodes to executable query functions: a single match over
//! the [`DslNode`] enum dispatches each node kind to its lowerer.

use crate::dsl::field_access::FieldAccessExecutor;
use crate::dsl::operators::{
    apply_builtin, apply_filter, apply_map, apply_reduce, apply_rfilter, apply_rmap,
    compare_values, with_string_value,
};
use crate::dsl::runtime::{DslError, QueryFn};
use crate::node::Node;
use crate::types::{DslLiteral, DslNode};

/// A boxed transformation from a node slice to an owned node set — the same
/// shape as a [`QueryFn`], named for the sub-expressions that build one.
type NodeSetFn = QueryFn;

/// Lowers a DSL AST node into a [`QueryFn`]. Node kinds that cannot appear as
/// a query root (bare literals and comparisons) return an error.
///
/// # Errors
///
/// Returns [`DslError::UnknownNodeType`] for those non-root kinds.
pub fn lower_node(ast: &DslNode) -> Result<QueryFn, DslError> {
    match ast {
        DslNode::Map(expr) => {
            let field_fn = lower_map_expr(expr);
            Ok(Box::new(move |nodes: &[Node]| {
                apply_map(nodes, field_fn.as_ref())
            }))
        }
        DslNode::Filter(expr) => {
            let predicate = lower_filter_expr(expr);
            Ok(Box::new(move |nodes: &[Node]| {
                apply_filter(nodes, predicate.as_ref())
            }))
        }
        DslNode::Reduce(expr) => {
            // A non-call reduce body is identity (reference-implementation
            // behavior); a call-expr body runs as a fold over the whole slice.
            let body = lower_map_expr(expr);
            Ok(Box::new(move |nodes: &[Node]| {
                apply_reduce(nodes, body.as_ref())
            }))
        }
        DslNode::Field(_) => Ok(lower_field_access(ast)),
        DslNode::Call { name, args } => {
            let name = name.clone();
            let args = args.clone();
            Ok(Box::new(move |nodes: &[Node]| {
                apply_builtin(&name, nodes, &args)
            }))
        }
        DslNode::Pipeline(stages) => {
            let stage_fns: Vec<QueryFn> =
                stages.iter().filter_map(|s| lower_node(s).ok()).collect();
            Ok(Box::new(move |nodes: &[Node]| {
                apply_pipeline_stages(&stage_fns, nodes)
            }))
        }
        // The recursive stages: the same predicate / expression as `filter` /
        // `map`, applied to every node of each input's subtree in pre-order
        // (the input itself first).
        DslNode::RFilter(expr) => {
            let predicate = lower_filter_expr(expr);
            Ok(Box::new(move |nodes: &[Node]| {
                apply_rfilter(nodes, predicate.as_ref())
            }))
        }
        DslNode::RMap(expr) => {
            let f = lower_map_expr(expr);
            Ok(Box::new(move |nodes: &[Node]| {
                apply_rmap(nodes, f.as_ref())
            }))
        }
        DslNode::Literal(_) | DslNode::Comparison { .. } => {
            Err(DslError::UnknownNodeType(ast.type_name().to_string()))
        }
    }
}

/// Applies the lowered pipeline stages sequentially. A stage that failed to
/// lower was dropped at lowering time (reference-implementation behavior;
/// pinned by the differential gate).
fn apply_pipeline_stages(stages: &[QueryFn], nodes: &[Node]) -> Vec<Node> {
    let mut current = nodes.to_vec();
    for stage in stages {
        current = stage(&current);
    }
    current
}

/// Lowers the expression inside a `map(...)`: a field node lowers to field
/// access, a call lowers to a builtin, anything else is identity.
fn lower_map_expr(expr: &DslNode) -> NodeSetFn {
    match expr {
        DslNode::Field(_) => lower_field_access(expr),
        DslNode::Call { name, args } => {
            let name = name.clone();
            let args = args.clone();
            Box::new(move |nodes: &[Node]| apply_builtin(&name, nodes, &args))
        }
        _ => Box::new(|nodes: &[Node]| nodes.to_vec()),
    }
}

/// Lowers field access into a function over a node slice: applies each field
/// name in sequence (chained `.a.b`). The executor is built once here, not
/// per evaluated node — a recursive query evaluates its stages on every node
/// of the tree.
fn lower_field_access(expr: &DslNode) -> NodeSetFn {
    let fields = match expr {
        DslNode::Field(fields) => fields.clone(),
        _ => Vec::new(),
    };
    let executor = FieldAccessExecutor::new();
    Box::new(move |nodes: &[Node]| {
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

/// Lowers the expression inside a `filter(...)` into a predicate: a comparison
/// becomes a value comparison; a field access becomes a "field yields at least
/// one node" existence check; anything else is the always-true predicate.
fn lower_filter_expr(expr: &DslNode) -> Box<dyn Fn(&Node) -> bool> {
    match expr {
        // `FieldAccess has Value`: the left field access yields a node set
        // (one literal per role for `.roles`), and membership is true iff any
        // left value's token equals the right literal's token. The
        // bracketed-roles branch in `check_membership` only triggers when the
        // left side is a single `[a b]`-bracketed literal, which the
        // single-field `.roles` access never produces (it yields one literal
        // per role).
        // `.roles has Value` is the structural-selection hot path: membership
        // in the role list, with no node materialized per role.
        DslNode::Comparison { lhs, op, rhs }
            if op == "has"
                && matches!(lhs.as_ref(), DslNode::Field(f) if f.len() == 1 && f[0] == "roles") =>
        {
            let rhs_val = literal_or_field_value(rhs);
            Box::new(move |n: &Node| !rhs_val.is_empty() && n.roles.contains(&rhs_val))
        }
        DslNode::Comparison { lhs, op, rhs } if op == "has" => {
            let left_fn = lower_field_access(lhs);
            let rhs_val = literal_or_field_value(rhs);
            Box::new(move |n: &Node| check_membership(&left_fn, &rhs_val, n))
        }
        DslNode::Comparison { lhs, op, rhs } => {
            let field = field_name_of(lhs);
            let op = op.clone();
            let rhs_val = literal_or_field_value(rhs);
            Box::new(move |n: &Node| {
                with_string_value(n, &field, |left| compare_values(left, &rhs_val, &op))
            })
        }
        DslNode::Field(fields) => {
            let field = fields.first().cloned().unwrap_or_default();
            Box::new(move |n: &Node| with_string_value(n, &field, |v| !v.is_empty()))
        }
        _ => Box::new(|_n: &Node| true),
    }
}

/// Evaluates `<field> has <rhs>` for one node: the left side is the
/// field-access node set, the right side the literal `rhs`. Empty on either
/// side is false. If the left side is a single `[...]`-bracketed literal, the
/// roles are extracted from inside the brackets and matched; otherwise any
/// token equality wins.
fn check_membership(left_fn: &dyn Fn(&[Node]) -> Vec<Node>, rhs_val: &str, node: &Node) -> bool {
    let left_vals = left_fn(std::slice::from_ref(node));
    if left_vals.is_empty() || rhs_val.is_empty() {
        return false;
    }

    // Roles-membership form: a single literal whose token is `[ ... ]` (len > 2).
    if left_vals.len() == 1 {
        let tok = &left_vals[0].token;
        let bytes = tok.as_bytes();
        if bytes.len() > 2 && bytes[0] == b'[' && bytes[bytes.len() - 1] == b']' {
            // Strip brackets, split on ASCII whitespace.
            let content = &tok[1..tok.len() - 1];
            return content.split_ascii_whitespace().any(|r| r == rhs_val);
        }
    }

    // General membership: any left token equals the right token.
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
/// `.a == .b` compares field *names* — reference-implementation behavior).
fn literal_or_field_value(node: &DslNode) -> String {
    match node {
        DslNode::Literal(DslLiteral::Str(s) | DslLiteral::Number(s)) => s.clone(),
        DslNode::Literal(DslLiteral::Bool(b)) => b.to_string(),
        DslNode::Field(fields) => fields.first().cloned().unwrap_or_default(),
        _ => String::new(),
    }
}
