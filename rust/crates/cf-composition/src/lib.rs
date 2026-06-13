//! `cf-composition` — Raw-file composition analyzer (`static/composition`).
//!
//! Classifies files into `source` / `vendor` / `generated` / `documentation` /
//! `configuration` / `binary` / `image` / `dotfile` using enry-style heuristics,
//! then aggregates per-category counts and percentages into the
//! `static/composition` report.
//!
//! # Compatibility
//!
//! All machine-format report serialization routes through `cf-gojson` (the
//! report-format JSON encoder), never `serde_json` — see [`analyzer`]. The
//! aggregated report is a *map-origin* [`cf_gojson::GoMap`], so its keys are
//! byte-sorted at encode time per the report contract for dynamic maps. Output
//! bytes are pinned against the reference binary by `rust/tests/compat`.
//!
//! # Module layout
//!
//! * [`category`] — `Category`, `ALL_CATEGORIES`, `CategoryCounts` (the minimal
//!   file-classification surface shared with file-history analysis).
//! * [`classifier`] — `Classifier` + the enry predicate subset.
//! * [`aggregator`] — `Aggregator` (per-file results -> aggregate report).
//! * [`report_section`] — `ReportSection` (terminal section data).
//! * [`analyzer`] — `Analyzer` (identity + format entry points).
//!
//! # Example
//!
//! Classify a single file into its [`Category`]:
//!
//! ```
//! use cf_composition::{Classifier, Category};
//!
//! let category = Classifier::new().classify("pkg/main.go", b"package main\n");
//! assert_eq!(category, Category::Source);
//! assert_eq!(category.as_str(), "source");
//! ```
//!
//! The analyzer's per-file report is a single-key `category` map, encoded
//! through `cf-gojson`:
//!
//! ```
//! use cf_composition::Analyzer;
//!
//! let report = Analyzer::new().analyze_file_content("pkg/main.go", b"package main\n");
//! let bytes = cf_gojson::Encoder::compact()
//!     .encode_to_vec(&cf_gojson::GoValue::Map(report));
//! assert_eq!(bytes, br#"{"category":"source"}"#);
//! ```

#![forbid(unsafe_code)]

pub mod aggregator;
pub mod analyzer;
pub mod category;
pub mod classifier;
pub mod report_section;

pub use aggregator::Aggregator;
pub use analyzer::{
    Analyzer, ANALYZER_DESCRIPTION, ANALYZER_FLAG, ANALYZER_ID, ANALYZER_NAME,
};
pub use category::{Category, CategoryCounts, ALL_CATEGORIES};
pub use classifier::Classifier;
pub use report_section::{CompositionReport, ReportSection, SECTION_TITLE};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aggregator::{KEY_BREAKDOWN, KEY_CATEGORY, KEY_PERCENTAGE, KEY_TOTAL_FILES};
    use crate::report_section::{
        SCORE_INFO_ONLY, SEVERITY_INFO, SEVERITY_POOR, STATUS_DEFAULT, STATUS_EMPTY,
    };
    use std::collections::HashMap;

    // ---- Analyzer identity ----

    #[test]
    fn analyzer_name() {
        assert_eq!(Analyzer::new().name(), ANALYZER_NAME);
    }

    #[test]
    fn analyzer_flag() {
        assert_eq!(Analyzer::new().flag(), ANALYZER_FLAG);
    }

    #[test]
    fn analyzer_descriptor_id() {
        assert_eq!(Analyzer::new().id(), ANALYZER_ID);
    }

    #[test]
    fn analyzer_thresholds_nil() {
        assert!(Analyzer::new().thresholds().is_none());
    }

    #[test]
    fn analyzer_list_configuration_options_empty() {
        assert!(Analyzer::new().list_configuration_options().is_empty());
    }

    #[test]
    fn analyzer_configure_no_error() {
        assert!(Analyzer::new().configure(&HashMap::new()).is_ok());
    }

    // ---- Classification ----

    fn classify(path: &str, content: &[u8]) -> Category {
        Classifier::new().classify(path, content)
    }

    #[test]
    fn analyze_content_go_file() {
        assert_eq!(
            classify("pkg/main.go", b"package main\n\nfunc main() {}\n"),
            Category::Source
        );
    }

    #[test]
    fn analyze_content_vendor_path() {
        assert_eq!(
            classify("vendor/github.com/foo/bar.go", b"package bar\n"),
            Category::Vendor
        );
    }

    #[test]
    fn analyze_content_markdown() {
        assert_eq!(classify("docs/README.md", b"# Hello\n"), Category::Documentation);
    }

    #[test]
    fn analyze_content_config_file() {
        assert_eq!(
            classify(".golangci.yml", b"linters:\n  enable:\n"),
            Category::Configuration
        );
    }

    #[test]
    fn analyze_content_binary_content() {
        let binary = [0x00u8, 0x01, 0x02, 0xFF, 0xFE, 0x00, 0x00, 0x00];
        assert_eq!(classify("data.bin", &binary), Category::Binary);
    }

    #[test]
    fn analyze_content_dot_file() {
        assert_eq!(
            classify(".editorconfig", b"[*]\nindent_style = tab\n"),
            Category::DotFile
        );
    }

    #[test]
    fn analyze_content_image_path() {
        assert_eq!(classify("logo.png", &[]), Category::Image);
    }

    #[test]
    fn analyze_file_content_report_shape() {
        let report = Analyzer::new().analyze_file_content("pkg/main.go", b"package main\n");
        let bytes =
            cf_gojson::Encoder::compact().encode_to_vec(&cf_gojson::GoValue::Map(report));
        assert_eq!(bytes, br#"{"category":"source"}"#);
    }

    // ---- Aggregator ----

    fn single(category: &str) -> HashMap<String, HashMap<String, cf_gojson::GoValue>> {
        let mut report = HashMap::new();
        report.insert(
            KEY_CATEGORY.to_string(),
            cf_gojson::GoValue::Str(category.to_string()),
        );
        let mut results = HashMap::new();
        results.insert(ANALYZER_NAME.to_string(), report);
        results
    }

    fn decode(report: &cf_gojson::GoMap) -> CompositionReport {
        let mut out = CompositionReport::default();
        for (key, value) in report.entries() {
            match (key.as_str(), value) {
                (KEY_TOTAL_FILES, cf_gojson::GoValue::Int(n)) => out.total_files = *n,
                (KEY_BREAKDOWN, cf_gojson::GoValue::Map(m)) => {
                    for (cat, v) in m.entries() {
                        if let cf_gojson::GoValue::Int(n) = v {
                            out.breakdown.insert(cat.clone(), *n);
                        }
                    }
                }
                _ => {}
            }
        }
        out
    }

    #[test]
    fn aggregator_empty_result() {
        let agg = Aggregator::new();
        let result = decode(&agg.get_result());
        assert_eq!(result.total_files, 0);
    }

    #[test]
    fn aggregator_single_file() {
        let mut agg = Aggregator::new();
        agg.aggregate(&single(Category::Source.as_str()));
        let result = decode(&agg.get_result());
        assert_eq!(result.total_files, 1);
        assert_eq!(result.breakdown.get(Category::Source.as_str()), Some(&1));
    }

    #[test]
    fn aggregator_multiple_files() {
        let mut agg = Aggregator::new();
        let files = [
            Category::Source,
            Category::Source,
            Category::Source,
            Category::Vendor,
            Category::Documentation,
        ];
        for cat in files {
            agg.aggregate(&single(cat.as_str()));
        }
        let result = decode(&agg.get_result());
        assert_eq!(result.total_files, files.len() as i64);
        assert_eq!(result.breakdown.get(Category::Source.as_str()), Some(&3));
        assert_eq!(result.breakdown.get(Category::Vendor.as_str()), Some(&1));
        assert_eq!(result.breakdown.get(Category::Documentation.as_str()), Some(&1));

        // Percentages are present in the encoded report.
        let mut pct = HashMap::new();
        for (key, value) in agg.get_result().entries() {
            if key == KEY_PERCENTAGE {
                if let cf_gojson::GoValue::Map(m) = value {
                    for (cat, v) in m.entries() {
                        if let cf_gojson::GoValue::Float(f) = v {
                            pct.insert(cat.clone(), *f);
                        }
                    }
                }
            }
        }
        assert!((pct[Category::Source.as_str()] - 60.0).abs() < 0.1);
        assert!((pct[Category::Vendor.as_str()] - 20.0).abs() < 0.1);
        assert!((pct[Category::Documentation.as_str()] - 20.0).abs() < 0.1);
    }

    #[test]
    fn aggregator_skips_invalid_category() {
        let mut agg = Aggregator::new();
        let mut report = HashMap::new();
        report.insert("not_a_category".to_string(), cf_gojson::GoValue::Int(42));
        let mut results = HashMap::new();
        results.insert(ANALYZER_NAME.to_string(), report);
        agg.aggregate(&results);

        let result = decode(&agg.get_result());
        // File counted but no known category incremented.
        assert_eq!(result.total_files, 1);
        assert_eq!(result.breakdown.get(Category::Source.as_str()), Some(&0));
    }

    // ---- Report section ----

    fn test_report() -> CompositionReport {
        let mut breakdown = HashMap::new();
        breakdown.insert(Category::Source.as_str().to_string(), 6);
        breakdown.insert(Category::Vendor.as_str().to_string(), 2);
        breakdown.insert(Category::Documentation.as_str().to_string(), 1);
        breakdown.insert(Category::Binary.as_str().to_string(), 1);
        CompositionReport {
            total_files: 10,
            breakdown,
        }
    }

    #[test]
    fn section_title() {
        assert_eq!(ReportSection::new(test_report()).section_title(), SECTION_TITLE);
    }

    #[test]
    fn section_score_info_only() {
        assert!((ReportSection::new(test_report()).score() - SCORE_INFO_ONLY).abs() < 0.001);
    }

    #[test]
    fn section_status_message() {
        assert_eq!(
            ReportSection::new(test_report()).status_message(),
            STATUS_DEFAULT
        );
    }

    #[test]
    fn section_status_message_empty() {
        assert_eq!(
            ReportSection::new(CompositionReport::default()).status_message(),
            STATUS_EMPTY
        );
    }

    #[test]
    fn section_nil_report() {
        let s = ReportSection::new(CompositionReport::default());
        assert_eq!(s.section_title(), SECTION_TITLE);
        assert_eq!(s.status_message(), STATUS_EMPTY);
    }

    #[test]
    fn section_key_metrics_count() {
        assert_eq!(ReportSection::new(test_report()).key_metrics().len(), 3);
    }

    #[test]
    fn section_key_metrics_labels() {
        let s = ReportSection::new(test_report());
        let m = s.key_metrics();
        assert_eq!(m[0].label, "Total Files");
        assert_eq!(m[1].label, "Source Files");
        assert_eq!(m[2].label, "Source %");
    }

    #[test]
    fn section_key_metrics_values() {
        let s = ReportSection::new(test_report());
        let m = s.key_metrics();
        assert_eq!(m[0].value, "10");
        assert_eq!(m[1].value, "6");
        assert!(m[2].value.contains("60"));
    }

    #[test]
    fn section_distribution() {
        let s = ReportSection::new(test_report());
        let dist = s.distribution();
        assert_eq!(dist.len(), 4);
        // First follows AllCategories order → source.
        assert_eq!(dist[0].label, Category::Source.as_str());
        assert_eq!(dist[0].count, 6);
    }

    #[test]
    fn section_distribution_empty() {
        assert!(ReportSection::new(CompositionReport::default())
            .distribution()
            .is_empty());
    }

    #[test]
    fn section_top_issues() {
        assert_eq!(ReportSection::new(test_report()).top_issues(2).len(), 2);
    }

    #[test]
    fn section_all_issues() {
        // 3 non-source categories with counts: vendor, docs, binary.
        assert_eq!(ReportSection::new(test_report()).all_issues().len(), 3);
    }

    #[test]
    fn section_issues_binary_severity_poor() {
        let s = ReportSection::new(test_report());
        let issues = s.all_issues();
        let binary = issues
            .iter()
            .find(|i| i.name == Category::Binary.as_str())
            .expect("binary category must appear in issues");
        assert_eq!(binary.severity, SEVERITY_POOR);
    }

    #[test]
    fn section_issues_vendor_severity_info() {
        let s = ReportSection::new(test_report());
        let issues = s.all_issues();
        let vendor = issues
            .iter()
            .find(|i| i.name == Category::Vendor.as_str())
            .expect("vendor category must appear in issues");
        assert_eq!(vendor.severity, SEVERITY_INFO);
    }

    #[test]
    fn section_issues_empty() {
        assert!(ReportSection::new(CompositionReport::default())
            .all_issues()
            .is_empty());
    }

    // ---- Format paths ----

    fn category_report(category: &str) -> cf_gojson::GoMap {
        let mut m = cf_gojson::GoMap::new(cf_gojson::MapOrigin::Map);
        m.push(KEY_CATEGORY, cf_gojson::GoValue::Str(category.to_string()));
        m
    }

    #[test]
    fn format_report_json_contains_source() {
        let mut buf = Vec::new();
        Analyzer::new()
            .format_report_json(&category_report("source"), &mut buf)
            .unwrap();
        assert!(String::from_utf8(buf).unwrap().contains("source"));
    }

    #[test]
    fn format_report_contains_binary() {
        let mut buf = Vec::new();
        Analyzer::new()
            .format_report(&category_report("binary"), &mut buf)
            .unwrap();
        assert!(String::from_utf8(buf).unwrap().contains("binary"));
    }

    #[test]
    fn format_report_plot_contains_docs() {
        let mut buf = Vec::new();
        Analyzer::new()
            .format_report_plot(&category_report("docs"), &mut buf)
            .unwrap();
        assert!(String::from_utf8(buf).unwrap().contains("docs"));
    }

    #[test]
    fn format_report_binary_envelope() {
        let mut buf = Vec::new();
        Analyzer::new()
            .format_report_binary(&category_report("source"), &mut buf)
            .unwrap();
        assert!(!buf.is_empty());
        assert_eq!(&buf[..4], b"CFB1");
        // Payload length (LE u32) matches the compact JSON body.
        let len = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]) as usize;
        assert_eq!(buf.len(), 8 + len);
        assert_eq!(&buf[8..], br#"{"category":"source"}"#);
    }
}
