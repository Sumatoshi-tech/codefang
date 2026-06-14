//! Composition analyzer.
//!
//! Implements the `static/composition` raw-file analyzer: it classifies each
//! file via [`Classifier`] and renders aggregated reports in every machine
//! format.
//!
//! # Compatibility
//!
//! All report serialization routes through `cf-gojson` / `cf-goyaml`, never
//! serde; the bytes are pinned against the reference binary by
//! `tests/compat`:
//!
//! * `json` / `text` / `plot` — `Encoder::indented("  ")` (two-space indent,
//!   HTML escaping ON, trailing newline).
//! * `bin` — the CFB1 envelope (`b"CFB1"` + LE u32 length + compact JSON
//!   payload, no trailing newline).
//! * `yaml` — `cf-goyaml` (4-space-indent YAML with byte-sorted map keys).

use std::collections::HashMap;
use std::io::{self, Write};

use cf_gojson::{Encoder, GoMap, GoValue, MapOrigin};

use crate::aggregator::{Aggregator, KEY_CATEGORY};
use crate::classifier::Classifier;
use crate::report_section::{CompositionReport, ReportSection};

/// Analyzer name.
pub const ANALYZER_NAME: &str = "composition";
/// CLI flag name.
pub const ANALYZER_FLAG: &str = "composition";
/// Fully-qualified analyzer ID.
pub const ANALYZER_ID: &str = "static/composition";
/// Human description.
pub const ANALYZER_DESCRIPTION: &str =
    "Classifies files by type (source, vendor, generated, docs, config, binary, image) using enry.";

/// CFB1 magic prefix for the binary envelope.
const BINARY_MAGIC: &[u8; 4] = b"CFB1";

/// Implements the raw-file composition analyzer.
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

    /// Returns the metric thresholds. Composition is informational, so there
    /// are none.
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
    /// # Errors
    /// Never fails; the `Result` keeps the analyzer-interface signature.
    #[allow(clippy::unused_self)]
    pub fn configure(&self, _facts: &HashMap<String, GoValue>) -> Result<(), CompositionError> {
        Ok(())
    }

    /// Creates a fresh aggregator.
    #[must_use]
    pub fn create_aggregator(&self) -> Aggregator {
        Aggregator::new()
    }

    /// Classifies a single file by path and content, returning a single-key
    /// report mapping `category` to the classification string.
    #[must_use]
    pub fn analyze_file_content(&self, path: &str, content: &[u8]) -> GoMap {
        let category = self.classifier.classify(path, content);
        // Per-file reports are dynamic string-keyed maps, hence map-origin.
        let mut report = GoMap::new(MapOrigin::Map);
        report.push(KEY_CATEGORY, GoValue::Str(category.as_str().to_string()));
        report
    }

    /// Creates a report section from a decoded composition report.
    #[must_use]
    pub fn create_report_section(&self, report: CompositionReport) -> ReportSection {
        ReportSection::new(report)
    }

    /// Writes human-readable text output (same bytes as the JSON format).
    ///
    /// # Errors
    /// Propagates writer I/O errors.
    pub fn format_report(&self, report: &GoMap, writer: &mut dyn Write) -> io::Result<()> {
        encode_json(report, writer)
    }

    /// Writes indented JSON output.
    ///
    /// # Errors
    /// Propagates writer I/O errors.
    pub fn format_report_json(&self, report: &GoMap, writer: &mut dyn Write) -> io::Result<()> {
        encode_json(report, writer)
    }

    /// Writes plot output (same bytes as the JSON format).
    ///
    /// # Errors
    /// Propagates writer I/O errors.
    pub fn format_report_plot(&self, report: &GoMap, writer: &mut dyn Write) -> io::Result<()> {
        self.format_report_json(report, writer)
    }

    /// Writes the CFB1 binary envelope: `b"CFB1"` + payload length as
    /// little-endian `u32` + compact JSON payload (HTML escaping ON, **no**
    /// trailing newline).
    ///
    /// # Errors
    /// Propagates writer I/O errors.
    pub fn format_report_binary(&self, report: &GoMap, writer: &mut dyn Write) -> io::Result<()> {
        // `Encoder::compact()`: compact, HTML-escape ON, no trailing newline —
        // exactly the CFB1 payload encoding.
        let payload = Encoder::compact().encode_to_vec(&GoValue::Map(report.clone()));

        let len = u32::try_from(payload.len()).unwrap_or(u32::MAX);
        writer.write_all(BINARY_MAGIC)?;
        writer.write_all(&len.to_le_bytes())?;
        writer.write_all(&payload)?;
        Ok(())
    }

    /// Writes YAML output by delegating to `cf-goyaml::marshal` (default
    /// 4-space indent). The report is a map-origin [`GoMap`], so its keys
    /// (`breakdown`, `percentages`, `total_files`) and the nested category maps
    /// are byte-sorted, per the report contract for dynamic maps.
    ///
    /// # Errors
    /// Propagates writer I/O errors.
    pub fn format_report_yaml(&self, report: &GoMap, writer: &mut dyn Write) -> io::Result<()> {
        let bytes = cf_goyaml::marshal(&GoValue::Map(report.clone()));
        writer.write_all(&bytes)
    }
}

/// Encodes a report as indented JSON per the report contract: two-space
/// indent, HTML escaping ON, byte-sorted map keys, shortest-round-trip float
/// rendering, and a single trailing newline.
fn encode_json(report: &GoMap, writer: &mut dyn Write) -> io::Result<()> {
    // The streaming-encoder report contract appends a single trailing newline.
    // `Encoder::indented` defaults trailing-newline OFF, so it must be turned
    // on explicitly here.
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
