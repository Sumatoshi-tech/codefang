//! `cf-halstead` — Halstead complexity metrics.
//!
//! The crate owns the whole measurement: the operator / operand
//! classification table ([`classify`]), source-span text recovery
//! ([`source`]), the function walker ([`detector`]), the derived-measure
//! arithmetic ([`calculator`]), the union aggregate with its per-function
//! statistics ([`aggregate`]), and every threshold, label and score formula
//! ([`thresholds`]). The `static/halstead` command handler in `cf-commands`
//! only walks files, parses them, and serializes these results; the
//! `history/quality` analyzer reads the file-level scalars through
//! [`analyze`] / [`analyze_with_source`] — the same detector, so both
//! surfaces agree.
//!
//! Definitions: `n1` = distinct operators, `n2` = distinct operands, `N1` =
//! total operators, `N2` = total operands; vocabulary `n = n1 + n2`; length
//! `N = N1 + N2`; volume `V = N · log2(n)`; difficulty
//! `D = (n1 / 2) · (N2 / n2)`; effort `E = D · V`; time `T = E / 18`; bugs
//! `B = V / 3000`.
//!
//! # Example
//!
//! Measure a one-function tree. The body `x = y + 5` contributes the
//! operators `=` / `+` (exposed by the mapping as `props["operator"]`) and
//! the operands `x` / `y` / `5`:
//!
//! ```
//! use cf_halstead::{analyze, analyze_functions};
//! use cf_uast_node::Node;
//!
//! fn id(token: &str) -> Node {
//!     let mut n = Node::with_token("Identifier", token);
//!     n.roles = vec!["Variable".into()];
//!     n
//! }
//!
//! let mut plus = Node::with_token("BinaryOp", "");
//! plus.props.insert("operator".into(), "+".into());
//! plus.add_child(id("y"));
//! plus.add_child(Node::literal("5"));
//!
//! let mut assign = Node::with_token("Assignment", "");
//! assign.props.insert("operator".into(), "=".into());
//! assign.add_child(id("x"));
//! assign.add_child(plus);
//!
//! let mut body = Node::with_token("Block", "");
//! body.roles = vec!["Body".into()];
//! body.add_child(assign);
//!
//! let mut func = Node::with_token("Function", "");
//! func.props.insert("name".into(), "f".into());
//! func.add_child(body);
//!
//! let mut root = Node::with_token("File", "");
//! root.add_child(func);
//!
//! let fns = analyze_functions(&root, None);
//! assert_eq!(fns[0].name, "f");
//! assert_eq!(fns[0].counts.distinct_operators, 2);
//! assert_eq!(fns[0].counts.total_operands, 3);
//! // n = 5, N = 5 → V = 5 · log2(5)
//! assert_eq!(analyze(&root).volume, 5.0 * 5f64.log2());
//! ```
//!
//! A tree with no functions yields all-zero measures:
//!
//! ```
//! use cf_halstead::{analyze, FileHalstead};
//! use cf_uast_node::Node;
//!
//! assert_eq!(analyze(&Node::with_token("File", "")), FileHalstead::default());
//! ```

/// Analyzer name as it appears in reports (`analyzer_name`).
pub const ANALYZER_NAME: &str = "halstead";

pub mod aggregate;
pub mod calculator;
pub mod classify;
pub mod detector;
pub mod source;
pub mod standalone;
pub mod thresholds;

pub use aggregate::{analyze_functions, summarize, FunctionMetrics, FunctionStats, Summary};
pub use standalone::{analyze, analyze_with_source, FileHalstead};
