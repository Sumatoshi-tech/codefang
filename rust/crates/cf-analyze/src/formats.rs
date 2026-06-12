//! Output format constants and validation.
//!
//! Output-format constants and validation. The `bin`→`binary`
//! alias and the exact `unsupported format: <fmt>` wording are byte-identity
//! relevant (they appear in CLI error output), so they are reproduced verbatim.

/// Error raised when a requested output format is not supported.
///
/// The `Display` impl reproduces `unsupported format: <fmt>` (CLI error
/// contract)
/// byte-for-byte (it appears in CLI error output).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatError {
    /// The format string is not in the supported set. Carries the original
    /// (un-normalized) format string, exactly as supplied.
    Unsupported {
        /// The offending format string, verbatim as supplied.
        format: String,
    },
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported { format } => write!(f, "unsupported format: {format}"),
        }
    }
}

impl std::error::Error for FormatError {}

/// `bin` — short CLI alias for binary output.
pub const FORMAT_BIN_ALIAS: &str = "bin";
/// `binary` — the canonical binary (CFB1) format.
pub const FORMAT_BINARY: &str = "binary";
/// `json` — indented JSON.
pub const FORMAT_JSON: &str = "json";
/// `yaml` — yaml.v3 output.
pub const FORMAT_YAML: &str = "yaml";
/// `plot` — HTML plot output.
pub const FORMAT_PLOT: &str = "plot";
/// `text` — human-readable CLI output.
pub const FORMAT_TEXT: &str = "text";
/// `compact` — single-line-per-analyzer static output.
pub const FORMAT_COMPACT: &str = "compact";
/// `timeseries` — merged time-series JSON array.
pub const FORMAT_TIMESERIES: &str = "timeseries";
/// `ndjson` — one JSON line per TC.
pub const FORMAT_NDJSON: &str = "ndjson";
/// `timeseries+ndjson` — merged timeseries as NDJSON.
pub const FORMAT_TIMESERIES_NDJSON: &str = "timeseries+ndjson";

/// Canonicalizes a user-provided output format string.
/// `NormalizeFormat`: lower-cased, trimmed, with the `bin`→`binary` alias.
#[must_use] 
pub fn normalize_format(format: &str) -> String {
    let normalized = format.trim().to_lowercase();
    if normalized == FORMAT_BIN_ALIAS {
        return FORMAT_BINARY.to_string();
    }
    normalized
}

/// Returns the canonical formats supported by all analyzers.
/// `UniversalFormats` (order preserved for parity).
#[must_use] 
pub const fn universal_formats() -> [&'static str; 7] {
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

/// Returns the formats supported by static analyzers.
#[must_use] 
pub const fn static_output_formats() -> [&'static str; 6] {
    [
        FORMAT_TEXT,
        FORMAT_COMPACT,
        FORMAT_JSON,
        FORMAT_YAML,
        FORMAT_PLOT,
        FORMAT_BINARY,
    ]
}

/// Validates `format` against the provided support list. Returns the normalized format or
/// [`AnalyzeError::UnsupportedFormat`] carrying the **original** (un-normalized)
/// format string (CLI error contract).
pub fn validate_format(format: &str, supported: &[&str]) -> Result<String, FormatError> {
    let normalized = normalize_format(format);
    for candidate in supported {
        if normalized == normalize_format(candidate) {
            return Ok(normalized);
        }
    }
    Err(FormatError::Unsupported {
        format: format.to_string(),
    })
}

/// Validates `format` against the universal contract.
pub fn validate_universal_format(format: &str) -> Result<String, FormatError> {
    let normalized = normalize_format(format);
    if universal_formats().contains(&normalized.as_str()) {
        return Ok(normalized);
    }
    Err(FormatError::Unsupported {
        format: format.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mirrors reference test TestNormalizeFormat.
    #[test]
    fn normalize() {
        assert_eq!(normalize_format("bin"), FORMAT_BINARY);
        assert_eq!(normalize_format("BIN"), FORMAT_BINARY);
        assert_eq!(normalize_format(" json "), "json");
        assert_eq!(normalize_format("YAML"), "yaml");
        assert_eq!(normalize_format("TimeSeries"), "timeseries");
    }

    // Mirrors reference test TestValidateFormat.
    #[test]
    fn validate_format_valid_and_invalid() {
        assert!(validate_format("json", &["json", "yaml"]).is_ok());
        let err = validate_format("xml", &["json", "yaml"]).unwrap_err();
        assert!(matches!(err, FormatError::Unsupported { .. }));
    }

    // Mirrors reference test TestValidateUniversalFormat.
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
