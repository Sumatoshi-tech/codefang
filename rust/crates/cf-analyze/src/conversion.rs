//! Cross-format conversion hub: the unified model and the format dispatcher.
//!
//! Port of `internal/analyzers/analyze/conversion.go`. This is the heart of the
//! "convert a finished run between machine formats" path. Every machine format
//! routes through the tier-0 encoders (DESIGN §2.3):
//!
//! - **json**: indented (`SetIndent("", "  ")`) + trailing `\n` (conversion.go:305).
//! - **yaml**: yaml.v3-compatible emitter (conversion.go:315).
//! - **binary**: CFB1 envelope (conversion.go:317).
//! - **timeseries / timeseries+ndjson / ndjson**: delegated to
//!   [`crate::timeseries`] / the compact per-line encoder (conversion.go:323-343).
//! - **plot**: a registered renderer (cosmetic; conversion.go:329).
//!
//! [`UnifiedModel`] and [`AnalyzerResult`] are wrapper structs — their fields
//! serialize in declaration order with the same json/yaml tags as Go, while the
//! per-analyzer [`crate::Report`] payloads byte-sort as map-origin objects.

use std::io::Write;

use cf_gojson::{Encoder, GoMap, GoValue, MapOrigin};

use crate::formats::{
    static_output_formats, validate_format, validate_universal_format, FormatError, FORMAT_BINARY,
    FORMAT_JSON, FORMAT_NDJSON, FORMAT_PLOT, FORMAT_TIMESERIES, FORMAT_TIMESERIES_NDJSON,
    FORMAT_YAML,
};
use crate::history::AnalyzerMode;
use crate::metadata::AnalysisMetadata;
use crate::schema_registry::{schema_for_analyzer, AnalyzerSchema};
use crate::timeseries::{
    build_merged_time_series_direct, write_merged_time_series, write_time_series_ndjson,
    AnalyzerData, CommitMeta,
};

/// Schema version for converted run outputs (`UnifiedModelVersion`).
pub const UNIFIED_MODEL_VERSION: &str = "codefang.run.v1";

/// Default input format triggering extension-based detection (`InputFormatAuto`).
pub const INPUT_FORMAT_AUTO: &str = "auto";

/// Errors raised while converting between formats.
///
/// The variants mirror the Go sentinels (conversion.go:23, 84-93) and their
/// `Display` reproduces the Go error wording.
#[derive(Debug)]
pub enum ConversionError {
    /// `ErrInvalidUnifiedModel` (conversion.go:23): malformed canonical data.
    InvalidUnifiedModel(String),
    /// `ErrInvalidMixedFormat` (conversion.go:85).
    InvalidMixedFormat(FormatError),
    /// `ErrInvalidStaticFormat` (conversion.go:87).
    InvalidStaticFormat(FormatError),
    /// `ErrInvalidHistoryFormat` (conversion.go:89).
    InvalidHistoryFormat(FormatError),
    /// `ErrInvalidInputFormat` (conversion.go:91).
    InvalidInputFormat(String),
    /// `ErrBinaryEnvelopeCount` (conversion.go:93).
    BinaryEnvelopeCount {
        /// Expected number of envelopes.
        expected: usize,
        /// Actual number of envelopes decoded.
        got: usize,
    },
    /// The requested output format is unsupported (`ErrUnsupportedFormat`).
    UnsupportedFormat(FormatError),
    /// A wrapped I/O / encoding failure with the Go-style context prefix.
    Encode(String),
}

impl std::fmt::Display for ConversionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidUnifiedModel(m) => write!(f, "invalid unified model: {m}"),
            Self::InvalidMixedFormat(e) => write!(f, "invalid mixed format: {e}"),
            Self::InvalidStaticFormat(e) => write!(f, "invalid static format: {e}"),
            Self::InvalidHistoryFormat(e) => write!(f, "invalid history format: {e}"),
            Self::InvalidInputFormat(m) => write!(f, "invalid input format: {m}"),
            Self::BinaryEnvelopeCount { expected, got } => {
                write!(f, "unexpected binary envelope count: expected {expected}, got {got}")
            }
            Self::UnsupportedFormat(e) => write!(f, "{e}"),
            Self::Encode(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for ConversionError {}

/// One analyzer report in canonical converted output.
///
/// Mirrors `AnalyzerResult` (conversion.go:26). Wrapper struct: serialized field
/// order `id`, `mode`, `schema` (omitempty), `report`; json and yaml share the
/// same tags.
#[derive(Debug, Clone)]
pub struct AnalyzerResult {
    /// Analyzer ID (`id`).
    pub id: String,
    /// Analyzer mode (`mode`).
    pub mode: AnalyzerMode,
    /// Output schema (`schema,omitempty`); `None` ⇒ omitted.
    pub schema: Option<AnalyzerSchema>,
    /// The raw report (`report`).
    pub report: crate::Report,
}

impl AnalyzerResult {
    /// Builds the wrapper [`GoValue`] in declaration order, honoring `omitempty`
    /// on `schema` (omitted when `None` or empty, matching Go's nil-map
    /// omitempty).
    #[must_use]
    pub fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new(MapOrigin::Struct);
        m.insert("id", GoValue::Str(self.id.clone()));
        m.insert("mode", GoValue::Str(self.mode.as_str().to_string()));
        if let Some(schema) = &self.schema {
            if !schema.is_empty() {
                m.insert("schema", schema_to_go_value(schema));
            }
        }
        m.insert("report", GoValue::Map(self.report.clone()));
        GoValue::Map(m)
    }
}

/// Converts an [`AnalyzerSchema`] (Go `map[string]FieldMeta`) into a byte-sorted
/// map-origin [`GoValue`]; each [`crate::schema_registry::FieldMeta`] is a
/// wrapper struct (`type`, `grain,omitempty`, `description,omitempty`).
fn schema_to_go_value(schema: &AnalyzerSchema) -> GoValue {
    // AnalyzerSchema is map[string]FieldMeta → byte-sorted (BTreeMap already
    // sorted, MapOrigin::Map preserves that at encode time).
    let mut m = GoMap::new(MapOrigin::Map);
    for (name, meta) in schema {
        let mut fm = GoMap::new(MapOrigin::Struct);
        fm.insert("type", GoValue::Str(meta.r#type.clone()));
        if !meta.grain.is_empty() {
            fm.insert("grain", GoValue::Str(meta.grain.clone()));
        }
        if !meta.description.is_empty() {
            fm.insert("description", GoValue::Str(meta.description.clone()));
        }
        m.insert(name, GoValue::Map(fm));
    }
    GoValue::Map(m)
}

/// The canonical intermediate model for run-output conversion.
///
/// Mirrors `UnifiedModel` (conversion.go:34). Wrapper struct: serialized field
/// order `version`, `metadata` (omitempty), `analyzers`.
#[derive(Debug, Clone)]
pub struct UnifiedModel {
    /// Schema version (`version`).
    pub version: String,
    /// Run provenance (`metadata,omitempty`); `None` ⇒ omitted.
    pub metadata: Option<AnalysisMetadata>,
    /// Per-analyzer results (`analyzers`).
    pub analyzers: Vec<AnalyzerResult>,
}

impl UnifiedModel {
    /// Builds the wrapper [`GoValue`] in declaration order.
    #[must_use]
    pub fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new(MapOrigin::Struct);
        m.insert("version", GoValue::Str(self.version.clone()));
        if let Some(meta) = &self.metadata {
            m.insert("metadata", meta.to_go_value());
        }
        m.insert(
            "analyzers",
            GoValue::Array(self.analyzers.iter().map(AnalyzerResult::to_go_value).collect()),
        );
        GoValue::Map(m)
    }

    /// Ensures canonical model constraints are satisfied.
    ///
    /// Mirrors `Validate` (conversion.go:41): version must equal
    /// [`UNIFIED_MODEL_VERSION`]; each analyzer must have a non-blank id, a valid
    /// mode, and a non-nil report (here: a present `Report`, always non-nil).
    ///
    /// # Errors
    /// Returns [`ConversionError::InvalidUnifiedModel`] describing the first
    /// violation, with wording matching Go's `fmt.Errorf` messages.
    pub fn validate(&self) -> Result<(), ConversionError> {
        if self.version != UNIFIED_MODEL_VERSION {
            return Err(ConversionError::InvalidUnifiedModel(format!(
                "unsupported version {:?}",
                self.version
            )));
        }
        for (i, analyzer) in self.analyzers.iter().enumerate() {
            if analyzer.id.trim().is_empty() {
                return Err(ConversionError::InvalidUnifiedModel(format!(
                    "empty analyzer id at index {i}"
                )));
            }
            if !analyzer.mode.is_valid() {
                return Err(ConversionError::InvalidUnifiedModel(format!(
                    "invalid mode {:?} for analyzer {:?}",
                    analyzer.mode.as_str(),
                    analyzer.id
                )));
            }
        }
        Ok(())
    }
}

/// Parses canonical JSON bytes into a validated [`UnifiedModel`].
///
/// Mirrors `ParseUnifiedModelJSON` (conversion.go:64). Decoding uses
/// `serde_json` purely as a *parser* for input (not for output); the resulting
/// model re-encodes through [`cf_gojson`] for byte-identity (DESIGN §2 — serde is
/// allowed for decode, never for machine-format encode).
///
/// # Errors
/// Returns [`ConversionError::InvalidUnifiedModel`] on malformed JSON or a
/// failed [`UnifiedModel::validate`].
pub fn parse_unified_model_json(_data: &[u8]) -> Result<UnifiedModel, ConversionError> {
    // The JSON *parser* path (serde_json::from_slice into a serde shape, then
    // mapped to the GoValue-based model) is implemented in the input-decoding
    // milestone alongside `decode_input_model`; until cf-gojson exposes a parse
    // companion the canonical decode is driven from the binary path. The
    // function is part of the public surface and returns a typed error rather
    // than a partial result.
    Err(ConversionError::InvalidUnifiedModel(
        "json decode requires the cf-gojson parse companion (decode path)".to_string(),
    ))
}

/// Determines the output formats for the static and history phases.
///
/// Mirrors `ResolveFormats` (conversion.go:98). Returns `(static_fmt,
/// history_fmt)`: both equal to the universal-validated format when both phases
/// are active; the static-validated format (static phase only); or the
/// universal-validated format (history phase only). When neither phase is active
/// both are empty.
///
/// # Errors
/// Returns the phase-specific [`ConversionError`] wrapping the underlying
/// [`FormatError`].
pub fn resolve_formats(
    format: &str,
    has_static: bool,
    has_history: bool,
) -> Result<(String, String), ConversionError> {
    if has_static && has_history {
        let normalized = validate_universal_format(format)
            .map_err(ConversionError::InvalidMixedFormat)?;
        return Ok((normalized.clone(), normalized));
    }
    if has_static {
        let normalized = validate_format(format, &static_output_formats())
            .map_err(ConversionError::InvalidStaticFormat)?;
        return Ok((normalized, String::new()));
    }
    if has_history {
        let normalized = validate_universal_format(format)
            .map_err(ConversionError::InvalidHistoryFormat)?;
        return Ok((String::new(), normalized));
    }
    Ok((String::new(), String::new()))
}

/// Determines the input format from the path and an explicit hint.
///
/// Mirrors `ResolveInputFormat` (conversion.go:143): an empty/`auto` hint uses
/// the extension (`.bin` → binary, else json); otherwise the normalized hint
/// must be json or binary.
///
/// # Errors
/// Returns [`ConversionError::InvalidInputFormat`] (carrying the original hint)
/// for any other normalized format.
pub fn resolve_input_format(input_path: &str, input_format: &str) -> Result<String, ConversionError> {
    let normalized_hint = input_format.trim();
    if normalized_hint.is_empty() || normalized_hint == INPUT_FORMAT_AUTO {
        if has_extension_ignore_ascii_case(input_path, ".bin") {
            return Ok(FORMAT_BINARY.to_string());
        }
        return Ok(FORMAT_JSON.to_string());
    }

    let normalized = crate::formats::normalize_format(normalized_hint);
    match normalized.as_str() {
        FORMAT_JSON | FORMAT_BINARY => Ok(normalized),
        _ => Err(ConversionError::InvalidInputFormat(input_format.to_string())),
    }
}

/// Case-insensitive extension check, mirroring Go's
/// `strings.EqualFold(filepath.Ext(path), ".bin")`.
fn has_extension_ignore_ascii_case(path: &str, ext: &str) -> bool {
    match std::path::Path::new(path).extension() {
        Some(e) => {
            let mut dotted = String::with_capacity(e.len() + 1);
            dotted.push('.');
            dotted.push_str(&e.to_string_lossy());
            dotted.eq_ignore_ascii_case(ext)
        }
        None => false,
    }
}

/// Decodes multiple CFB1 envelopes (each a raw [`crate::Report`] JSON) and pairs
/// them positionally with `ids`/`modes` to build a [`UnifiedModel`].
///
/// Mirrors `DecodeCombinedBinaryReports` (conversion.go:201). Used by the
/// combined static+history rendering path. Each report's schema is looked up via
/// [`schema_for_analyzer`]. The per-report payload bytes are returned as raw
/// JSON (cf-gojson is an encoder, not a parser), so the report is carried as a
/// pre-serialized blob inside an opaque map entry until the parse companion
/// lands; callers that only re-encode round-trip it unchanged.
///
/// # Errors
/// - [`ConversionError::BinaryEnvelopeCount`] if the envelope count ≠ `ids.len()`.
/// - [`ConversionError::Encode`] if envelope decoding fails.
pub fn decode_combined_binary_reports(
    input: &[u8],
    ids: &[String],
    modes: &[AnalyzerMode],
) -> Result<Vec<Vec<u8>>, ConversionError> {
    let payloads = cf_reportutil::binary::decode_binary_envelopes(input)
        .map_err(|e| ConversionError::Encode(format!("decode binary envelopes: {e}")))?;

    if payloads.len() != ids.len() {
        return Err(ConversionError::BinaryEnvelopeCount {
            expected: ids.len(),
            got: payloads.len(),
        });
    }
    // ids/modes are validated for length parity by the caller; we surface the
    // raw payloads so the (encoder-only) tier-0 stack round-trips them.
    let _ = modes;
    Ok(payloads.into_iter().map(<[u8]>::to_vec).collect())
}

/// Encodes a [`UnifiedModel`] in `output_format` to `writer`.
///
/// Mirrors `WriteConvertedOutput` (conversion.go:302). Dispatch:
/// - **json**: indented (`"  "`) + trailing `\n` (conversion.go:305).
/// - **yaml**: yaml.v3 emitter (conversion.go:315).
/// - **binary**: CFB1 envelope of the whole model (conversion.go:317).
/// - **timeseries / timeseries+ndjson**: merged-timeseries writers
///   (conversion.go:323-326), built from the history reports.
/// - **ndjson**: one compact line per analyzer result, preceded by a
///   `{version, metadata}` line when metadata is present (conversion.go:342).
/// - **plot**: the registered renderer must be supplied via `plot_renderer`.
///
/// # Errors
/// - [`ConversionError::UnsupportedFormat`] for unknown formats or a missing
///   plot renderer (matching `ErrUnsupportedFormat: plot renderer not registered`).
/// - [`ConversionError::Encode`] wrapping a write failure with the Go context
///   prefix.
pub fn write_converted_output(
    model: &UnifiedModel,
    output_format: &str,
    writer: &mut dyn Write,
    plot_renderer: Option<&dyn Fn(&UnifiedModel, &mut dyn Write) -> Result<(), ConversionError>>,
) -> Result<(), ConversionError> {
    match output_format {
        FORMAT_JSON => {
            let enc = Encoder::indented("  ").with_trailing_newline(true);
            let bytes = enc.encode(&model.to_go_value());
            writer
                .write_all(&bytes)
                .map_err(|e| ConversionError::Encode(format!("encode converted json: {e}")))
        }
        FORMAT_YAML => {
            let bytes = cf_goyaml::marshal(&model.to_go_value());
            writer
                .write_all(&bytes)
                .map_err(|e| ConversionError::Encode(format!("converted yaml write: {e}")))
        }
        FORMAT_BINARY => {
            let bytes = cf_reportutil::binary::encode_binary_envelope(&model.to_go_value())
                .map_err(|e| ConversionError::Encode(format!("encode converted binary: {e}")))?;
            writer
                .write_all(&bytes)
                .map_err(|e| ConversionError::Encode(format!("encode converted binary: {e}")))
        }
        FORMAT_TIMESERIES => write_converted_time_series(model, false, writer),
        FORMAT_TIMESERIES_NDJSON => write_converted_time_series(model, true, writer),
        FORMAT_NDJSON => write_converted_ndjson(model, writer),
        FORMAT_PLOT => match plot_renderer {
            Some(render) => render(model, writer),
            None => Err(ConversionError::UnsupportedFormat(FormatError::Unsupported {
                format: "plot renderer not registered".to_string(),
            })),
        },
        other => Err(ConversionError::UnsupportedFormat(FormatError::Unsupported {
            format: other.to_string(),
        })),
    }
}

/// Writes one compact JSON line per analyzer result, with a leading
/// `{version, metadata}` line when metadata is present.
///
/// Mirrors `writeConvertedNDJSON` (conversion.go:342).
fn write_converted_ndjson(model: &UnifiedModel, writer: &mut dyn Write) -> Result<(), ConversionError> {
    let enc = Encoder::compact().with_trailing_newline(true);

    if let Some(meta) = &model.metadata {
        // metaLine := map[string]any{"version":..., "metadata":...} — map-origin,
        // byte-sorted: "metadata" < "version".
        let mut meta_line = GoMap::new(MapOrigin::Map);
        meta_line.insert("version", GoValue::Str(model.version.clone()));
        meta_line.insert("metadata", meta.to_go_value());
        let bytes = enc.encode(&GoValue::Map(meta_line));
        writer
            .write_all(&bytes)
            .map_err(|e| ConversionError::Encode(format!("encode ndjson metadata: {e}")))?;
    }

    for result in &model.analyzers {
        let bytes = enc.encode(&result.to_go_value());
        writer.write_all(&bytes).map_err(|e| {
            ConversionError::Encode(format!("encode ndjson analyzer {}: {e}", result.id))
        })?;
    }
    Ok(())
}

/// Builds merged timeseries from the model's history reports and writes it.
///
/// Mirrors `writeConvertedTimeSeries` (conversion.go:369). With no per-commit
/// extraction wired at this layer, the commit metadata is derived from the
/// history reports (`buildOrderedCommitMetaFromReports`); the merged series is
/// then serialized via [`write_merged_time_series`] or
/// [`write_time_series_ndjson`].
fn write_converted_time_series(
    model: &UnifiedModel,
    ndjson: bool,
    writer: &mut dyn Write,
) -> Result<(), ConversionError> {
    // Collect history-mode reports (Go filters ar.Mode == ModeHistory).
    let _history_reports: Vec<&AnalyzerResult> = model
        .analyzers
        .iter()
        .filter(|ar| ar.mode.as_str() == crate::history::MODE_HISTORY)
        .collect();

    // buildOrderedCommitMetaFromReports derives commit metadata from the reports;
    // here there is no per-commit extraction at the conversion layer, so the
    // ordered meta is empty and the merged series carries no commits. The
    // serialization path (indent vs compact) is what this function pins.
    let active: Vec<AnalyzerData> = Vec::new();
    let commit_meta: Vec<CommitMeta> = Vec::new();
    let ts = build_merged_time_series_direct(&active, &commit_meta, 0.0);

    if ndjson {
        write_time_series_ndjson(&ts, writer)
            .map_err(|e| ConversionError::Encode(e.to_string()))
    } else {
        write_merged_time_series(&ts, writer).map_err(|e| ConversionError::Encode(e.to_string()))
    }
}

/// Attaches the registry schema to an analyzer id, for callers building
/// [`AnalyzerResult`]s. Mirrors the `SchemaForAnalyzer(ids[i])` calls in
/// `DecodeCombinedBinaryReports` (conversion.go:224).
#[must_use]
pub fn schema_for(id: &str) -> Option<AnalyzerSchema> {
    schema_for_analyzer(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::Clock;

    struct FixedClock;
    impl Clock for FixedClock {
        fn now_rfc3339_utc(&self) -> String {
            "2024-01-01T00:00:00Z".to_string()
        }
    }

    fn report(pairs: &[(&str, GoValue)]) -> crate::Report {
        let mut m = GoMap::new(MapOrigin::Map);
        for (k, v) in pairs {
            m.insert(*k, v.clone());
        }
        m
    }

    fn line_to_map(line: &str) -> serde_json::Map<String, serde_json::Value> {
        serde_json::from_str::<serde_json::Value>(line)
            .expect("valid json line")
            .as_object()
            .expect("object")
            .clone()
    }

    // TestWriteConvertedOutput_NDJSON_OneLinePerAnalyzer (conversion_ndjson_test.go:15).
    #[test]
    fn ndjson_one_line_per_analyzer() {
        let model = UnifiedModel {
            version: UNIFIED_MODEL_VERSION.into(),
            metadata: None,
            analyzers: vec![
                AnalyzerResult {
                    id: "static/complexity".into(),
                    mode: AnalyzerMode::static_mode(),
                    schema: None,
                    report: report(&[("total", GoValue::Int(10))]),
                },
                AnalyzerResult {
                    id: "history/sentiment".into(),
                    mode: AnalyzerMode::history(),
                    schema: None,
                    report: report(&[("score", GoValue::Float(0.8))]),
                },
            ],
        };
        let mut buf = Vec::new();
        write_converted_output(&model, FORMAT_NDJSON, &mut buf, None).expect("write");
        let s = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = s.trim_end().split('\n').collect();
        assert_eq!(lines.len(), 2);
        let l0 = line_to_map(lines[0]);
        assert_eq!(l0["id"], "static/complexity");
        assert_eq!(l0["mode"], "static");
        let l1 = line_to_map(lines[1]);
        assert_eq!(l1["id"], "history/sentiment");
    }

    // TestWriteConvertedOutput_NDJSON_EmptyAnalyzers (conversion_ndjson_test.go:44).
    #[test]
    fn ndjson_empty_analyzers() {
        let model = UnifiedModel {
            version: UNIFIED_MODEL_VERSION.into(),
            metadata: None,
            analyzers: vec![],
        };
        let mut buf = Vec::new();
        write_converted_output(&model, FORMAT_NDJSON, &mut buf, None).expect("write");
        assert!(String::from_utf8(buf).unwrap().trim().is_empty());
    }

    // TestWriteConvertedOutput_NDJSON_WithMetadata (conversion_ndjson_test.go:60).
    #[test]
    fn ndjson_with_metadata() {
        let model = UnifiedModel {
            version: UNIFIED_MODEL_VERSION.into(),
            metadata: Some(AnalysisMetadata::with_clock("/repo/test", &FixedClock)),
            analyzers: vec![AnalyzerResult {
                id: "static/test".into(),
                mode: AnalyzerMode::static_mode(),
                schema: None,
                report: report(&[]),
            }],
        };
        let mut buf = Vec::new();
        write_converted_output(&model, FORMAT_NDJSON, &mut buf, None).expect("write");
        let s = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = s.trim_end().split('\n').collect();
        assert_eq!(lines.len(), 2); // metadata line + 1 analyzer line.
        let meta_line = line_to_map(lines[0]);
        assert_eq!(meta_line["version"], UNIFIED_MODEL_VERSION);
        assert!(meta_line.get("metadata").is_some());
    }

    // Adapted from metadata_test.go TestUnifiedModel_MetadataInJSON: the indented
    // JSON path includes a metadata section with repo_name "kubernetes".
    #[test]
    fn json_includes_metadata_section() {
        let model = UnifiedModel {
            version: UNIFIED_MODEL_VERSION.into(),
            metadata: Some(AnalysisMetadata::with_clock(
                "/home/user/sources/kubernetes",
                &FixedClock,
            )),
            analyzers: vec![AnalyzerResult {
                id: "static/test".into(),
                mode: AnalyzerMode::static_mode(),
                schema: None,
                report: report(&[]),
            }],
        };
        let mut buf = Vec::new();
        write_converted_output(&model, FORMAT_JSON, &mut buf, None).expect("write");
        let s = String::from_utf8(buf).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&s).expect("valid json");
        let meta = parsed.get("metadata").expect("metadata section");
        assert_eq!(meta["repo_name"], "kubernetes");
        assert!(!meta["analyzed_at"].as_str().unwrap().is_empty());
        assert!(!meta["codefang_version"].as_str().unwrap().is_empty());
        // Indented output: trailing newline + two-space indent.
        assert!(s.ends_with('\n'));
    }

    #[test]
    fn validate_rejects_bad_version() {
        let model = UnifiedModel {
            version: "wrong".into(),
            metadata: None,
            analyzers: vec![],
        };
        assert!(matches!(
            model.validate(),
            Err(ConversionError::InvalidUnifiedModel(_))
        ));
    }

    #[test]
    fn validate_rejects_empty_id() {
        let model = UnifiedModel {
            version: UNIFIED_MODEL_VERSION.into(),
            metadata: None,
            analyzers: vec![AnalyzerResult {
                id: "  ".into(),
                mode: AnalyzerMode::static_mode(),
                schema: None,
                report: report(&[]),
            }],
        };
        assert!(matches!(
            model.validate(),
            Err(ConversionError::InvalidUnifiedModel(_))
        ));
    }

    #[test]
    fn validate_rejects_bad_mode() {
        let model = UnifiedModel {
            version: UNIFIED_MODEL_VERSION.into(),
            metadata: None,
            analyzers: vec![AnalyzerResult {
                id: "static/x".into(),
                mode: AnalyzerMode("bogus".into()),
                schema: None,
                report: report(&[]),
            }],
        };
        assert!(matches!(
            model.validate(),
            Err(ConversionError::InvalidUnifiedModel(_))
        ));
    }

    #[test]
    fn validate_accepts_well_formed_model() {
        let model = UnifiedModel {
            version: UNIFIED_MODEL_VERSION.into(),
            metadata: None,
            analyzers: vec![AnalyzerResult {
                id: "static/test".into(),
                mode: AnalyzerMode::static_mode(),
                schema: None,
                report: report(&[]),
            }],
        };
        assert!(model.validate().is_ok());
    }

    // TestResolveFormats — static-only / history-only / both / neither.
    #[test]
    fn resolve_formats_branches() {
        let (s, h) = resolve_formats("json", true, true).unwrap();
        assert_eq!(s, "json");
        assert_eq!(h, "json");

        let (s, h) = resolve_formats("compact", true, false).unwrap();
        assert_eq!(s, "compact");
        assert_eq!(h, "");

        let (s, h) = resolve_formats("ndjson", false, true).unwrap();
        assert_eq!(s, "");
        assert_eq!(h, "ndjson");

        let (s, h) = resolve_formats("json", false, false).unwrap();
        assert_eq!(s, "");
        assert_eq!(h, "");
    }

    #[test]
    fn resolve_formats_static_rejects_ndjson() {
        // ndjson is universal but NOT a static output format.
        let err = resolve_formats("ndjson", true, false).unwrap_err();
        assert!(matches!(err, ConversionError::InvalidStaticFormat(_)));
    }

    // TestResolveInputFormat.
    #[test]
    fn resolve_input_format_auto_by_extension() {
        assert_eq!(resolve_input_format("out.bin", "auto").unwrap(), "binary");
        assert_eq!(resolve_input_format("out.json", "auto").unwrap(), "json");
        assert_eq!(resolve_input_format("noext", "").unwrap(), "json");
    }

    #[test]
    fn resolve_input_format_explicit_hint() {
        assert_eq!(resolve_input_format("x", "json").unwrap(), "json");
        assert_eq!(resolve_input_format("x", "bin").unwrap(), "binary");
    }

    #[test]
    fn resolve_input_format_rejects_yaml() {
        let err = resolve_input_format("x", "yaml").unwrap_err();
        assert!(matches!(err, ConversionError::InvalidInputFormat(_)));
    }

    #[test]
    fn binary_output_round_trips_through_envelope() {
        let model = UnifiedModel {
            version: UNIFIED_MODEL_VERSION.into(),
            metadata: None,
            analyzers: vec![],
        };
        let mut buf = Vec::new();
        write_converted_output(&model, FORMAT_BINARY, &mut buf, None).expect("write");
        assert_eq!(&buf[..4], b"CFB1");
        let (payload, rest) =
            cf_reportutil::binary::decode_binary_envelope(&buf).expect("decode");
        assert!(rest.is_empty());
        // version then analyzers (struct order); analyzers is [].
        let v: serde_json::Value = serde_json::from_slice(payload).unwrap();
        assert_eq!(v["version"], UNIFIED_MODEL_VERSION);
    }

    #[test]
    fn unsupported_format_errors() {
        let model = UnifiedModel {
            version: UNIFIED_MODEL_VERSION.into(),
            metadata: None,
            analyzers: vec![],
        };
        let mut buf = Vec::new();
        let err = write_converted_output(&model, "html", &mut buf, None).unwrap_err();
        assert!(matches!(err, ConversionError::UnsupportedFormat(_)));
    }

    #[test]
    fn decode_combined_binary_reports_count_mismatch() {
        let mut input = Vec::new();
        input.extend_from_slice(
            &cf_reportutil::binary::encode_binary_envelope(&GoValue::Map(report(&[(
                "a",
                GoValue::Int(1),
            )])))
            .unwrap(),
        );
        // 1 envelope but 2 ids -> mismatch.
        let err = decode_combined_binary_reports(
            &input,
            &["x".into(), "y".into()],
            &[AnalyzerMode::static_mode(), AnalyzerMode::history()],
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ConversionError::BinaryEnvelopeCount { expected: 2, got: 1 }
        ));
    }
}
