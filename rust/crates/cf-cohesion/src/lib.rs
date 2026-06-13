//! `cf-cohesion` — static LCOM-HS cohesion analyzer (analyzer id:
//! `static/cohesion`). Also used by the quality analyzer.
//!
//! * [`Analyzer::analyze`] walks a UAST, extracts functions and their variables, and
//!   computes three module-level scalars — `lcom` (LCOM-HS, Henderson-Sellers),
//!   `cohesion_score`, and `function_cohesion` — plus a per-function table.
//! * Per-function cohesion uses a single global Bloom filter of *shared* variables
//!   (variables appearing in more than one function); see
//!   [`calc::build_global_variable_filter`] /
//!   [`Analyzer::calculate_function_level_cohesion`]. Bloom false positives are
//!   part of the defined behavior.
//!
//! # Byte-identity notes
//!
//! * [`metrics::ComputedMetrics`] and its nested structs serialize their fields
//!   in **declaration order**, honoring `omitempty` (report-format contract).
//! * The `distribution` field is a string-keyed map; its keys are byte-sorted
//!   on encode.
//! * The dynamic [`Report`] map (the analyzer's intermediate result) has byte-sorted
//!   keys when emitted; the per-function `functions` array order is
//!   **nondeterministic in the reference implementation** and is therefore a
//!   *named-canonicalizer* path in the golden harness rather than a raw-byte gate.
//!   The scalars (`lcom`, `cohesion_score`, `function_cohesion`,
//!   `total_functions`) are stable.
//!
//! Compatibility: output bytes are pinned against the reference implementation
//! by the differential gate in `rust/tests/compat`. See the module-level docs
//! of [`calc`], [`metrics`], [`report_section`] and [`aggregator`].
//!
//! # Example
//!
//! Analyze a two-function module and read the stable scalars. With `f` using
//! `{x, y}` and `g` using `{x}`, LCOM-HS is `0.25`, so the cohesion score
//! (`1 - lcom`) is `0.75`:
//!
//! ```
//! use cf_cohesion::Analyzer;
//! use cf_cohesion::report_value::ReportValue;
//! use cf_cohesion::uast::TestNode;
//!
//! let f = TestNode::function("f", 5, vec![TestNode::variable("x"), TestNode::variable("y")]);
//! let g = TestNode::function("g", 3, vec![TestNode::variable("x")]);
//! let root = TestNode::block(vec![f, g]);
//!
//! let report = Analyzer::new().analyze(&root).unwrap();
//! assert_eq!(report.get("total_functions"), Some(&ReportValue::Int(2)));
//!
//! let lcom = report.get("lcom").unwrap().as_float().unwrap();
//! assert!((lcom - 0.25).abs() < 1e-9);
//! let score = report.get("cohesion_score").unwrap().as_float().unwrap();
//! assert!((score - 0.75).abs() < 1e-9);
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod aggregator;
pub mod analyzer;
pub mod calc;
pub mod metrics;
pub mod report_section;
pub mod report_value;
pub mod serialize;
pub mod uast;
pub mod visitor;

pub use analyzer::{Analyzer, Function, FunctionReportItem};
pub use metrics::{
    AggregateData, ComputedMetrics, FunctionCohesionData, FunctionData, LowCohesionFunctionData,
    ReportData,
};
pub use report_value::{Report, ReportValue};

/// Crate name, used by smoke tests to confirm the module links.
pub const CRATE_NAME: &str = "cf-cohesion";

/// The analyzer name as it appears in reports.
pub const ANALYZER_NAME: &str = "cohesion";
