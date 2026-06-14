//! `cf-analyze` — analyzer registry/dispatch hub and cross-format report
//! conversion.
//!
//! This crate is the keystone the whole analyzer ecosystem builds on: every
//! concrete analyzer, the common/plumbing layers, the framework, the MCP server,
//! and both binaries reference it. Per specs/rust-rewrite/DESIGN.md §3 it sits at
//! **tier 3-5** and MUST stay strictly **below** the framework — it defines the
//! analyzer contracts and the serialization hub but pulls in no framework code.
//!
//! # What lives here
//!
//! - [`Report`] / [`GoMap`] / [`GoValue`] — the dynamic string-keyed report
//!   model, reusing the [`cf_gojson`] value tree so serialization is byte-exact.
//! - The analyzer interfaces ([`Analyzer`], [`FormattableAnalyzer`],
//!   [`StaticAnalyzer`], [`RawFileAnalyzer`], [`HistoryAnalyzer`],
//!   [`Parallelizable`], …).
//! - [`BaseHistoryAnalyzer`] — the embeddable default `HistoryAnalyzer`
//!   implementation.
//! - The **cross-format conversion hub** ([`write_converted_output`],
//!   [`UnifiedModel`], [`MergedTimeSeries`], [`StreamingSink`], the format
//!   constants/validation) built over [`cf_gojson`] / [`cf_goyaml`] /
//!   [`cf_reportutil`].
//!
//! # Byte-identity discipline (DESIGN §2)
//!
//! Every machine format routes through the tier-0 encoders. Dynamic report maps
//! ([`Report`], the flattened `MergedCommitData`) are [`GoMap`]s built with
//! [`MapOrigin::Map`] so the encoder **byte-sorts** their keys at encode time,
//! per the report-format contract for dynamic maps. Wrapper types ([`UnifiedModel`],
//! [`AnalyzerResult`], [`MergedTimeSeries`], [`AnalysisMetadata`],
//! [`NdjsonLine`]) are built with [`MapOrigin::Struct`] so fields stay in source
//! declaration order and honor `omitempty`.
#![allow(clippy::module_name_repetitions)]

pub use cf_gojson::{GoMap, GoValue, MapOrigin};

pub mod aggregation_mode;
pub mod aggregator;
pub mod analyzer;
pub mod base_history;
pub mod commits_by_tick;
pub mod conversion;
pub mod descriptor;
pub mod error;
pub mod formats;
pub mod glob;
pub mod history;
pub mod interfaces;
pub mod json_parse;
pub mod metadata;
pub mod registry;
pub mod report;
pub mod schema_registry;
pub mod streaming_sink;
pub mod tc;
pub mod thresholds;
pub mod typed_collection;
pub mod timeseries;

pub use aggregation_mode::{AggregationMode, AggregationModeAware};
pub use aggregator::GenericAggregator;
pub use interfaces::{
    Aggregator, AggregatorOptions, AggregatorSpillInfo, CommitStatsDrainer, CONFIG_TMP_DIR,
};
pub use analyzer::{
    Analyzer, FormattableAnalyzer, RawFileAnalyzer, Report, ResultAggregator, StateSizer,
    StaticAnalyzer, Thresholds, VisitorProvider, ERR_ANALYSIS_FAILED, ERR_NIL_ROOT_NODE,
    ERR_UNREGISTERED_ANALYZER,
};
pub use base_history::{
    BaseHistoryAnalyzer, MetricsSerializer, ERR_MISSING_COMPUTE_METRICS,
};
pub use commits_by_tick::build_commits_by_tick;
pub use conversion::{
    decode_combined_binary_reports, parse_unified_model_json, resolve_formats,
    resolve_input_format, write_converted_output, AnalyzerResult, UnifiedModel,
    UNIFIED_MODEL_VERSION,
};
pub use descriptor::{new_descriptor, normalize_name, Descriptor};
pub use formats::{
    normalize_format, universal_formats, validate_format, validate_universal_format,
    FormatError, FORMAT_BINARY, FORMAT_BIN_ALIAS, FORMAT_COMPACT, FORMAT_JSON, FORMAT_NDJSON,
    FORMAT_PLOT, FORMAT_TEXT, FORMAT_TIMESERIES, FORMAT_TIMESERIES_NDJSON, FORMAT_YAML,
};
pub use history::{AnalyzerMode, MODE_HISTORY, MODE_STATIC};
pub use metadata::{AnalysisMetadata, Clock, SystemClock};
pub use schema_registry::{schema_for_analyzer, AnalyzerSchema, FieldMeta};
pub use streaming_sink::{NdjsonLine, StreamingSink};
pub use tc::{CommitHash, Tc, Tick};
pub use timeseries::{
    build_merged_time_series_direct, write_merged_time_series, write_time_series_ndjson,
    AnalyzerData, CommitMeta, MergedCommitData, MergedTimeSeries, TimeSeriesError,
    TIMESERIES_MODEL_VERSION,
};
pub use typed_collection::{
    TypedCollection, DIRECTORY_KEY, LANGUAGE_KEY, SOURCE_FILE_KEY,
};

/// Sentinel error text returned by stub methods that are not yet wired.
/// Kept crate-public so analyzers can match on it.
pub const ERR_NOT_IMPLEMENTED: &str = "not implemented";
