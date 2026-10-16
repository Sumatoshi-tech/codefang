//! Composition aggregator: accumulates per-file classes into the
//! [`CompositionReport`] (total files + per-class counts) and its raw
//! `breakdown` / `percentages` / `total_files` report map.

use cf_gojson::{GoMap, GoValue, MapOrigin};

use crate::category::{ClassCounts, FileClass, ALL_FILE_CLASSES};

/// Report key for the per-class counts map.
pub const KEY_BREAKDOWN: &str = "breakdown";
/// Report key for the per-class percentages map.
pub const KEY_PERCENTAGE: &str = "percentages";
/// Report key for the total file count.
pub const KEY_TOTAL_FILES: &str = "total_files";

/// 100.0, used to convert a fraction to a percentage.
const PERCENT_MULTIPLIER: f64 = 100.0;

/// The aggregated composition of a file tree.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompositionReport {
    /// Total files classified.
    pub total_files: i64,
    /// Per-class counts.
    pub counts: ClassCounts,
}

impl CompositionReport {
    /// Files of `class`.
    #[must_use]
    pub fn count(&self, class: FileClass) -> i64 {
        self.counts.get(class)
    }

    /// Share of `class` as a fraction in `[0, 1]` (`0.0` with no files).
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // file counts are far below 2^52
    pub fn fraction(&self, class: FileClass) -> f64 {
        if self.total_files == 0 {
            return 0.0;
        }
        self.count(class) as f64 / self.total_files as f64
    }

    /// Share of `class` as a percentage in `[0, 100]`.
    #[must_use]
    pub fn percent(&self, class: FileClass) -> f64 {
        self.fraction(class) * PERCENT_MULTIPLIER
    }

    /// Builds the raw aggregated report: a map-origin [`GoMap`] (keys
    /// byte-sort on encode) with `breakdown` (every class, zero counts
    /// included), `percentages` (`count / total * 100`, empty when no file
    /// was classified) and `total_files`.
    ///
    /// ```
    /// use cf_composition::{Aggregator, Category, FileClass};
    /// use cf_gojson::GoValue;
    ///
    /// let mut agg = Aggregator::new();
    /// agg.add(Category::Source.into());
    /// agg.add(Category::Source.into());
    /// agg.add(FileClass::Other);
    ///
    /// let raw = agg.report().to_raw_report();
    /// assert_eq!(raw.get("total_files"), Some(&GoValue::Int(3)));
    /// let breakdown = raw.get("breakdown").unwrap().as_map().unwrap();
    /// assert_eq!(breakdown.get("source"), Some(&GoValue::Int(2)));
    /// assert_eq!(breakdown.get("other"), Some(&GoValue::Int(1)));
    /// ```
    #[must_use]
    pub fn to_raw_report(&self) -> GoMap {
        let mut breakdown = GoMap::new(MapOrigin::Map);
        let mut percentages = GoMap::new(MapOrigin::Map);
        for class in ALL_FILE_CLASSES {
            breakdown.push(class.as_str(), GoValue::Int(self.count(class)));
            if self.total_files > 0 {
                percentages.push(class.as_str(), GoValue::Float(self.percent(class)));
            }
        }
        let mut report = GoMap::new(MapOrigin::Map);
        report.push(KEY_BREAKDOWN, GoValue::Map(breakdown));
        report.push(KEY_PERCENTAGE, GoValue::Map(percentages));
        report.push(KEY_TOTAL_FILES, GoValue::Int(self.total_files));
        report
    }
}

/// Aggregates file classes across a file tree.
#[derive(Debug, Default, Clone, Copy)]
pub struct Aggregator {
    report: CompositionReport,
}

impl Aggregator {
    /// Creates an empty aggregator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Counts one file of `class`.
    pub fn add(&mut self, class: FileClass) {
        self.report.total_files += 1;
        self.report.counts.increment(class);
    }

    /// The aggregated report so far.
    #[must_use]
    pub fn report(&self) -> CompositionReport {
        self.report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::category::Category;
    use cf_gojson::Encoder;

    #[test]
    fn empty_report_has_no_percentages() {
        let raw = Aggregator::new().report().to_raw_report();
        let json = Encoder::marshal().encode_to_string(&GoValue::Map(raw));
        assert_eq!(
            json,
            r#"{"breakdown":{"binary":0,"configuration":0,"documentation":0,"dotfile":0,"generated":0,"image":0,"other":0,"source":0,"vendor":0},"percentages":{},"total_files":0}"#
        );
    }

    #[test]
    fn counts_fractions_and_percentages() {
        let mut agg = Aggregator::new();
        for class in [
            Category::Source.into(),
            Category::Source.into(),
            Category::Source.into(),
            Category::Vendor.into(),
            FileClass::Other,
        ] {
            agg.add(class);
        }
        let r = agg.report();
        assert_eq!(r.total_files, 5);
        assert_eq!(r.count(Category::Source.into()), 3);
        assert!((r.fraction(Category::Vendor.into()) - 0.2).abs() < 1e-12);
        assert!((r.percent(FileClass::Other) - 20.0).abs() < 1e-12);
        let sum: f64 = ALL_FILE_CLASSES.iter().map(|c| r.percent(*c)).sum();
        assert!((sum - 100.0).abs() < 1e-9, "percentages sum to {sum}");
    }
}
