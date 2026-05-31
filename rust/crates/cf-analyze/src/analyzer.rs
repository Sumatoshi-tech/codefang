//! Core analyzer contracts and the dynamic [`Report`] model.
//!
//! Port of `internal/analyzers/analyze/analyzer.go`.
//!
//! Go's `Report = map[string]any` becomes a [`cf_gojson::GoMap`] (built with
//! [`cf_gojson::MapOrigin::Map`] so its keys byte-sort at encode time, matching
//! Go's `map[string]any` ordering — DESIGN §2.2). The analyzer interfaces map to
//! Rust traits. The Go `Factory` (parallel/sequential static dispatch over UAST
//! nodes) is ported with the structure preserved; the UAST `Node` and the
//! visitor machinery live in not-yet-ported crates, so the node type is a trait
//! parameter and the visitor traits are minimal (see the crate's
//! structured-output notes for the deferred-dependency list).

use std::io::Write;

use cf_pipeline::ConfigurationOption;

use crate::descriptor::Descriptor;

/// `ErrUnregisteredAnalyzer` (analyzer.go:17).
pub const ERR_UNREGISTERED_ANALYZER: &str = "no registered analyzer with name";
/// `ErrAnalysisFailed` (analyzer.go:20).
pub const ERR_ANALYSIS_FAILED: &str = "analysis failed";
/// `ErrNilRootNode` (analyzer.go:23).
pub const ERR_NIL_ROOT_NODE: &str = "root node is nil";

/// A report — the dynamic `map[string]any` analyzer output.
///
/// Mirrors `Report = map[string]any` (analyzer.go:26). It is a [`GoMap`] built
/// with [`MapOrigin::Map`] so the encoder byte-sorts the keys, reproducing Go's
/// map encoding exactly.
///
/// [`GoMap`]: cf_gojson::GoMap
/// [`MapOrigin::Map`]: cf_gojson::MapOrigin::Map
pub type Report = cf_gojson::GoMap;

/// Color-coded thresholds for multiple metrics.
///
/// Mirrors `Thresholds = map[string]map[string]any` (analyzer.go:75). Structure:
/// `{"metric_name": {"red": v, "yellow": v, "green": v}}`.
pub type Thresholds = cf_gojson::GoMap;

/// Extracts a `[]map[string]any` (here `Vec<&GoMap>`) from a report key.
///
/// Mirrors `ReportFunctionList` (analyzer.go:30). Handles a directly-typed array
/// of objects (Go's `[]map[string]any`) and a JSON-decoded `[]any` of objects.
/// The Go `TypedCollection` fast path is covered by [`crate::TypedCollection`]
/// at the construction boundary, so here a value already materialized as an
/// array of objects is returned; non-object elements are filtered out (matching
/// the Go `[]any` branch). Returns `(maps, found)` where `found` mirrors Go's
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
/// Mirrors `ReportFunctionListWithFallback` (analyzer.go:64).
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
/// Mirrors `Analyzer` (analyzer.go:78).
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
/// aggregation, and format methods. Mirrors `FormattableAnalyzer` (analyzer.go:91).
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

/// UAST-based static analysis contract. Mirrors `StaticAnalyzer` (analyzer.go:109).
///
/// `Node` is a trait parameter so this crate does not depend on `cf-uast-node`.
pub trait StaticAnalyzer<Node>: FormattableAnalyzer {
    /// Analyzes a UAST root node into a [`Report`] (`Analyze`).
    ///
    /// # Errors
    /// Returns an error (e.g. on a nil root) per the analyzer.
    fn analyze(&self, root: &Node) -> Result<Report, crate::history::AnalyzerError>;
}

/// Raw-file analysis contract (path + bytes, no UAST). Mirrors `RawFileAnalyzer`
/// (analyzer.go:118).
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

/// Enables single-pass traversal optimization. Mirrors `VisitorProvider`
/// (analyzer.go:125).
pub trait VisitorProvider<V> {
    /// Creates a single-pass analysis visitor (`CreateVisitor`).
    fn create_visitor(&self) -> V;
}

/// Aggregates per-analyzer results into one report. Mirrors `ResultAggregator`
/// (analyzer.go:130).
pub trait ResultAggregator {
    /// Folds a set of named reports into the aggregate (`Aggregate`).
    fn aggregate(&mut self, results: &[(String, Report)]);
    /// Returns the aggregated report (`GetResult`).
    fn get_result(&self) -> Report;
}

/// Implemented by aggregators supporting configurable spill thresholds. Mirrors
/// `SpillThresholdSetter` (analyzer.go:137).
pub trait SpillThresholdSetter {
    /// Sets the spill-to-disk threshold in bytes.
    fn set_spill_threshold(&mut self, threshold: i64);
}

/// Implemented by aggregators that can estimate in-memory state size. Mirrors
/// `StateSizer` (analyzer.go:143).
pub trait StateSizer {
    /// Estimated bytes of in-memory state.
    fn estimated_state_size(&self) -> i64;
}

/// Manages registration and execution of static analyzers.
///
/// Mirrors `Factory` (analyzer.go:148). This port keeps the registry and the
/// single-analyzer dispatch (`run_analyzer`); the full parallel/sequential
/// visitor fan-out (analyzer.go:226-341) is driven by the framework, which binds
/// the UAST `Node` type and the visitor traits (see the crate's
/// structured-output notes for the deferred-dependency list).
pub struct Factory<N, A: StaticAnalyzer<N>> {
    analyzers: Vec<(String, A)>,
    max_parallel: usize,
    _node: std::marker::PhantomData<fn() -> N>,
}

impl<N, A: StaticAnalyzer<N>> Factory<N, A> {
    /// Creates a factory from a list of analyzers. Mirrors `NewFactory`
    /// (analyzer.go:154): `max_parallel` defaults to the CPU count.
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

    /// Adds an analyzer to the registry. Mirrors `RegisterAnalyzer`
    /// (analyzer.go:168). Keyed by [`Analyzer::name`].
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
    pub fn max_parallel(&self) -> usize {
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

    /// Executes the named analyzer on `root`. Mirrors `RunAnalyzer`
    /// (analyzer.go:173).
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
    fn error_constants_match_go() {
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
