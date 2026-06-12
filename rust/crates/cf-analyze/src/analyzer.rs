//! Core analyzer contracts and the dynamic [`Report`] model.
//!
//! A [`Report`] is a [`cf_gojson::GoMap`] built with
//! [`cf_gojson::MapOrigin::Map`] so its keys byte-sort at encode time, per
//! the report-contract string-keyed map ordering (DESIGN §2.2). The analyzer
//! interfaces are traits. The [`Factory`] (parallel/sequential static dispatch
//! over UAST nodes) keeps the reference structure; the UAST `Node` and the
//! visitor machinery live in higher crates, so the node type is a trait
//! parameter and the visitor traits are minimal (see the crate's
//! structured-output notes for the deferred-dependency list).

use std::io::Write;

use cf_pipeline::ConfigurationOption;

use crate::descriptor::Descriptor;

/// Sentinel error text: the requested analyzer is not registered.
pub const ERR_UNREGISTERED_ANALYZER: &str = "no registered analyzer with name";
/// Sentinel error text: an analysis run failed.
pub const ERR_ANALYSIS_FAILED: &str = "analysis failed";
/// Sentinel error text: the UAST root node is missing.
pub const ERR_NIL_ROOT_NODE: &str = "root node is nil";

/// A report — the dynamic string-keyed analyzer output map.
///
/// It is a [`GoMap`] built with [`MapOrigin::Map`] so the encoder byte-sorts
/// the keys, per the report-format contract for dynamic maps.
///
/// [`GoMap`]: cf_gojson::GoMap
/// [`MapOrigin::Map`]: cf_gojson::MapOrigin::Map
pub type Report = cf_gojson::GoMap;

/// Color-coded thresholds for multiple metrics.
///
/// Structure:
/// `{"metric_name": {"red": v, "yellow": v, "green": v}}`.
pub type Thresholds = cf_gojson::GoMap;

/// Extracts a `[]map[string]any` (here `Vec<&GoMap>`) from a report key.
///
/// Handles a directly-typed array
/// of objects and a JSON-decoded heterogeneous array of objects.
/// The `TypedCollection` fast path is covered by [`crate::TypedCollection`]
/// at the construction boundary, so here a value already materialized as an
/// array of objects is returned; non-object elements are filtered out (matching
/// the dynamic-array branch). Returns `(maps, found)` where `found` reports
/// `len(result) > 0`.
#[must_use]
pub fn report_function_list<'a>(report: &'a Report, key: &str) -> (Vec<&'a cf_gojson::GoMap>, bool) {
    let Some(val) = lookup(report, key) else {
        return (Vec::new(), false);
    };

    if let cf_gojson::GoValue::Array(items) = val {
        let mut result = Vec::with_capacity(items.len());
        for item in items {
            if let cf_gojson::GoValue::Map(m) = item {
                result.push(m);
            }
        }
        let found = !result.is_empty();
        return (result, found);
    }

    (Vec::new(), false)
}

/// Extracts a function list, trying `primary_key` first, then `fallback_key`.
///
///
#[must_use]
pub fn report_function_list_with_fallback<'a>(
    report: &'a Report,
    primary_key: &str,
    fallback_key: &str,
) -> (Vec<&'a cf_gojson::GoMap>, bool) {
    let (functions, ok) = report_function_list(report, primary_key);
    if ok {
        return (functions, true);
    }
    report_function_list(report, fallback_key)
}

fn lookup<'a>(report: &'a Report, key: &str) -> Option<&'a cf_gojson::GoValue> {
    report
        .entries()
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
}

/// The common base interface for all analyzers.
///
///
pub trait Analyzer {
    /// The analyzer name (`Name`).
    fn name(&self) -> String;
    /// The CLI flag (`Flag`).
    fn flag(&self) -> String;
    /// Stable analyzer metadata (`Descriptor`).
    fn descriptor(&self) -> Descriptor;
    /// Configurable options (`ListConfigurationOptions`).
    fn list_configuration_options(&self) -> Vec<ConfigurationOption>;
    /// Applies configuration facts (`Configure`).
    ///
    /// # Errors
    /// Propagates analyzer-specific configuration failures.
    fn configure(&mut self, facts: &cf_gojson::GoMap) -> Result<(), crate::history::AnalyzerError>;
}

/// Shared contract for analyzers producing reportable output with thresholds,
/// aggregation, and format methods.
pub trait FormattableAnalyzer: Analyzer {
    /// Color-coded thresholds (`Thresholds`).
    fn thresholds(&self) -> Thresholds;
    /// Creates a result aggregator (`CreateAggregator`).
    fn create_aggregator(&self) -> Box<dyn ResultAggregator>;

    /// Writes the report in the default (text) format (`FormatReport`).
    ///
    /// # Errors
    /// Propagates serialization failures.
    fn format_report(
        &self,
        report: &Report,
        writer: &mut dyn Write,
    ) -> Result<(), crate::history::AnalyzerError>;
    /// Writes the report as JSON (`FormatReportJSON`).
    ///
    /// # Errors
    /// Propagates serialization failures.
    fn format_report_json(
        &self,
        report: &Report,
        writer: &mut dyn Write,
    ) -> Result<(), crate::history::AnalyzerError>;
    /// Writes the report as YAML (`FormatReportYAML`).
    ///
    /// # Errors
    /// Propagates serialization failures.
    fn format_report_yaml(
        &self,
        report: &Report,
        writer: &mut dyn Write,
    ) -> Result<(), crate::history::AnalyzerError>;
    /// Writes the report as a plot (`FormatReportPlot`).
    ///
    /// # Errors
    /// Propagates rendering failures.
    fn format_report_plot(
        &self,
        report: &Report,
        writer: &mut dyn Write,
    ) -> Result<(), crate::history::AnalyzerError>;
    /// Writes the report as a CFB1 binary envelope (`FormatReportBinary`).
    ///
    /// # Errors
    /// Propagates serialization failures.
    fn format_report_binary(
        &self,
        report: &Report,
        writer: &mut dyn Write,
    ) -> Result<(), crate::history::AnalyzerError>;
}

/// UAST-based static analysis contract.
///
/// `Node` is a trait parameter so this crate does not depend on `cf-uast-node`.
pub trait StaticAnalyzer<Node>: FormattableAnalyzer {
    /// Analyzes a UAST root node into a [`Report`] (`Analyze`).
    ///
    /// # Errors
    /// Returns an error (e.g. on a nil root) per the analyzer.
    fn analyze(&self, root: &Node) -> Result<Report, crate::history::AnalyzerError>;
}

/// Raw-file analysis contract (path + bytes, no UAST).
pub trait RawFileAnalyzer: FormattableAnalyzer {
    /// Analyzes raw file content into a [`Report`] (`AnalyzeFileContent`).
    ///
    /// # Errors
    /// Propagates analyzer-specific failures.
    fn analyze_file_content(
        &self,
        path: &str,
        content: &[u8],
    ) -> Result<Report, crate::history::AnalyzerError>;
}

/// Enables single-pass traversal optimization.
pub trait VisitorProvider<V> {
    /// Creates a single-pass analysis visitor (`CreateVisitor`).
    fn create_visitor(&self) -> V;
}

/// Aggregates per-analyzer results into one report.
pub trait ResultAggregator {
    /// Folds a set of named reports into the aggregate (`Aggregate`).
    fn aggregate(&mut self, results: &[(String, Report)]);
    /// Returns the aggregated report (`GetResult`).
    fn get_result(&self) -> Report;
}

/// Implemented by aggregators supporting configurable spill thresholds.
pub trait SpillThresholdSetter {
    /// Sets the spill-to-disk threshold in bytes.
    fn set_spill_threshold(&mut self, threshold: i64);
}

/// Implemented by aggregators that can estimate in-memory state size.
pub trait StateSizer {
    /// Estimated bytes of in-memory state.
    fn estimated_state_size(&self) -> i64;
}

/// Manages registration and execution of static analyzers.
///
/// This port keeps the registry and the
/// single-analyzer dispatch (`run_analyzer`); the full parallel/sequential
/// visitor fan-out is driven by the framework, which binds
/// the UAST `Node` type and the visitor traits (see the crate's
/// structured-output notes for the deferred-dependency list).
pub struct Factory<N, A: StaticAnalyzer<N>> {
    analyzers: Vec<(String, A)>,
    max_parallel: usize,
    _node: std::marker::PhantomData<fn() -> N>,
}

impl<N, A: StaticAnalyzer<N>> Factory<N, A> {
    /// Creates a factory from a list of analyzers.
    /// `max_parallel` defaults to the CPU count.
    #[must_use]
    pub fn new(analyzers: Vec<A>) -> Self {
        let max_parallel =
            std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
        let mut f = Self {
            analyzers: Vec::new(),
            max_parallel,
            _node: std::marker::PhantomData,
        };
        for a in analyzers {
            f.register_analyzer(a);
        }
        f
    }

    /// Adds an analyzer to the registry, keyed by [`Analyzer::name`].
    pub fn register_analyzer(&mut self, analyzer: A) {
        let name = analyzer.name();
        if let Some(slot) = self.analyzers.iter_mut().find(|(n, _)| *n == name) {
            slot.1 = analyzer;
        } else {
            self.analyzers.push((name, analyzer));
        }
    }

    /// The configured max parallelism.
    #[must_use]
    pub const fn max_parallel(&self) -> usize {
        self.max_parallel
    }

    /// Looks up a registered analyzer by name.
    #[must_use]
    pub fn analyzer(&self, name: &str) -> Option<&A> {
        self.analyzers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, a)| a)
    }

    /// Executes the named analyzer on `root`.
    ///
    /// # Errors
    /// Returns [`crate::history::AnalyzerError::Other`] with the
    /// [`ERR_UNREGISTERED_ANALYZER`] message when `name` is unknown, else the
    /// analyzer's own error.
    pub fn run_analyzer(
        &self,
        name: &str,
        root: &N,
    ) -> Result<Report, crate::history::AnalyzerError> {
        match self.analyzer(name) {
            Some(a) => a.analyze(root),
            None => Err(crate::history::AnalyzerError::Other(format!(
                "{ERR_UNREGISTERED_ANALYZER}: {name}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_gojson::{GoValue, MapOrigin};

    fn report(pairs: &[(&str, GoValue)]) -> Report {
        let mut m = cf_gojson::GoMap::new(MapOrigin::Map);
        for (k, v) in pairs {
            m.insert(*k, v.clone());
        }
        m
    }

    #[test]
    fn error_constants_match_reference() {
        assert_eq!(ERR_UNREGISTERED_ANALYZER, "no registered analyzer with name");
        assert_eq!(ERR_ANALYSIS_FAILED, "analysis failed");
        assert_eq!(ERR_NIL_ROOT_NODE, "root node is nil");
    }

    #[test]
    fn report_function_list_array_of_objects() {
        let mut f0 = cf_gojson::GoMap::new(MapOrigin::Map);
        f0.insert("name", GoValue::Str("foo".into()));
        let r = report(&[("functions", GoValue::Array(vec![GoValue::Map(f0)]))]);
        let (list, ok) = report_function_list(&r, "functions");
        assert!(ok);
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn report_function_list_missing_key() {
        let r = report(&[]);
        let (list, ok) = report_function_list(&r, "functions");
        assert!(!ok);
        assert!(list.is_empty());
    }

    #[test]
    fn report_function_list_filters_non_objects() {
        let r = report(&[(
            "functions",
            GoValue::Array(vec![GoValue::Int(1), GoValue::Str("x".into())]),
        )]);
        let (list, ok) = report_function_list(&r, "functions");
        assert!(!ok);
        assert!(list.is_empty());
    }

    #[test]
    fn fallback_uses_secondary_key() {
        let mut f0 = cf_gojson::GoMap::new(MapOrigin::Map);
        f0.insert("name", GoValue::Str("foo".into()));
        let r = report(&[("backup", GoValue::Array(vec![GoValue::Map(f0)]))]);
        let (list, ok) = report_function_list_with_fallback(&r, "primary", "backup");
        assert!(ok);
        assert_eq!(list.len(), 1);
    }
}
