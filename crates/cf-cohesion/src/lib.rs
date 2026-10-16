//! `cf-cohesion` — the `static/cohesion` analyzer: LCOM-HS (Henderson-Sellers)
//! and per-function variable-sharing cohesion over a UAST.
//!
//! * [`Analyzer::analyze_file`] walks one file's UAST, discovers its functions
//!   and the distinct variables each one uses (the rules live in
//!   [`visitor`]), and computes the file's [`FileReport`]: `lcom`,
//!   `cohesion_score = 1 - lcom`, the mean `function_cohesion`, and a
//!   per-function table.
//! * [`aggregate`] folds many files into one [`Aggregate`]: every function is
//!   kept, scalars are function-weighted, and files without functions carry no
//!   data.
//! * [`metrics::compute_metrics`] and [`report_section`] derive the machine
//!   and section views the CLI renders.
//!
//! The documentation of record is `site/explanation/cohesion.md` (the model)
//! and `site/analyzers/cohesion.md` (the output).
//!
//! # Example
//!
//! ```
//! use cf_cohesion::Analyzer;
//!
//! let src = b"package a\n\nfunc f() int {\n\tx := 1\n\ty := 2\n\treturn x + y\n}\n\nfunc g() int {\n\tx := 3\n\treturn x\n}\n";
//! let root = cf_uast::Parser::new().parse("a.go", src).unwrap();
//! let report = Analyzer::new().analyze_file(&root);
//!
//! // f uses {x, y}, g uses {x}: m = 2, a = 2, sum(mA) = 3 -> LCOM = 1 - 3/4.
//! assert_eq!(report.total_functions, 2);
//! assert!((report.lcom - 0.25).abs() < 1e-9);
//! assert!((report.cohesion_score - 0.75).abs() < 1e-9);
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod aggregator;
pub mod analyzer;
pub mod calc;
pub mod metrics;
pub mod report_section;
pub mod report_value;
pub mod uast;
pub mod visitor;

pub use aggregator::{aggregate, Aggregate, AggregatedFunction, AnalyzedFile};
pub use analyzer::{Analyzer, FileReport, Function, FunctionMetrics};
pub use metrics::ComputedMetrics;
pub use report_value::{Report, ReportValue};

/// The analyzer name as it appears in reports.
pub const ANALYZER_NAME: &str = "cohesion";
