//! Cohesion visitor: the traversal the static pipeline actually runs.
//!
//! The static pipeline drives the cohesion analyzer with a preorder DFS and a
//! context stack. This traversal is part of the report contract (pinned by the
//! differential gate).
//!
//! Key differences from the
//! [`Analyzer::analyze`](crate::analyzer::Analyzer::analyze) path:
//!
//! * Function detection is type Function/Method OR roles
//!   {Function AND Declaration}, not role Function OR type Function/Method.
//! * Variables are attributed to the **innermost** enclosing function on the
//!   context stack, via the variable-node processing on every visited node
//!   while a context is active. A variable inside a nested function therefore
//!   belongs only to that nested function, never to the outer one (the
//!   `find_functions` path would attribute it to both).
//! * Functions are emitted in exit order (a function is appended when its
//!   subtree walk completes).

use crate::analyzer::{Analyzer, Function};
use crate::uast::Node;

/// One entry on the visitor's context stack: the function being built and the
/// variables collected so far for it.
struct Ctx {
    function: Function,
}

/// Collects functions from `root` with the context-stack preorder traversal.
///
/// Cohesion of each returned [`Function`] is left at `0.0`; the caller fills it
/// via [`Analyzer::compute_per_function_cohesion`].
pub(crate) fn collect_functions_via_visitor<N: Node>(analyzer: &Analyzer, root: &N) -> Vec<Function> {
    let mut stack: Vec<Ctx> = Vec::new();
    let mut functions: Vec<Function> = Vec::new();
    walk(analyzer, root, &mut stack, &mut functions);
    // Any unclosed contexts (cannot happen for a well-formed tree, since every
    // entered function exits): flush them.
    while let Some(ctx) = stack.pop() {
        functions.push(ctx.function);
    }
    functions
}

fn walk<N: Node>(analyzer: &Analyzer, n: &N, stack: &mut Vec<Ctx>, functions: &mut Vec<Function>) {
    // On enter.
    let is_fn = analyzer.is_visitor_function(n);
    if is_fn {
        // Push a context: capture name + line count at push time.
        stack.push(Ctx {
            function: Function {
                name: analyzer.extract_function_name(n),
                line_count: n.count_lines(),
                variables: Vec::new(),
                cohesion: 0.0,
            },
        });
    }
    // Process variables against the *innermost* (top) context, if any. The
    // function node itself is processed under its own freshly-pushed context.
    if let Some(ctx) = stack.last_mut() {
        process_variable_node(analyzer, n, &mut ctx.function.variables);
    }

    // Recurse (preorder DFS, source-order children).
    for child in n.children() {
        walk(analyzer, child, stack, functions);
    }

    // On exit: pop and emit the function.
    if is_fn {
        if let Some(ctx) = stack.pop() {
            functions.push(ctx.function);
        }
    }
}

/// A node satisfying both the declaration and identifier predicates
/// contributes its name twice (two independent `if`s — report-contract quirk,
/// see [`Analyzer`] docs).
fn process_variable_node<N: Node>(analyzer: &Analyzer, n: &N, vars: &mut Vec<String>) {
    if analyzer.is_variable_declaration_pub(n) {
        add_variable_if_valid(n, vars);
    }
    if analyzer.is_variable_identifier_pub(n) {
        add_variable_if_valid(n, vars);
    }
}

fn add_variable_if_valid<N: Node>(n: &N, vars: &mut Vec<String>) {
    let name = n.entity_name();
    if !name.is_empty() {
        vars.push(name);
    }
}
