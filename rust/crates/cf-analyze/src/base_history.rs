//! `BaseHistoryAnalyzer` — embeddable default `HistoryAnalyzer` implementation.
//!
//! Port of `internal/analyzers/analyze/base_history.go`. Concrete analyzers embed
//! this to avoid boilerplate. The serialization paths route through
//! [`cf_gojson`] / [`cf_goyaml`] / [`cf_reportutil`] so JSON/YAML/binary output
//! stays byte-identical (DESIGN §2.3): JSON uses `json.Marshal` semantics
//! (compact, HTML-escape on, **no** trailing newline — the Go base uses
//! `json.Marshal`, not an `Encoder`), YAML uses the yaml.v3-compatible emitter,
//! and Binary uses the CFB1 envelope.
//!
//! Go's structural-typing `metricsSerializer` (ToJSON/ToYAML) is expressed as the
//! [`MetricsSerializer`] trait; metrics implementing it provide a format-specific
//! [`cf_gojson::GoValue`], otherwise the metrics' own value is used.

use std::io::Write;

use cf_pipeline::ConfigurationOption;

use crate::interfaces::{Aggregator, AggregatorOptions};
use crate::descriptor::Descriptor;
use crate::formats::{FormatError, FORMAT_BINARY, FORMAT_JSON, FORMAT_PLOT, FORMAT_TEXT, FORMAT_YAML};
use crate::history::AnalyzerError;
use crate::tc::Tick;

/// `ErrMissingComputeMetrics` (base_history.go:18).
pub const ERR_MISSING_COMPUTE_METRICS: &str = "missing ComputeMetricsFn hook";

/// The Go `metricsSerializer` structural interface (base_history.go:23).
///
/// Metrics types that need a different shape for JSON vs YAML implement this; the
/// returned [`cf_gojson::GoValue`] is what gets serialized. Metrics that don't
/// implement it serialize directly (their own [`to_go_value`](Self::to_go_value)).
pub trait MetricsSerializer {
    /// JSON representation (`ToJSON`).
    fn to_json(&self) -> cf_gojson::GoValue;
    /// YAML representation (`ToYAML`).
    fn to_yaml(&self) -> cf_gojson::GoValue;
}

/// A complete default implementation for the history-analyzer + parallelizable
/// contracts, intended to be embedded by concrete analyzers.
///
/// Mirrors `BaseHistoryAnalyzer[M]` (base_history.go:33). The generic `M` is the
/// metrics type; the [`compute_metrics_fn`](Self::compute_metrics_fn) hook turns
/// a [`Report`] into `M` and `M` must produce a serializable
/// [`cf_gojson::GoValue`] via [`to_go_value`](Self::to_go_value) (the default) or
/// a [`MetricsSerializer`] impl.
pub struct BaseHistoryAnalyzer<M> {
    /// Stable descriptor (`Desc`).
    pub desc: Descriptor,
    /// Whether the analyzer is sequential-only (`Sequential`).
    pub sequential: bool,
    /// Whether `consume` is CPU-heavy (`CPUHeavyFlag`).
    pub cpu_heavy_flag: bool,
    /// Estimated working state size in bytes (`EstimatedStateSize`).
    pub estimated_state_size: i64,
    /// Estimated TC payload size in bytes (`EstimatedTCSize`).
    pub estimated_tc_size: i64,
    /// Configurable options (`ConfigOptions`).
    pub config_options: Vec<ConfigurationOption>,

    // Hooks.
    /// Converts a report to typed metrics (`ComputeMetricsFn`). `None` ⇒
    /// [`ERR_MISSING_COMPUTE_METRICS`].
    pub compute_metrics_fn: Option<Box<dyn Fn(&crate::analyzer::Report) -> Result<M, AnalyzerError> + Send + Sync>>,
    /// Converts aggregated ticks to a report (`TicksToReportFn`).
    pub ticks_to_report_fn: Option<Box<dyn Fn(&[Tick]) -> crate::analyzer::Report + Send + Sync>>,
    /// Aggregator factory (`AggregatorFn`).
    pub aggregator_fn: Option<Box<dyn Fn(AggregatorOptions) -> Box<dyn Aggregator> + Send + Sync>>,

    /// Custom text serializer (`SerializeTextFn`).
    pub serialize_text_fn: Option<Box<dyn Fn(&crate::analyzer::Report, &mut dyn Write) -> Result<(), AnalyzerError> + Send + Sync>>,
    /// Custom plot serializer (`SerializePlotFn`).
    pub serialize_plot_fn: Option<Box<dyn Fn(&crate::analyzer::Report, &mut dyn Write) -> Result<(), AnalyzerError> + Send + Sync>>,

    /// How to turn an `M` into a serializable value. Set by the constructor; the
    /// Rust analogue of "the metrics value is itself JSON-serializable".
    pub metrics_to_value: Box<dyn Fn(&M) -> cf_gojson::GoValue + Send + Sync>,
}

impl<M> BaseHistoryAnalyzer<M> {
    /// Creates a base analyzer given the descriptor and a metrics→value function.
    ///
    /// All hooks default to `None`; set them on the returned value.
    #[must_use]
    pub fn new(
        desc: Descriptor,
        metrics_to_value: impl Fn(&M) -> cf_gojson::GoValue + Send + Sync + 'static,
    ) -> Self {
        Self {
            desc,
            sequential: false,
            cpu_heavy_flag: false,
            estimated_state_size: 0,
            estimated_tc_size: 0,
            config_options: Vec::new(),
            compute_metrics_fn: None,
            ticks_to_report_fn: None,
            aggregator_fn: None,
            serialize_text_fn: None,
            serialize_plot_fn: None,
            metrics_to_value: Box::new(metrics_to_value),
        }
    }

    /// The analyzer name (descriptor ID). Mirrors `Name` (base_history.go:54).
    #[must_use]
    pub fn name(&self) -> String {
        self.desc.id.clone()
    }

    /// The CLI flag — the part after `history/`, else the whole ID.
    ///
    /// Mirrors `Flag` (base_history.go:59).
    #[must_use]
    pub fn flag(&self) -> String {
        match self.desc.id.split_once('/') {
            Some((_, rest)) => rest.to_string(),
            None => self.desc.id.clone(),
        }
    }

    /// The analyzer description. Mirrors `Description` (base_history.go:69).
    #[must_use]
    pub fn description(&self) -> String {
        self.desc.description.clone()
    }

    /// Stable descriptor. Mirrors `Descriptor` (base_history.go:74).
    #[must_use]
    pub fn descriptor(&self) -> Descriptor {
        self.desc.clone()
    }

    /// True if the analyzer cannot be parallelized. Mirrors `SequentialOnly`.
    #[must_use]
    pub fn sequential_only(&self) -> bool {
        self.sequential
    }

    /// True if `consume` is CPU-intensive. Mirrors `CPUHeavy`.
    #[must_use]
    pub fn cpu_heavy(&self) -> bool {
        self.cpu_heavy_flag
    }

    /// Estimated working-state bytes. Mirrors `WorkingStateSize`.
    #[must_use]
    pub fn working_state_size(&self) -> i64 {
        self.estimated_state_size
    }

    /// Estimated per-commit TC payload bytes. Mirrors `AvgTCSize`.
    #[must_use]
    pub fn avg_tc_size(&self) -> i64 {
        self.estimated_tc_size
    }

    /// Configurable options. Mirrors `ListConfigurationOptions`.
    #[must_use]
    pub fn list_configuration_options(&self) -> Vec<ConfigurationOption> {
        self.config_options.clone()
    }

    /// Default no-op configure. Mirrors `Configure` (base_history.go:104).
    ///
    /// # Errors
    /// Never returns an error in the base implementation.
    pub fn configure(&mut self, _facts: &cf_gojson::GoMap) -> Result<(), AnalyzerError> {
        Ok(())
    }

    /// Creates an aggregator via the hook, or `None`. Mirrors `NewAggregator`
    /// (base_history.go:112).
    #[must_use]
    pub fn new_aggregator(&self, opts: AggregatorOptions) -> Option<Box<dyn Aggregator>> {
        self.aggregator_fn.as_ref().map(|f| f(opts))
    }

    /// Serializes a finalized report in `format`. Mirrors `Serialize`
    /// (base_history.go:133).
    ///
    /// Custom text/plot hooks take priority; otherwise `compute_metrics_fn`
    /// produces the metrics and they are written via JSON/YAML/Binary.
    ///
    /// # Errors
    /// - [`AnalyzerError::MissingComputeMetrics`] when no compute hook is set;
    /// - the compute hook's own error;
    /// - [`AnalyzerError::UnsupportedFormat`] for unknown formats.
    pub fn serialize(
        &self,
        result: &crate::analyzer::Report,
        format: &str,
        writer: &mut dyn Write,
    ) -> Result<(), AnalyzerError> {
        if format == FORMAT_TEXT {
            if let Some(f) = &self.serialize_text_fn {
                return f(result, writer);
            }
        }
        if format == FORMAT_PLOT {
            if let Some(f) = &self.serialize_plot_fn {
                return f(result, writer);
            }
        }

        let Some(compute) = &self.compute_metrics_fn else {
            return Err(AnalyzerError::MissingComputeMetrics);
        };

        let metrics = compute(result)?;
        self.write_metrics_to_format(&metrics, format, writer)
    }

    /// Encodes metrics in `format`. Mirrors `writeMetricsToFormat`
    /// (base_history.go:156).
    fn write_metrics_to_format(
        &self,
        metrics: &M,
        format: &str,
        writer: &mut dyn Write,
    ) -> Result<(), AnalyzerError> {
        match format {
            FORMAT_JSON => {
                // json.Marshal target (compact, escape on, NO trailing newline).
                let value = (self.metrics_to_value)(metrics);
                let bytes = cf_gojson::marshal(&value);
                writer
                    .write_all(&bytes)
                    .map_err(|e| AnalyzerError::Other(format!("json write: {e}")))
            }
            FORMAT_YAML => {
                let value = (self.metrics_to_value)(metrics);
                let bytes = cf_goyaml::marshal(&value);
                writer
                    .write_all(&bytes)
                    .map_err(|e| AnalyzerError::Other(format!("yaml write: {e}")))
            }
            FORMAT_BINARY => {
                let value = (self.metrics_to_value)(metrics);
                let bytes = cf_reportutil::binary::encode_binary_envelope(&value)
                    .map_err(|e| AnalyzerError::Other(format!("binary encode: {e}")))?;
                writer
                    .write_all(&bytes)
                    .map_err(|e| AnalyzerError::Other(format!("binary write: {e}")))
            }
            other => Err(AnalyzerError::UnsupportedFormat(FormatError::Unsupported {
                format: other.to_string(),
            })),
        }
    }

    /// Serializes from aggregated ticks via the report hook, then [`serialize`].
    ///
    /// Mirrors `SerializeTICKs` (base_history.go:194).
    ///
    /// # Errors
    /// [`AnalyzerError::NotImplemented`] when no `ticks_to_report_fn` is set, or
    /// any error from [`serialize`](Self::serialize).
    pub fn serialize_ticks(
        &self,
        ticks: &[Tick],
        format: &str,
        writer: &mut dyn Write,
    ) -> Result<(), AnalyzerError> {
        let Some(to_report) = &self.ticks_to_report_fn else {
            return Err(AnalyzerError::NotImplemented);
        };
        let report = to_report(ticks);
        self.serialize(&report, format, writer)
    }

    /// Converts aggregated ticks into a report. Mirrors `ReportFromTICKs`
    /// (base_history.go:205).
    ///
    /// # Errors
    /// [`AnalyzerError::NotImplemented`] when no `ticks_to_report_fn` is set.
    pub fn report_from_ticks(&self, ticks: &[Tick]) -> Result<crate::analyzer::Report, AnalyzerError> {
        match &self.ticks_to_report_fn {
            Some(to_report) => Ok(to_report(ticks)),
            None => Err(AnalyzerError::NotImplemented),
        }
    }

    /// Default no-op plumbing snapshot. Mirrors `SnapshotPlumbing`.
    #[must_use]
    pub fn snapshot_plumbing(&self) -> crate::history::PlumbingSnapshot {
        None
    }

    /// Default no-op snapshot apply. Mirrors `ApplySnapshot`.
    pub fn apply_snapshot(&self, _snapshot: crate::history::PlumbingSnapshot) {}

    /// Default no-op snapshot release. Mirrors `ReleaseSnapshot`.
    pub fn release_snapshot(&self, _snapshot: crate::history::PlumbingSnapshot) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_gojson::{GoMap, GoValue, MapOrigin};

    /// Test metrics mirroring base_history_test.go's DummyMetrics{name,count}.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct DummyMetrics {
        name: String,
        count: i64,
    }

    fn dummy_to_value(m: &DummyMetrics) -> GoValue {
        // Wrapper struct: name then count (declaration order).
        let mut go = GoMap::new(MapOrigin::Struct);
        go.insert("name", GoValue::Str(m.name.clone()));
        go.insert("count", GoValue::Int(m.count));
        GoValue::Map(go)
    }

    fn compute_dummy(r: &crate::analyzer::Report) -> Result<DummyMetrics, AnalyzerError> {
        // Mirror computeDummyMetrics: error when report["error"] == true.
        let is_err = r
            .entries()
            .iter()
            .any(|(k, v)| k == "error" && matches!(v, GoValue::Bool(true)));
        if is_err {
            return Err(AnalyzerError::Other("mock error".into()));
        }
        Ok(DummyMetrics {
            count: 42,
            name: "dummy".into(),
        })
    }

    /// Zero-value descriptor for tests (Go's zero `Descriptor{}`: empty id /
    /// description, default `static` mode).
    fn dummy_descriptor() -> Descriptor {
        Descriptor {
            id: String::new(),
            description: String::new(),
            mode: crate::descriptor::AnalyzerMode::Static,
        }
    }

    fn base_with_compute() -> BaseHistoryAnalyzer<DummyMetrics> {
        let mut b = BaseHistoryAnalyzer::new(dummy_descriptor(), dummy_to_value);
        b.compute_metrics_fn = Some(Box::new(compute_dummy));
        b
    }

    fn empty_report() -> crate::analyzer::Report {
        GoMap::new(MapOrigin::Map)
    }

    // TestBaseHistoryAnalyzer_Metadata (base_history_test.go:43).
    #[test]
    fn metadata() {
        let opts = vec![ConfigurationOption {
            default: cf_pipeline::DefaultValue::String(String::new()),
            name: "test-opt".into(),
            description: String::new(),
            flag: String::new(),
            option_type: cf_pipeline::ConfigurationOptionType::String,
        }];
        let mut base = BaseHistoryAnalyzer::new(
            Descriptor {
                id: "history/dummy".into(),
                description: "Dummy analyzer".into(),
                mode: crate::descriptor::AnalyzerMode::History,
            },
            dummy_to_value,
        );
        base.sequential = true;
        base.cpu_heavy_flag = false;
        base.estimated_state_size = 1024;
        base.estimated_tc_size = 256;
        base.config_options = opts.clone();

        assert_eq!(base.name(), "history/dummy");
        assert_eq!(base.flag(), "dummy");
        assert_eq!(base.description(), "Dummy analyzer");
        assert_eq!(base.descriptor().id, "history/dummy");
        assert!(base.sequential_only());
        assert!(!base.cpu_heavy());
        assert_eq!(base.working_state_size(), 1024);
        assert_eq!(base.avg_tc_size(), 256);
        assert_eq!(base.list_configuration_options().len(), opts.len());
        assert!(base.configure(&empty_report()).is_ok());
    }

    // TestBaseHistoryAnalyzer_FlagNoSlash (base_history_test.go:71).
    #[test]
    fn flag_no_slash() {
        let base = BaseHistoryAnalyzer::new(
            Descriptor {
                id: "dummy".into(),
                description: String::new(),
                mode: crate::descriptor::AnalyzerMode::Static,
            },
            dummy_to_value,
        );
        assert_eq!(base.flag(), "dummy");
    }

    // TestBaseHistoryAnalyzer_Serialize/JSON (base_history_test.go:88).
    #[test]
    fn serialize_json() {
        let base = base_with_compute();
        let mut buf = Vec::new();
        base.serialize(&empty_report(), FORMAT_JSON, &mut buf).expect("serialize");
        let s = String::from_utf8(buf).unwrap();
        // Compact json.Marshal: no trailing newline, struct field order name,count.
        assert_eq!(s, r#"{"name":"dummy","count":42}"#);
    }

    // TestBaseHistoryAnalyzer_Serialize/YAML (base_history_test.go:104).
    #[test]
    fn serialize_yaml() {
        let base = base_with_compute();
        let mut buf = Vec::new();
        base.serialize(&empty_report(), FORMAT_YAML, &mut buf).expect("serialize");
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("count: 42"));
        assert!(s.contains("name: dummy"));
    }

    // TestBaseHistoryAnalyzer_Serialize/Binary (base_history_test.go:116).
    #[test]
    fn serialize_binary() {
        let base = base_with_compute();
        let mut buf = Vec::new();
        base.serialize(&empty_report(), FORMAT_BINARY, &mut buf).expect("serialize");
        let (payload, rest) =
            cf_reportutil::binary::decode_binary_envelope(&buf).expect("decode envelope");
        assert!(rest.is_empty());
        assert_eq!(payload, br#"{"name":"dummy","count":42}"#);
    }

    // TestBaseHistoryAnalyzer_Serialize/Unsupported Format (base_history_test.go:134).
    #[test]
    fn serialize_unsupported_format() {
        let base = base_with_compute();
        let mut buf = Vec::new();
        let err = base.serialize(&empty_report(), "unsupported", &mut buf).unwrap_err();
        assert!(matches!(err, AnalyzerError::UnsupportedFormat(_)));
    }

    // TestBaseHistoryAnalyzer_Serialize/ComputeError (base_history_test.go:143).
    #[test]
    fn serialize_compute_error() {
        let base = base_with_compute();
        let mut err_report = GoMap::new(MapOrigin::Map);
        err_report.insert("error", GoValue::Bool(true));
        let mut buf = Vec::new();
        let err = base.serialize(&err_report, FORMAT_JSON, &mut buf).unwrap_err();
        assert!(err.to_string().contains("mock error"));
    }

    // TestBaseHistoryAnalyzer_Serialize/MissingHook (base_history_test.go:152).
    #[test]
    fn serialize_missing_hook() {
        let base = BaseHistoryAnalyzer::new(dummy_descriptor(), dummy_to_value);
        let mut buf = Vec::new();
        let err = base.serialize(&empty_report(), FORMAT_JSON, &mut buf).unwrap_err();
        assert!(matches!(err, AnalyzerError::MissingComputeMetrics));
    }

    // TestBaseHistoryAnalyzer_SerializeTICKs (base_history_test.go:164).
    #[test]
    fn serialize_ticks() {
        let mut base = base_with_compute();
        base.ticks_to_report_fn = Some(Box::new(|ticks: &[Tick]| {
            let mut r = GoMap::new(MapOrigin::Map);
            if !ticks.is_empty() {
                r.insert("ticks_len", GoValue::Int(ticks.len() as i64));
            }
            r
        }));
        let ticks = vec![Tick { tick: 1, ..Default::default() }, Tick { tick: 2, ..Default::default() }];
        let mut buf = Vec::new();
        base.serialize_ticks(&ticks, FORMAT_JSON, &mut buf).expect("serialize ticks");
        let s = String::from_utf8(buf).unwrap();
        assert_eq!(s, r#"{"name":"dummy","count":42}"#);
    }

    // TestBaseHistoryAnalyzer_SerializeTICKs/MissingHook (base_history_test.go:185).
    #[test]
    fn serialize_ticks_missing_hook() {
        let base = BaseHistoryAnalyzer::new(dummy_descriptor(), dummy_to_value);
        let ticks = vec![Tick { tick: 1, ..Default::default() }];
        let mut buf = Vec::new();
        let err = base.serialize_ticks(&ticks, FORMAT_JSON, &mut buf).unwrap_err();
        assert!(matches!(err, AnalyzerError::NotImplemented));
    }

    // TestBaseHistoryAnalyzer_Snapshots (base_history_test.go:197).
    #[test]
    fn snapshots_are_noops() {
        let base = BaseHistoryAnalyzer::new(dummy_descriptor(), dummy_to_value);
        let snap = base.snapshot_plumbing();
        assert!(snap.is_none());
        base.apply_snapshot(None);
        base.release_snapshot(None);
    }

    #[test]
    fn missing_compute_message_matches_go() {
        assert_eq!(ERR_MISSING_COMPUTE_METRICS, "missing ComputeMetricsFn hook");
    }
}
