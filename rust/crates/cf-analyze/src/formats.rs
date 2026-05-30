//! Output format constants and validation.
//!
//! Faithful port of `internal/analyzers/analyze/formats.go`. The `bin`→`binary`
//! alias and the exact `unsupported format: <fmt>` wording are byte-identity
//! relevant (they appear in CLI error output), so they are reproduced verbatim.

use crate::error::AnalyzeError;

/// `bin` — short CLI alias for binary output. Go `FormatBinAlias`.
pub const FORMAT_BIN_ALIAS: &str = "bin";
/// `binary` — the canonical binary (CFB1) format. Go `FormatBinary`.
pub const FORMAT_BINARY: &str = "binary";
/// `json` — indented JSON. Go `FormatJSON`.
pub const FORMAT_JSON: &str = "json";
/// `yaml` — yaml.v3 output. Go `FormatYAML`.
pub const FORMAT_YAML: &str = "yaml";
/// `plot` — HTML plot output. Go `FormatPlot`.
pub const FORMAT_PLOT: &str = "plot";
/// `text` — human-readable CLI output. Go `FormatText`.
pub const FORMAT_TEXT: &str = "text";
/// `compact` — single-line-per-analyzer static output. Go `FormatCompact`.
pub const FORMAT_COMPACT: &str = "compact";
/// `timeseries` — merged time-series JSON array. Go `FormatTimeSeries`.
pub const FORMAT_TIMESERIES: &str = "timeseries";
/// `ndjson` — one JSON line per TC. Go `FormatNDJSON`.
pub const FORMAT_NDJSON: &str = "ndjson";
/// `timeseries+ndjson` — merged timeseries as NDJSON. Go `FormatTimeSeriesNDJSON`.
pub const FORMAT_TIMESERIES_NDJSON: &str = "timeseries+ndjson";

/// Canonicalizes a user-provided output format string. Port of Go
/// `NormalizeFormat`: lower-cased, trimmed, with the `bin`→`binary` alias.
pub fn normalize_format(format: &str) -> String {
    let normalized = format.trim().to_lowercase();
    if normalized == FORMAT_BIN_ALIAS {
        return FORMAT_BINARY.to_string();
    }
    normalized
}

/// Returns the canonical formats supported by all analyzers. Port of Go
/// `UniversalFormats` (order preserved for parity).
pub fn universal_formats() -> [&'static str; 7] {
    [
        FORMAT_JSON,
        FORMAT_YAML,
        FORMAT_PLOT,
        FORMAT_BINARY,
        FORMAT_TIMESERIES,
        FORMAT_NDJSON,
        FORMAT_TEXT,
    ]
}

/// Returns the formats supported by static analyzers. Port of Go
/// `staticOutputFormats`.
pub fn static_output_formats() -> [&'static str; 6] {
    [
        FORMAT_TEXT,
        FORMAT_COMPACT,
        FORMAT_JSON,
        FORMAT_YAML,
        FORMAT_PLOT,
        FORMAT_BINARY,
    ]
}

/// Validates `format` against the provided support list. Port of Go
/// `ValidateFormat`. Returns the normalized format or
/// [`AnalyzeError::UnsupportedFormat`] carrying the **original** (un-normalized)
/// format string, matching Go's `fmt.Errorf("%w: %s", ..., format)`.
pub fn validate_format(format: &str, supported: &[&str]) -> Result<String, AnalyzeError> {
    let normalized = normalize_format(format);
    for candidate in supported {
        if normalized == normalize_format(candidate) {
            return Ok(normalized);
        }
    }
    Err(AnalyzeError::UnsupportedFormat(format.to_string()))
}

/// Validates `format` against the universal contract. Port of Go
/// `ValidateUniversalFormat`.
pub fn validate_universal_format(format: &str) -> Result<String, AnalyzeError> {
    let normalized = normalize_format(format);
    if universal_formats().contains(&normalized.as_str()) {
        return Ok(normalized);
    }
    Err(AnalyzeError::UnsupportedFormat(format.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Port of TestNormalizeFormat.
    #[test]
    fn normalize() {
        assert_eq!(normalize_format("bin"), FORMAT_BINARY);
        assert_eq!(normalize_format("BIN"), FORMAT_BINARY);
        assert_eq!(normalize_format(" json "), "json");
        assert_eq!(normalize_format("YAML"), "yaml");
        assert_eq!(normalize_format("TimeSeries"), "timeseries");
    }

    // Port of TestValidateFormat.
    #[test]
    fn validate_format_valid_and_invalid() {
        assert!(validate_format("json", &["json", "yaml"]).is_ok());
        let err = validate_format("xml", &["json", "yaml"]).unwrap_err();
        assert!(matches!(err, AnalyzeError::UnsupportedFormat(_)));
    }

    // Port of TestValidateUniversalFormat.
    #[test]
    fn validate_universal() {
        for f in ["json", "yaml", "plot", "binary", "timeseries", "ndjson", "text"] {
            assert!(validate_universal_format(f).is_ok(), "format {f} should be valid");
        }
        assert!(validate_universal_format("invalid").is_err());
    }

    #[test]
    fn unsupported_format_message_uses_original_string() {
        let err = validate_universal_format("XmL").unwrap_err();
        assert_eq!(err.to_string(), "unsupported format: XmL");
    }
}
