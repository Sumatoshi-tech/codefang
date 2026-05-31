//! `cf-cohesion` — Rust port of the Go `internal/analyzers/cohesion` package.
//!
//! Purpose (from the porting brief): *Static LCOM-HS cohesion + per-function Bloom
//! shared-vars. Function-table order nondeterministic; scalars stable. Also used by
//! quality.*
//!
//! This crate reproduces the behavior of the Go analyzer:
//!
//! * [`Analyzer::analyze`] walks a UAST, extracts functions and their variables, and
//!   computes three module-level scalars — `lcom` (LCOM-HS, Henderson-Sellers),
//!   `cohesion_score`, and `function_cohesion` — plus a per-function table.
//! * Per-function cohesion uses a single global Bloom filter of *shared* variables
//!   (variables appearing in more than one function), exactly as the Go code does
//!   ([`calc::build_global_variable_filter`] /
//!   [`Analyzer::calculate_function_level_cohesion`]).
//! * The machine-format report ([`metrics::ComputedMetrics`]) is serialized through
//!   the Go-byte-compatible encoders (`cf-gojson` / `cf-goyaml`) and the CFB1 binary
//!   envelope (`cf-reportutil`), per `specs/rust-rewrite/DESIGN.md` §2.
//!
//! # Byte-identity notes
//!
//! * [`metrics::ComputedMetrics`] and its nested structs are *wrapper* structs: their
//!   fields serialize in **declaration order**, honoring `omitempty`, matching the Go
//!   struct tags one-for-one. They are emitted via the fixed-order `GoMap` builder of
//!   `cf-gojson`, not serde.
//! * The `distribution` field is a `map[string]int` in Go; its keys are byte-sorted
//!   on encode.
//! * The dynamic [`Report`] map (the analyzer's intermediate result) has byte-sorted
//!   keys when emitted; the per-function `functions` array order is
//!   **nondeterministic in Go** (it derives from Go map iteration in
//!   `deduplicateNodes`) and is therefore a *named-canonicalizer* path in the golden
//!   harness rather than a raw-byte gate. The scalars (`lcom`, `cohesion_score`,
//!   `function_cohesion`, `total_functions`) are stable.
//!
//! See the module-level docs of [`calc`], [`metrics`], [`report_section`] and
//! [`aggregator`] for the detail of each ported Go file.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod aggregator;
pub mod analyzer;
pub mod bloom;
pub mod calc;
pub mod metrics;
pub mod report_section;
pub mod report_value;
pub mod serialize;
pub mod uast;

pub use analyzer::{Analyzer, Function, FunctionReportItem};
pub use metrics::{
    AggregateData, ComputedMetrics, FunctionCohesionData, FunctionData, LowCohesionFunctionData,
    ReportData,
};
pub use report_value::{Report, ReportValue};

/// Crate name, used by smoke tests to confirm the module links.
pub const CRATE_NAME: &str = "cf-cohesion";

/// The analyzer name, identical to the Go `(*Analyzer).Name()`.
pub const ANALYZER_NAME: &str = "cohesion";
