//! Cross-file aggregation for the static analyzer.
//!
//! [`Aggregate`] folds the per-file import lists into one table keyed by
//! import path: the **number of files** importing the path, plus the language
//! and local-file flag of the first file that imported it (which drive the
//! stdlib / relative classification in [`crate::metrics`]).

use std::collections::BTreeMap;

use crate::extract::Import;
use crate::metrics::{ImportRecord, ReportData};

/// One aggregated import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Number of analyzed files importing the path.
    pub count: i64,
    /// Parser language name of the first file that imported the path.
    pub language: String,
    /// Whether the first importing file named it as a local file include.
    pub local_file: bool,
}

/// Import path -> [`Entry`] across every analyzed file.
#[derive(Debug, Default, Clone)]
pub struct Aggregate {
    entries: BTreeMap<String, Entry>,
    total_files: i64,
}

impl Aggregate {
    /// An empty aggregate.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Folds one analyzed file (import-free files still count towards
    /// `total_files`). `imports` is expected deduplicated, as
    /// [`crate::extract_imports`] returns it; each path counts once per file.
    pub fn add_file(&mut self, language: &str, imports: &[Import]) {
        self.total_files += 1;
        for import in imports {
            self.entries
                .entry(import.path.clone())
                .and_modify(|e| e.count += 1)
                .or_insert_with(|| Entry {
                    count: 1,
                    language: language.to_string(),
                    local_file: import.local_file,
                });
        }
    }

    /// Number of analyzed files.
    #[must_use]
    pub fn total_files(&self) -> i64 {
        self.total_files
    }

    /// Number of unique import paths.
    #[must_use]
    pub fn unique_count(&self) -> i64 {
        self.entries.len() as i64
    }

    /// `(path, entry)` pairs in byte-sorted path order.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &Entry)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// `(path, count)` pairs ordered by count descending, then path
    /// ascending — the deterministic issue order of the report section.
    #[must_use]
    pub fn ranked(&self) -> Vec<(&str, i64)> {
        let mut ranked: Vec<(&str, i64)> = self
            .entries
            .iter()
            .map(|(k, v)| (k.as_str(), v.count))
            .collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        ranked
    }

    /// The metrics input: one record per unique path, in path order.
    #[must_use]
    pub fn report_data(&self) -> ReportData {
        ReportData {
            imports: self
                .entries
                .iter()
                .map(|(path, e)| ImportRecord {
                    path: path.clone(),
                    language: e.language.clone(),
                    local_file: e.local_file,
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_files_not_occurrences_and_keeps_first_language() {
        let mut agg = Aggregate::new();
        agg.add_file("go", &[Import::package("fmt"), Import::package("os")]);
        agg.add_file("python", &[Import::package("os"), Import::package("sys")]);
        agg.add_file("markdown", &[]);

        assert_eq!(agg.total_files(), 3);
        assert_eq!(agg.unique_count(), 3);
        assert_eq!(agg.ranked(), vec![("os", 2), ("fmt", 1), ("sys", 1)]);

        let os = agg.entries().find(|(p, _)| *p == "os").expect("os").1;
        assert_eq!(os.language, "go");
        let data = agg.report_data();
        assert_eq!(data.imports.len(), 3);
        assert_eq!(data.imports[0].path, "fmt");
        assert_eq!(data.imports[2].language, "python");
    }

    #[test]
    fn local_file_flag_comes_from_the_first_importer() {
        let mut agg = Aggregate::new();
        agg.add_file("c", &[Import::local("local.h")]);
        agg.add_file("c", &[Import::package("local.h")]);
        assert!(agg.report_data().imports[0].local_file);
        assert_eq!(agg.ranked(), vec![("local.h", 2)]);
    }

    #[test]
    fn empty_aggregate_is_the_empty_report() {
        let agg = Aggregate::new();
        assert_eq!(agg.total_files(), 0);
        assert_eq!(agg.unique_count(), 0);
        assert!(agg.ranked().is_empty());
        assert!(agg.report_data().imports.is_empty());
    }
}
