//! Cohesion visitor — port of `internal/analyzers/cohesion/visitor.go`.
//!
//! The static pipeline runs the cohesion analyzer through its
//! [`analyze.AnalysisVisitor`] (Go `cohesion.Visitor`), driven by a preorder DFS
//! (`MultiAnalyzerTraverser.Traverse`). This module reproduces that traversal
//! faithfully so the folder-walk output matches Go byte-for-byte.
//!
//! Key differences from the [`Analyzer::analyze`](crate::analyzer::Analyzer::analyze)
//! (`findFunctions`) path:
//!
//! * Function detection is `HasAnyType(Function,Method) OR
//!   HasAllRoles(Function,Declaration)` (Go `(*Visitor).isFunction`), not
//!   `role Function OR type Function/Method`.
//! * Variables are attributed to the **innermost** enclosing function on the
//!   context stack (`currentContext()` = stack top), via `processVariableNode`
//!   on every visited node while a context is active. A variable inside a nested
//!   function therefore belongs only to that nested function, never to the outer
//!   one (the `findFunctions` path would attribute it to both).
//! * Functions are emitted in `OnExit` order (Go `popContext` appends on exit).

use crate::analyzer::{Analyzer, Function};
use crate::uast::Node;

/// One entry on the visitor's context stack (Go `cohesionContext`): the function
/// being built and the variables collected so far for it.
struct Ctx {
    function: Function,
}

/// Collects functions from `root` exactly as the Go cohesion visitor does when
/// the `MultiAnalyzerTraverser` walks the tree in preorder.
///
/// Cohesion of each returned [`Function`] is left at `0.0`; the caller fills it
/// via [`Analyzer::compute_per_function_cohesion`].
pub(crate) fn collect_functions_via_visitor<N: Node>(analyzer: &Analyzer, root: &N) -> Vec<Function> {
    let mut stack: Vec<Ctx> = Vec::new();
    let mut functions: Vec<Function> = Vec::new();
    walk(analyzer, root, &mut stack, &mut functions);
    // Any unclosed contexts (cannot happen for a well-formed tree, but mirror Go's
    // OnExit emission which fires for every entered function): flush them.
    while let Some(ctx) = stack.pop() {
        functions.push(ctx.function);
    }
    functions
}

fn walk<N: Node>(analyzer: &Analyzer, n: &N, stack: &mut Vec<Ctx>, functions: &mut Vec<Function>) {
    // OnEnter.
    let is_fn = analyzer.is_visitor_function(n);
    if is_fn {
        // pushContext: capture name + line count now (Go reads them at push time).
        stack.push(Ctx {
            function: Function {
                name: analyzer.extract_function_name(n),
                line_count: n.count_lines(),
                variables: Vec::new(),
                cohesion: 0.0,
            },
        });
    }
    // processNode against the *innermost* (top) context, if any. The function node
    // itself is processed under its own freshly-pushed context, matching Go.
    if let Some(ctx) = stack.last_mut() {
        process_variable_node(analyzer, n, &mut ctx.function.variables);
    }

    // Recurse (preorder DFS, source-order children).
    for child in n.children() {
        walk(analyzer, child, stack, functions);
    }

    // OnExit: pop and emit the function (Go `popContext`).
    if is_fn {
        if let Some(ctx) = stack.pop() {
            functions.push(ctx.function);
        }
    }
}

/// Go `(*Analyzer).processVariableNode`: a node satisfying both the declaration
/// and identifier predicates contributes its name twice (two independent `if`s).
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
