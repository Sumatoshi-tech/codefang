//! Composition analyzer.
//!
//! Ported from Go `internal/analyzers/composition/analyzer.go`. Implements the
//! `static/composition` raw-file analyzer: it classifies each file via
//! [`Classifier`] and renders aggregated reports in every machine format.
//!
//! # Byte-identity (DESIGN rules 1 & 2)
//!
//! All report serialization routes through `cf-gojson`, never `serde_json`:
//!
//! * `json` / `text` / `plot` — `Encoder::indented("  ")` (indent `"  "`, HTML
//!   escaping ON, trailing newline; Go `json.NewEncoder` + `SetIndent("","  ")`).
//! * `bin` — the CFB1 envelope (`b"CFB1"` + LE u32 length + compact JSON
//!   payload, no trailing newline), per DESIGN §2.5.
//! * `yaml` — must route through `cf-goyaml`; that crate is still a scaffold, so
//!   the YAML path is stubbed and tracked in the crate `todos` (DESIGN rule 5).

use std::collections::HashMap;
use std::io::{self, Write};

use cf_gojson::{Encoder, GoMap, GoValue, MapOrigin};

use crate::aggregator::{Aggregator, KEY_CATEGORY};
use crate::classifier::Classifier;
use crate::report_section::{CompositionReport, ReportSection};

/// Analyzer identity constants (Go `analyzer*`).
pub const ANALYZER_NAME: &str = "composition";
/// CLI flag name.
pub const ANALYZER_FLAG: &str = "composition";
/// Fully-qualified analyzer ID.
pub const ANALYZER_ID: &str = "static/composition";
/// Human description.
pub const ANALYZER_DESCRIPTION: &str =
    "Classifies files by type (source, vendor, generated, docs, config, binary, image) using enry.";

/// CFB1 magic prefix for the binary envelope (DESIGN §2.5).
const BINARY_MAGIC: &[u8; 4] = b"CFB1";

/// Implements the raw-file composition analyzer.
///
/// Mirrors Go `Analyzer`, which holds a `*filehistory.Classifier`.
#[derive(Debug, Default, Clone)]
pub struct Analyzer {
    classifier: Classifier,
}

impl Analyzer {
    /// Creates a new composition analyzer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            classifier: Classifier::new(),
        }
    }

    /// Returns the analyzer name.
    #[must_use]
    pub fn name(&self) -> &'static str {
        ANALYZER_NAME
    }

    /// Returns the CLI flag name.
    #[must_use]
    pub fn flag(&self) -> &'static str {
        ANALYZER_FLAG
    }

    /// Returns the analyzer ID (`static/composition`).
    #[must_use]
    pub fn id(&self) -> &'static str {
        ANALYZER_ID
    }

    /// Returns the analyzer description.
    #[must_use]
    pub fn description(&self) -> &'static str {
        ANALYZER_DESCRIPTION
    }

    /// Returns the metric thresholds. Composition is informational, so there are
    /// none (Go returns `nil`).
    #[must_use]
    pub fn thresholds(&self) -> Option<()> {
        None
    }

    /// Returns the available configuration options. Composition has none.
    #[must_use]
    pub fn list_configuration_options(&self) -> Vec<()> {
        Vec::new()
    }

    /// Applies configuration facts. Composition takes none and never errors.
    ///
    /// Mirrors Go `Configure(_ map[string]any) error { return nil }`.
    #[allow(clippy::unused_self)]
    pub fn configure(&self, _facts: &HashMap<String, GoValue>) -> Result<(), CompositionError> {
        Ok(())
    }

    /// Creates a fresh aggregator.
    #[must_use]
    pub fn create_aggregator(&self) -> Aggregator {
        Aggregator::new()
    }

    /// Classifies a single file by path and content.
    ///
    /// Mirrors Go `AnalyzeFileContent`: returns a single-key report mapping
    /// `category` to the classification string. This never errors in Go.
    #[must_use]
    pub fn analyze_file_content(&self, path: &str, content: &[u8]) -> GoMap {
        let category = self.classifier.classify(path, content);
        // Go `analyze.Report` is a `map[string]any`, hence map-origin.
        let mut report = GoMap::new(MapOrigin::Map);
        report.push(KEY_CATEGORY, GoValue::Str(category.as_str().to_string()));
        report
    }

    /// Creates a report section from a decoded composition report.
    #[must_use]
    pub fn create_report_section(&self, report: CompositionReport) -> ReportSection {
        ReportSection::new(report)
    }

    /// Writes human-readable text output (same as JSON in Go).
    pub fn format_report(&self, report: &GoMap, writer: &mut dyn Write) -> io::Result<()> {
        encode_json(report, writer)
    }

    /// Writes indented JSON output.
    pub fn format_report_json(&self, report: &GoMap, writer: &mut dyn Write) -> io::Result<()> {
        encode_json(report, writer)
    }

    /// Writes plot output (same as JSON in Go).
    pub fn format_report_plot(&self, report: &GoMap, writer: &mut dyn Write) -> io::Result<()> {
        self.format_report_json(report, writer)
    }

    /// Writes the CFB1 binary envelope.
    ///
    /// Layout (DESIGN §2.5): `b"CFB1"` + payload length as little-endian `u32`
    /// + compact JSON payload (HTML escaping ON, **no** trailing newline).
    pub fn format_report_binary(&self, report: &GoMap, writer: &mut dyn Write) -> io::Result<()> {
        // `Encoder::compact()` == Go `json.Marshal`: compact, HTML-escape ON,
        // no trailing newline — exactly the CFB1 payload encoding.
        let payload = Encoder::compact().encode_to_vec(&GoValue::Map(report.clone()));

        let len = u32::try_from(payload.len()).unwrap_or(u32::MAX);
        writer.write_all(BINARY_MAGIC)?;
        writer.write_all(&len.to_le_bytes())?;
        writer.write_all(&payload)?;
        Ok(())
    }

    /// Writes YAML output.
    ///
    /// Mirrors Go `yaml.NewEncoder(writer).Encode(report)` (`gopkg.in/yaml.v3`,
    /// default 4-space indent) by delegating to `cf-goyaml::marshal`. The report
    /// is a map-origin [`GoMap`], so its keys (`breakdown`, `percentages`,
    /// `total_files`) and the nested category maps are byte-sorted, matching how
    /// yaml.v3 orders Go `map[string]…` keys.
    pub fn format_report_yaml(&self, report: &GoMap, writer: &mut dyn Write) -> io::Result<()> {
        let bytes = cf_goyaml::marshal(&GoValue::Map(report.clone()));
        writer.write_all(&bytes)
    }
}

/// Encodes a report as indented JSON, matching Go's
/// `json.NewEncoder(w); enc.SetIndent("", "  "); enc.Encode(report)`.
///
/// `cf-gojson`'s default `Encoder` reproduces HTML escaping ON, byte-sorted map
/// keys, Go float rendering, and the single trailing newline of
/// `Encoder.Encode`.
fn encode_json(report: &GoMap, writer: &mut dyn Write) -> io::Result<()> {
    // Go's `json.NewEncoder(w); SetIndent("","  "); Encode(report)` emits a
    // two-space indent, HTML-escape ON, AND a single trailing newline (every
    // `Encoder.Encode` appends one `\n`). `Encoder::indented` defaults
    // trailing-newline OFF, so it must be turned on explicitly here.
    let bytes = Encoder::indented("  ")
        .with_trailing_newline(true)
        .encode_to_vec(&GoValue::Map(report.clone()));
    writer.write_all(&bytes)
}

/// Error type for the (currently infallible) configuration path.
#[derive(Debug)]
pub struct CompositionError(pub String);

impl std::fmt::Display for CompositionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for CompositionError {}
