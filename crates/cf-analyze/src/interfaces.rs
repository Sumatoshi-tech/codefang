//! Analyzer trait hierarchy and supporting types.
//!
//! The reference signatures' ambient-context parameters are dropped (no
//! context threading here); cancellation, where needed, is a concern of the
//! `cf-framework`/`cf-pipeline` schedulers, noted in the crate TODOs.

use std::io::Write;

use cf_pipeline::ConfigurationOption;
use cf_uast_node::Node;

use crate::descriptor::Descriptor;
use crate::error::AnalyzeError;
use crate::report::Report;
use crate::thresholds::Thresholds;

/// A time-interval checkpoint key (tick index).
pub type Tick = i64;

/// Tick Container — per-commit analyzer data emitted during `Consume`.
///
/// `data` is a string-keyed map of arbitrary JSON values.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tc {
    /// `data` — analyzer-specific per-commit data.
    pub data: Report,
}

impl Tc {
    /// Creates an empty TC.
    #[must_use] 
    pub fn new() -> Self {
        Self { data: Report::new_map() }
    }
}

/// Controls whether per-item data is collected during aggregation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AggregationMode {
    /// Collects all per-item data (the default).
    #[default]
    Full,
    /// Skips per-item data collection.
    SummaryOnly,
}

/// The common base trait for all analyzers.
pub trait Analyzer {
    /// Returns the unique analyzer name.
    fn name(&self) -> String;
    /// Returns the CLI flag.
    fn flag(&self) -> String;
    /// Returns stable metadata.
    fn descriptor(&self) -> Descriptor;
    /// Lists configurable options.
    fn list_configuration_options(&self) -> Vec<ConfigurationOption>;
    /// Applies configuration facts.
    fn configure(&mut self, facts: &Report) -> Result<(), AnalyzeError>;
}

/// Shared contract for analyzers producing reportable output with thresholds,
/// aggregation, and per-format serialization.
///
/// Both [`StaticAnalyzer`] and [`RawFileAnalyzer`] satisfy this trait. The reference
/// `CreateAggregator()`/`ResultAggregator` machinery is represented by
/// [`FormattableAnalyzer::create_aggregator`] returning a boxed
/// [`ResultAggregator`].
pub trait FormattableAnalyzer: Analyzer {
    /// Returns color-coded thresholds.
    fn thresholds(&self) -> Thresholds;
    /// Creates a result aggregator.
    fn create_aggregator(&self) -> Box<dyn ResultAggregator>;
    /// Writes a human-readable report.
    fn format_report(&self, report: &Report, w: &mut dyn Write) -> Result<(), AnalyzeError>;
    /// Writes the report as JSON.
    fn format_report_json(&self, report: &Report, w: &mut dyn Write) -> Result<(), AnalyzeError>;
    /// Writes the report as YAML.
    fn format_report_yaml(&self, report: &Report, w: &mut dyn Write) -> Result<(), AnalyzeError>;
    /// Writes the report as a plot.
    fn format_report_plot(&self, report: &Report, w: &mut dyn Write) -> Result<(), AnalyzeError>;
    /// Writes the report as CFB1 binary.
    fn format_report_binary(&self, report: &Report, w: &mut dyn Write) -> Result<(), AnalyzeError>;
}

/// Contract for UAST-based static analysis.
pub trait StaticAnalyzer: FormattableAnalyzer {
    /// Analyzes a parsed UAST root.
    fn analyze(&self, root: &Node) -> Result<Report, AnalyzeError>;
}

/// Contract for analyzers operating on raw file content.
pub trait RawFileAnalyzer: FormattableAnalyzer {
    /// Analyzes raw file bytes.
    fn analyze_file_content(
        &self,
        path: &str,
        content: &[u8],
    ) -> Result<Report, AnalyzeError>;
}

/// Contract for analyzers operating over commit history.
pub trait HistoryAnalyzer: Analyzer {
    /// Processes a single commit's data.
    fn consume(&mut self, tc: &Tc) -> Result<(), AnalyzeError>;
    /// Produces the final report.
    fn finalize(&mut self) -> Result<Report, AnalyzeError>;
}

/// A [`HistoryAnalyzer`] that also supports serialization.
pub trait LeafAnalyzer: HistoryAnalyzer {
    /// Serializes a report in the requested format.
    fn serialize(
        &self,
        report: &Report,
        format: &str,
        w: &mut dyn Write,
    ) -> Result<(), AnalyzeError>;
    /// Serializes aggregated ticks.
    fn serialize_ticks(
        &self,
        ticks: &[Tick],
        format: &str,
        w: &mut dyn Write,
    ) -> Result<(), AnalyzeError>;
}

/// Aggregates analyzer results.
pub trait ResultAggregator {
    /// Combines per-analyzer results.
    fn aggregate(&mut self, results: &[(String, Report)]);
    /// Returns the merged result.
    fn get_result(&self) -> Report;
}

/// Combines per-commit reports into a final report.
pub trait Aggregator {
    /// Merges a TC into running state.
    fn consume(&mut self, tc: &Tc) -> Result<(), AnalyzeError>;
    /// Returns the final merged report.
    fn finalize(&mut self) -> Result<Report, AnalyzeError>;
}

/// Options for aggregator creation.
#[derive(Debug, Clone, Copy, Default)]
pub struct AggregatorOptions {
    /// Caps parallel aggregation operations.
    pub max_parallel: usize,
}

/// Facts key for the global temporary directory override. When set, analyzers use this directory
/// for spill and hibernation files instead of the system temp dir.
pub const CONFIG_TMP_DIR: &str = "TmpDir";

/// On-disk spill state of an [`Aggregator`]. Used by the checkpoint system to save and restore
/// spill directories.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AggregatorSpillInfo {
    /// `dir` — directory containing spill files. Empty if no spills occurred.
    pub dir: String,
    /// `count` — number of spill files written.
    pub count: i64,
}

/// Extracts and clears per-commit data between chunks during streaming
/// timeseries NDJSON output. Aggregators that store per-commit summary data
/// implement this to enable per-chunk flushing.
pub trait CommitStatsDrainer {
    /// Returns per-commit summary data and per-tick commit ordering, then clears
    /// these maps from the aggregator. Cumulative state remains intact.
    fn drain_commit_stats(&mut self) -> (Report, Vec<(Tick, Vec<String>)>);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tc_new_is_empty() {
        let tc = Tc::new();
        assert!(tc.data.is_empty());
    }

    #[test]
    fn aggregation_mode_default_is_full() {
        assert_eq!(AggregationMode::default(), AggregationMode::Full);
    }
}
