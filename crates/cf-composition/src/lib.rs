//! `cf-composition` — raw-file composition analyzer (`static/composition`).
//!
//! Classifies every walked file into `source` / `documentation` /
//! `configuration` / `vendor` / `generated` / `dotfile` / `image` / `binary`
//! / `other` using enry-style heuristics ([`Classifier`]), then aggregates
//! per-class counts and percentages ([`Aggregator`] → [`CompositionReport`])
//! and renders the report section, its JSON value and the `--per-file`
//! entries ([`report_section`]).
//!
//! The analyzer classifies **every** file the walk hands it — vendored and
//! generated files are exactly what it exists to count — so the only walk
//! filter that applies to it is `--languages`.
//!
//! # Module layout
//!
//! * [`category`] — `Category` (the 8-way classification shared with
//!   file-history analysis), `FileClass` (`Category` + `other`),
//!   `ALL_FILE_CLASSES`, `ClassCounts`.
//! * [`classifier`] — `Classifier` + the enry predicate subset.
//! * [`aggregator`] — `Aggregator`, `CompositionReport` and its raw report.
//! * [`report_section`] — `ReportSection`, the JSON value, per-file entries.
//!
//! All report values are [`cf_gojson`] trees; nothing here uses `serde`.
//!
//! # Example
//!
//! ```
//! use cf_composition::{Aggregator, Category, Classifier, FileClass};
//!
//! let classifier = Classifier::new();
//! let mut agg = Aggregator::new();
//! for (path, content) in [
//!     ("pkg/main.go", &b"package main\n"[..]),
//!     ("vendor/x/y.go", b"package x\n"),
//!     ("data.xyz", b"hello\n"),
//! ] {
//!     agg.add(classifier.classify_file(path, content));
//! }
//! let report = agg.report();
//! assert_eq!(report.total_files, 3);
//! assert_eq!(report.count(Category::Vendor.into()), 1);
//! assert_eq!(report.count(FileClass::Other), 1);
//! ```

#![forbid(unsafe_code)]

pub mod aggregator;
pub mod category;
pub mod classifier;
pub mod report_section;

pub use aggregator::{Aggregator, CompositionReport};
pub use category::{Category, ClassCounts, FileClass, ALL_FILE_CLASSES};
pub use classifier::Classifier;
pub use report_section::{FileRecord, ReportSection, SECTION_TITLE};

/// Analyzer name.
pub const ANALYZER_NAME: &str = "composition";
/// CLI flag name.
pub const ANALYZER_FLAG: &str = "composition";
/// Fully-qualified analyzer ID.
pub const ANALYZER_ID: &str = "static/composition";
/// Human description.
pub const ANALYZER_DESCRIPTION: &str =
    "Classifies files by type (source, vendor, generated, docs, config, binary, image, other) using enry.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyzer_identity() {
        assert_eq!(ANALYZER_NAME, "composition");
        assert_eq!(ANALYZER_FLAG, "composition");
        assert_eq!(ANALYZER_ID, "static/composition");
    }

    #[test]
    fn classify_then_aggregate_covers_every_class_and_sums_to_100() {
        let classifier = Classifier::new();
        let files: [(&str, &[u8]); 10] = [
            ("main.go", b"package main\n"),
            ("README.md", b"# T\n"),
            ("config.yaml", b"k: v\n"),
            ("vendor/x/y.go", b"package x\n"),
            ("gen/thing.pb.go", b"// Code generated. DO NOT EDIT.\n"),
            (".editorconfig", b"[*]\n"),
            ("logo.png", b""),
            ("data.bin", b"\x00\x01"),
            ("data.xyz", b"hello\n"),
            ("empty.go", b""),
        ];
        let mut agg = Aggregator::new();
        for (path, content) in files {
            agg.add(classifier.classify_file(path, content));
        }
        let report = agg.report();
        assert_eq!(report.total_files, 10);
        for class in ALL_FILE_CLASSES {
            let want = if class == FileClass::Category(Category::Source) {
                2
            } else {
                1
            };
            assert_eq!(report.count(class), want, "{class:?}");
        }
        let sum: f64 = ALL_FILE_CLASSES.iter().map(|c| report.percent(*c)).sum();
        assert!((sum - 100.0).abs() < 1e-9, "{sum}");
    }
}
