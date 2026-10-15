//! Import/dependency analysis.
//!
//! Two related analyzers share this crate:
//!
//! * **`static/imports`** — [`extract_imports`] turns one parsed file into the
//!   modules/packages/files it imports (language-aware, deduplicated per
//!   file); [`Aggregate`] folds files into per-import file counts; [`metrics`]
//!   classifies each import (`relative` / `stdlib` / `external`, using the
//!   per-language tables in [`stdlib`]) and derives the [`ComputedMetrics`]
//!   the json/yaml/bin formats serialize.
//! * **`history/imports`** ([`history`]) — per-developer, per-language,
//!   per-tick import usage in a 4-level [`history::ImportsMap`] that merges
//!   additively; [`history::HistoryReport`] is the value the machine
//!   formats encode (`tick_size`, `author_index`, developer → language →
//!   import → tick → count).
//!
//! Serialization of every machine format routes through
//! [`ComputedMetrics::to_go_value`] / [`history::HistoryReport::to_go_value`]
//! and the `cf-gojson` / `cf-goyaml` encoders; the dynamic [`ReportValue`]
//! exists for the plot callers that rebuild the static report from a value
//! tree.
//!
//! # Example
//!
//! Extract the imports of a Go file from its real parse tree and aggregate
//! them:
//!
//! ```
//! use cf_imports::{extract_imports, Aggregate};
//! use cf_uast::Parser;
//!
//! let src = b"package main\n\nimport (\n\t\"fmt\"\n\tstr \"strings\"\n\t_ \"embed\"\n)\n";
//! let parser = Parser::new();
//! let root = parser.parse("main.go", src).unwrap();
//!
//! let imports = extract_imports(&root, &parser.get_language("main.go"), src);
//! let paths: Vec<&str> = imports.iter().map(|i| i.path.as_str()).collect();
//! assert_eq!(paths, ["fmt", "strings", "embed"]);
//!
//! let mut agg = Aggregate::new();
//! agg.add_file("go", &imports);
//! assert_eq!(agg.total_files(), 1);
//! assert_eq!(agg.ranked(), vec![("embed", 1), ("fmt", 1), ("strings", 1)]);
//! ```

pub mod aggregate;
pub mod extract;
pub mod history;
pub mod language;
pub mod metrics;
pub mod report;
pub mod section;
pub mod statement;
pub mod stdlib;

// Re-exports for the primary public surface.
pub use aggregate::{Aggregate, Entry};
pub use extract::{extract_imports, Import, IMPORT_TRIGGER_TYPES};
pub use history::{HistoryReport, ImportEntry, ImportsMap};
pub use language::Language;
pub use metrics::{
    compute_all_metrics, compute_metrics, AggregateData, ComputedMetrics, ImportCategoryData,
    ImportData, ImportDependencyData, ImportRecord, ReportData,
};
pub use report::ReportValue;
