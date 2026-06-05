//! Analyzer trait hierarchy and supporting types.
//!
//! Port of the interfaces declared across `analyzer.go`, `history.go`, `tc.go`
//! and `aggregation_mode.go`. The Go `context.Context` parameters are dropped
//! (Rust does not thread an ambient context); cancellation, where needed, is a
//! concern of the eventual `cf-framework`/`cf-pipeline` schedulers, noted in
//! the crate TODOs.

use std::io::Write;

use cf_pipeline::ConfigurationOption;
use cf_uast_node::Node;

use crate::descriptor::Descriptor;
use crate::error::AnalyzeError;
use crate::report::Report;
use crate::thresholds::Thresholds;

/// A time-interval checkpoint key (tick index). Port of Go `type TICK = int`.
pub type Tick = i64;

/// Tick Container — per-commit analyzer data emitted during `Consume`.
///
/// Port of Go `TC`. `data` is a string-keyed map of arbitrary JSON values.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tc {
    /// `data` — analyzer-specific per-commit data.
    pub data: Report,
}

impl Tc {
    /// Creates an empty TC. Port of Go `NewTC`.
    pub fn new() -> Self {
        Tc { data: Report::new_map() }
    }
}

/// Controls whether per-item data is collected during aggregation. Port of Go
/// `AggregationMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AggregationMode {
    /// Collects all per-item data (Go zero value `AggregationModeFull`).
    #[default]
    Full,
    /// Skips per-item data collection (Go `AggregationModeSummaryOnly`).
    SummaryOnly,
}

/// The common base trait for all analyzers. Port of Go `Analyzer`.
pub trait Analyzer {
    /// Returns the unique analyzer name. Go `Name()`.
    fn name(&self) -> String;
    /// Returns the CLI flag. Go `Flag()`.
    fn flag(&self) -> String;
    /// Returns stable metadata. Go `Descriptor()`.
    fn descriptor(&self) -> Descriptor;
    /// Lists configurable options. Go `ListConfigurationOptions()`.
    fn list_configuration_options(&self) -> Vec<ConfigurationOption>;
    /// Applies configuration facts. Go `Configure(map[string]any) error`.
    fn configure(&mut self, facts: &Report) -> Result<(), AnalyzeError>;
}

/// Shared contract for analyzers producing reportable output with thresholds,
/// aggregation, and per-format serialization. Port of Go `FormattableAnalyzer`.
///
/// Both [`StaticAnalyzer`] and [`RawFileAnalyzer`] satisfy this trait. The Go
/// `CreateAggregator()`/`ResultAggregator` machinery is represented by
/// [`FormattableAnalyzer::create_aggregator`] returning a boxed
/// [`ResultAggregator`].
pub trait FormattableAnalyzer: Analyzer {
    /// Returns color-coded thresholds. Go `Thresholds()`.
    fn thresholds(&self) -> Thresholds;
    /// Creates a result aggregator. Go `CreateAggregator()`.
    fn create_aggregator(&self) -> Box<dyn ResultAggregator>;
    /// Writes a human-readable report. Go `FormatReport`.
    fn format_report(&self, report: &Report, w: &mut dyn Write) -> Result<(), AnalyzeError>;
    /// Writes the report as JSON. Go `FormatReportJSON`.
    fn format_report_json(&self, report: &Report, w: &mut dyn Write) -> Result<(), AnalyzeError>;
    /// Writes the report as YAML. Go `FormatReportYAML`.
    fn format_report_yaml(&self, report: &Report, w: &mut dyn Write) -> Result<(), AnalyzeError>;
    /// Writes the report as a plot. Go `FormatReportPlot`.
    fn format_report_plot(&self, report: &Report, w: &mut dyn Write) -> Result<(), AnalyzeError>;
    /// Writes the report as CFB1 binary. Go `FormatReportBinary`.
    fn format_report_binary(&self, report: &Report, w: &mut dyn Write) -> Result<(), AnalyzeError>;
}

/// Contract for UAST-based static analysis. Port of Go `StaticAnalyzer`.
pub trait StaticAnalyzer: FormattableAnalyzer {
    /// Analyzes a parsed UAST root. Go `Analyze(root *node.Node)`.
    fn analyze(&self, root: &Node) -> Result<Report, AnalyzeError>;
}

/// Contract for analyzers operating on raw file content. Port of Go
/// `RawFileAnalyzer`.
pub trait RawFileAnalyzer: FormattableAnalyzer {
    /// Analyzes raw file bytes. Go `AnalyzeFileContent(path, content)`.
    fn analyze_file_content(
        &self,
        path: &str,
        content: &[u8],
    ) -> Result<Report, AnalyzeError>;
}

/// Contract for analyzers operating over commit history. Port of Go
/// `HistoryAnalyzer`.
pub trait HistoryAnalyzer: Analyzer {
    /// Processes a single commit's data. Go `Consume(ctx, tc)`.
    fn consume(&mut self, tc: &Tc) -> Result<(), AnalyzeError>;
    /// Produces the final report. Go `Finalize(ctx)`.
    fn finalize(&mut self) -> Result<Report, AnalyzeError>;
}

/// A [`HistoryAnalyzer`] that also supports serialization. Port of Go
/// `LeafAnalyzer`.
pub trait LeafAnalyzer: HistoryAnalyzer {
    /// Serializes a report in the requested format. Go `Serialize`.
    fn serialize(
        &self,
        report: &Report,
        format: &str,
        w: &mut dyn Write,
    ) -> Result<(), AnalyzeError>;
    /// Serializes aggregated ticks. Go `SerializeTICKs`.
    fn serialize_ticks(
        &self,
        ticks: &[Tick],
        format: &str,
        w: &mut dyn Write,
    ) -> Result<(), AnalyzeError>;
}

/// Aggregates analyzer results. Port of Go `ResultAggregator`.
pub trait ResultAggregator {
    /// Combines per-analyzer results. Go `Aggregate(map[string]Report)`.
    fn aggregate(&mut self, results: &[(String, Report)]);
    /// Returns the merged result. Go `GetResult()`.
    fn get_result(&self) -> Report;
}

/// Combines per-commit reports into a final report. Port of Go `Aggregator`.
pub trait Aggregator {
    /// Merges a TC into running state. Go `Consume(ctx, tc)`.
    fn consume(&mut self, tc: &Tc) -> Result<(), AnalyzeError>;
    /// Returns the final merged report. Go `Finalize(ctx)`.
    fn finalize(&mut self) -> Result<Report, AnalyzeError>;
}

/// Options for aggregator creation. Port of Go `AggregatorOptions`.
#[derive(Debug, Clone, Copy, Default)]
pub struct AggregatorOptions {
    /// Caps parallel aggregation operations. Go `MaxParallel`.
    pub max_parallel: usize,
}

/// Facts key for the global temporary directory override. Port of Go
/// `ConfigTmpDir` (`aggregator.go:79`). When set, analyzers use this directory
/// for spill and hibernation files instead of the system temp dir.
pub const CONFIG_TMP_DIR: &str = "TmpDir";

/// On-disk spill state of an [`Aggregator`]. Port of Go `AggregatorSpillInfo`
/// (`aggregator.go:69`). Used by the checkpoint system to save and restore
/// spill directories.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AggregatorSpillInfo {
    /// `dir` — directory containing spill files. Empty if no spills occurred.
    pub dir: String,
    /// `count` — number of spill files written.
    pub count: i64,
}

/// Extracts and clears per-commit data between chunks during streaming
/// timeseries NDJSON output. Port of Go `CommitStatsDrainer`
/// (`aggregator.go:57`). Aggregators that store per-commit summary data
/// implement this to enable per-chunk flushing.
pub trait CommitStatsDrainer {
    /// Returns per-commit summary data and per-tick commit ordering, then clears
    /// these maps from the aggregator. Cumulative state remains intact. Port of
    /// Go `DrainCommitStats`.
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
