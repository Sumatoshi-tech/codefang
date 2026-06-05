//! Output-format constants, normalization, validation, and resolution — the
//! Rust port of Go `internal/analyzers/analyze/formats.go` plus the
//! `ResolveFormats` / `ResolveInputFormat` helpers from `conversion.go`.
//!
//! `cf-commands` reproduces these here (rather than re-exporting `cf-analyze`)
//! because the format gate is exercised directly by the `run` command's flag
//! handling and because `cf-analyze` is not yet building in this tree. When
//! `cf-analyze` compiles, this module's constants and functions are kept
//! byte-behavior-identical to `cf_analyze::formats` so the eventual switch is a
//! pure dependency change with no behavior delta.
//!
//! # Behavior parity
//!
//! - [`normalize_format`] trims surrounding whitespace, lower-cases (Go
//!   `strings.ToLower(strings.TrimSpace(...))`), and maps the `bin` alias to
//!   [`FORMAT_BINARY`], mirroring Go `NormalizeFormat`.
//! - [`validate_format`] accepts the per-analyzer machine formats
//!   (`json`/`yaml`/`binary`/`text`/`compact`); it intentionally rejects
//!   [`FORMAT_PLOT`], matching the Go `ValidateFormat` switch.
//! - [`validate_universal_format`] accepts the cross-format-conversion set used
//!   by `run`/`render` (`json`/`yaml`/`binary`/`timeseries`/`ndjson`/`compact`/
//!   `timeseries+ndjson`).
//!
//! All three return the **exact** Go error string `unsupported format: <fmt>`,
//! where `<fmt>` is the caller's *original* (un-normalized) input — Go formats
//! `fmt.Errorf("%w: %s", ErrUnsupportedFormat, format)` with the original
//! `format` argument, not the normalized one. See [`FormatError`].

/// `json` — indented JSON (`SetIndent("","  ")`), the default run/render format.
pub const FORMAT_JSON: &str = "json";
/// `yaml` — YAML via the go-compat YAML emitter.
pub const FORMAT_YAML: &str = "yaml";
/// `plot` — interactive HTML plot output (handled specially, not a machine format).
pub const FORMAT_PLOT: &str = "plot";
/// `bin` alias accepted on input; normalizes to [`FORMAT_BINARY`].
pub const FORMAT_BIN_ALIAS: &str = "bin";
/// `binary` — the CFB1 envelope machine format.
pub const FORMAT_BINARY: &str = "binary";
/// `timeseries` — merged time-series JSON.
pub const FORMAT_TIMESERIES: &str = "timeseries";
/// `ndjson` — newline-delimited JSON (one report per line).
pub const FORMAT_NDJSON: &str = "ndjson";
/// `compact` — compact single-line JSON.
pub const FORMAT_COMPACT: &str = "compact";
/// `timeseries+ndjson` — streaming per-commit NDJSON time series.
pub const FORMAT_TIMESERIES_NDJSON: &str = "timeseries+ndjson";
/// `text` — human-readable table / plain output.
pub const FORMAT_TEXT: &str = "text";

/// `auto` — the sentinel that asks [`resolve_input_format`] to infer the input
/// format from the file extension (Go `InputFormatAuto`).
pub const INPUT_FORMAT_AUTO: &str = "auto";
/// `json` input format (Go `InputFormatJSON`).
pub const INPUT_FORMAT_JSON: &str = "json";
/// `binary` input format (Go `InputFormatBinary`).
pub const INPUT_FORMAT_BINARY: &str = "binary";

/// Error returned when a format string is not recognized.
///
/// `Display` formats as the exact Go string `unsupported format: <fmt>`, where
/// `<fmt>` is the original (un-normalized) caller input — matching Go's
/// `fmt.Errorf("%w: %s", ErrUnsupportedFormat, format)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatError {
    /// The offending format string, exactly as the caller supplied it.
    pub fmt: String,
}

impl core::fmt::Display for FormatError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "unsupported format: {}", self.fmt)
    }
}

impl std::error::Error for FormatError {}

/// Lower-cases (after trimming surrounding whitespace) and maps aliases to
/// canonical format names. Mirrors Go `NormalizeFormat`.
///
/// `bin` → [`FORMAT_BINARY`]; everything else is returned trimmed + lower-cased.
#[must_use]
pub fn normalize_format(format: &str) -> String {
    let normalized = format.trim().to_lowercase();
    if normalized == FORMAT_BIN_ALIAS {
        return FORMAT_BINARY.to_string();
    }
    normalized
}

/// Validates a format for **per-analyzer** machine output. Accepts
/// `json`/`yaml`/`binary`/`text`/`compact`; rejects everything else (including
/// `plot`, which Go routes specially). Mirrors Go `ValidateFormat`.
///
/// On success returns the normalized name. On failure returns a [`FormatError`]
/// carrying the *original* `format` string.
///
/// # Errors
///
/// Returns [`FormatError`] if the normalized format is not one of the accepted
/// per-analyzer formats.
pub fn validate_format(format: &str) -> Result<String, FormatError> {
    let normalized = normalize_format(format);
    match normalized.as_str() {
        FORMAT_JSON | FORMAT_YAML | FORMAT_BINARY | FORMAT_TEXT | FORMAT_COMPACT => Ok(normalized),
        _ => Err(FormatError {
            fmt: format.to_string(),
        }),
    }
}

/// Returns `true` if `normalized` is in the universal-conversion set. Mirrors
/// Go's `universalFormats` map.
fn is_universal_format(normalized: &str) -> bool {
    matches!(
        normalized,
        FORMAT_JSON
            | FORMAT_YAML
            | FORMAT_BINARY
            | FORMAT_TIMESERIES
            | FORMAT_NDJSON
            | FORMAT_COMPACT
            | FORMAT_TIMESERIES_NDJSON
    )
}

/// Validates a format for **universal** cross-format conversion. Mirrors Go
/// `ValidateUniversalFormat`.
///
/// On success returns the normalized name. On failure returns a [`FormatError`]
/// carrying the *original* `format` string.
///
/// # Errors
///
/// Returns [`FormatError`] if the normalized format is not in the universal set.
pub fn validate_universal_format(format: &str) -> Result<String, FormatError> {
    let normalized = normalize_format(format);
    if is_universal_format(&normalized) {
        Ok(normalized)
    } else {
        Err(FormatError {
            fmt: format.to_string(),
        })
    }
}

/// Resolves the per-phase output formats for a mixed static/history run, mirror
/// of Go `ResolveFormats` (`conversion.go`).
///
/// Returns `(static_format, history_format)`. Each is the validated universal
/// format when the corresponding phase has analyzers, or the empty string when
/// it does not. `plot` is handled specially: both phases render `plot` to the
/// same output directory and validation is skipped.
///
/// # Errors
///
/// Returns [`FormatError`] when a non-plot `format` fails
/// [`validate_universal_format`].
pub fn resolve_formats(
    format: &str,
    has_static: bool,
    has_history: bool,
) -> Result<(String, String), FormatError> {
    let normalized = normalize_format(format);

    // Plot format is handled specially - both phases render to the same dir.
    if normalized == FORMAT_PLOT {
        let static_fmt = if has_static {
            FORMAT_PLOT.to_string()
        } else {
            String::new()
        };
        let history_fmt = if has_history {
            FORMAT_PLOT.to_string()
        } else {
            String::new()
        };
        return Ok((static_fmt, history_fmt));
    }

    // For non-plot formats, validate against universal formats.
    let validated = validate_universal_format(&normalized)?;

    let static_fmt = if has_static {
        validated.clone()
    } else {
        String::new()
    };
    let history_fmt = if has_history { validated } else { String::new() };

    Ok((static_fmt, history_fmt))
}

/// Determines the input format from the path and explicit flag, mirror of Go
/// `ResolveInputFormat` (`conversion.go`).
///
/// When `input_format` is not [`INPUT_FORMAT_AUTO`] it is normalized and
/// returned. Otherwise the file extension drives the choice: `.json` → `json`,
/// `.bin`/`.binary` → `binary`, anything else → `json` (Go's `default`).
///
/// This never errors (the Go signature returns an error, but the body has no
/// failure path); the `Result` is kept for signature parity with the Go caller.
///
/// # Errors
///
/// Never returns `Err`; the `Result` mirrors the Go signature.
#[allow(clippy::missing_panics_doc)]
pub fn resolve_input_format(input_path: &str, input_format: &str) -> Result<String, FormatError> {
    if input_format != INPUT_FORMAT_AUTO {
        return Ok(normalize_format(input_format));
    }

    let ext = extension_lower(input_path);
    match ext.as_str() {
        ".json" => Ok(INPUT_FORMAT_JSON.to_string()),
        ".bin" | ".binary" => Ok(INPUT_FORMAT_BINARY.to_string()),
        _ => Ok(INPUT_FORMAT_JSON.to_string()),
    }
}

/// Returns the lower-cased file extension (including the leading dot), mirroring
/// Go `strings.ToLower(filepath.Ext(path))`. Go's `filepath.Ext` returns the
/// suffix from the final dot of the final path element, or "" if none.
fn extension_lower(path: &str) -> String {
    // Restrict to the final path element so dots in directory names are ignored,
    // matching Go's filepath.Ext.
    let base = path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path);
    match base.rfind('.') {
        Some(idx) => base[idx..].to_lowercase(),
        None => String::new(),
    }
}

/// Applies the `--ndjson` modifier to a resolved format: when set and the format
/// is `timeseries`, composes it into `timeseries+ndjson`; otherwise returns the
/// format unchanged. Mirrors the Go composition in `run.go`
/// (`if opts.NDJSON && format == FormatTimeSeries { format = FormatTimeSeriesNDJSON }`).
#[must_use]
pub fn apply_ndjson_modifier(format: &str, ndjson: bool) -> String {
    if ndjson && format == FORMAT_TIMESERIES {
        FORMAT_TIMESERIES_NDJSON.to_string()
    } else {
        format.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_lowercases_and_trims() {
        assert_eq!(normalize_format("JSON"), "json");
        assert_eq!(normalize_format("  YaMl  "), "yaml");
    }

    #[test]
    fn normalize_maps_bin_alias_to_binary() {
        assert_eq!(normalize_format("bin"), "binary");
        assert_eq!(normalize_format("BIN"), "binary");
        // already-canonical "binary" is untouched.
        assert_eq!(normalize_format("binary"), "binary");
    }

    #[test]
    fn validate_format_accepts_per_analyzer_set() {
        for f in ["json", "yaml", "binary", "text", "compact", "bin"] {
            assert!(validate_format(f).is_ok(), "{f} should validate");
        }
        // bin normalizes to binary on success.
        assert_eq!(validate_format("bin").unwrap(), "binary");
    }

    #[test]
    fn validate_format_rejects_plot_and_unknown() {
        assert!(validate_format("plot").is_err());
        assert!(validate_format("timeseries").is_err());
        assert!(validate_format("ndjson").is_err());
    }

    #[test]
    fn validate_universal_accepts_conversion_set() {
        for f in [
            "json",
            "yaml",
            "binary",
            "timeseries",
            "ndjson",
            "compact",
            "timeseries+ndjson",
            "bin",
        ] {
            assert!(validate_universal_format(f).is_ok(), "{f} should validate");
        }
    }

    #[test]
    fn validate_universal_rejects_plot_and_text() {
        assert!(validate_universal_format("plot").is_err());
        assert!(validate_universal_format("text").is_err());
    }

    #[test]
    fn error_string_uses_original_format_and_go_wording() {
        // Go: fmt.Errorf("%w: %s", ErrUnsupportedFormat, format) with original arg.
        let err = validate_universal_format("PLOT").unwrap_err();
        assert_eq!(err.to_string(), "unsupported format: PLOT");
        let err2 = validate_format("nope").unwrap_err();
        assert_eq!(err2.to_string(), "unsupported format: nope");
    }

    #[test]
    fn resolve_formats_plot_routes_both_phases() {
        let (s, h) = resolve_formats("plot", true, true).unwrap();
        assert_eq!(s, "plot");
        assert_eq!(h, "plot");
        let (s, h) = resolve_formats("plot", false, true).unwrap();
        assert_eq!(s, "");
        assert_eq!(h, "plot");
    }

    #[test]
    fn resolve_formats_non_plot_fills_only_present_phases() {
        let (s, h) = resolve_formats("json", true, false).unwrap();
        assert_eq!(s, "json");
        assert_eq!(h, "");
        let (s, h) = resolve_formats("bin", true, true).unwrap();
        assert_eq!(s, "binary");
        assert_eq!(h, "binary");
    }

    #[test]
    fn resolve_formats_propagates_unsupported_error() {
        let err = resolve_formats("text", true, true).unwrap_err();
        assert_eq!(err.to_string(), "unsupported format: text");
    }

    #[test]
    fn resolve_input_format_explicit_is_normalized() {
        assert_eq!(resolve_input_format("x.dat", "bin").unwrap(), "binary");
        assert_eq!(resolve_input_format("x.dat", "JSON").unwrap(), "json");
    }

    #[test]
    fn resolve_input_format_auto_uses_extension() {
        assert_eq!(resolve_input_format("report.json", "auto").unwrap(), "json");
        assert_eq!(resolve_input_format("report.BIN", "auto").unwrap(), "binary");
        assert_eq!(resolve_input_format("report.binary", "auto").unwrap(), "binary");
        // Unknown / no extension -> json (Go default).
        assert_eq!(resolve_input_format("report.txt", "auto").unwrap(), "json");
        assert_eq!(resolve_input_format("report", "auto").unwrap(), "json");
        // Dot in directory, none in basename -> no extension -> json.
        assert_eq!(resolve_input_format("a.b/report", "auto").unwrap(), "json");
    }

    #[test]
    fn ndjson_modifier_composes_only_timeseries() {
        assert_eq!(apply_ndjson_modifier("timeseries", true), "timeseries+ndjson");
        assert_eq!(apply_ndjson_modifier("timeseries", false), "timeseries");
        assert_eq!(apply_ndjson_modifier("json", true), "json");
    }
}
