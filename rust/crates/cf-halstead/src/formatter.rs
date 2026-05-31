//! Report formatting and assessment helpers (`formatter.go`).
//!
//! The JSON/YAML/binary report bodies serialize the [`ComputedMetrics`] view
//! (NOT the raw analyzer report) via the Go-byte-compatible encoders, matching
//! `FormatReportJSON` (json.MarshalIndent two-space), `FormatReportYAML`
//! (yaml.Marshal) and `FormatReportBinary` (CFB1 envelope around compact JSON).

use std::io::{self, Write};

use crate::gojson;
use crate::metrics::ComputedMetrics;

// --- Assessment thresholds (formatter.go) ---

const VOLUME_THRESHOLD_HIGH: f64 = 5000.0; // volumeThresholdHigh
const MAGIC_1000: f64 = 1000.0;
const DIFFICULTY_THRESHOLD_HIGH: f64 = 5.0;
const MAGIC_15: f64 = 15.0;
const EFFORT_THRESHOLD_HIGH: f64 = 1000.0;
const MAGIC_10000: f64 = 10000.0;
const BUGS_THRESHOLD_MIN: f64 = 0.1;
const BUGS_THRESHOLD_MEDIUM: f64 = 0.5;

/// Formats Halstead analysis reports (`ReportFormatter`).
#[derive(Debug, Clone, Copy, Default)]
pub struct ReportFormatter;

impl ReportFormatter {
    /// Creates a new formatter.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Overall message from the file-level measures (`GetHalsteadMessage`).
    ///
    /// Note: this is distinct from the aggregator's volume-only message
    /// (`buildHalsteadMessage`); it is the one stored in the per-file report.
    #[must_use]
    pub fn halstead_message(&self, volume: f64, difficulty: f64, effort: f64) -> &'static str {
        if volume <= 100.0 && difficulty <= 5.0 && effort <= 1000.0 {
            return "Excellent complexity - code is simple and maintainable";
        }
        if volume <= 1000.0 && difficulty <= 15.0 && effort <= 10000.0 {
            return "Good complexity - code is reasonably complex";
        }
        if volume <= 5000.0 && difficulty <= 30.0 && effort <= 50000.0 {
            return "Fair complexity - consider simplifying some functions";
        }
        "High complexity - code should be refactored for better maintainability"
    }

    /// Emoji assessment for volume (`GetVolumeAssessment`).
    #[must_use]
    pub fn volume_assessment(&self, volume: f64) -> String {
        if volume <= VOLUME_THRESHOLD_HIGH {
            "🟢 Low".to_string()
        } else if volume <= MAGIC_1000 {
            "🟡 Medium".to_string()
        } else {
            "🔴 High".to_string()
        }
    }

    /// Emoji assessment for difficulty (`GetDifficultyAssessment`).
    #[must_use]
    pub fn difficulty_assessment(&self, difficulty: f64) -> String {
        if difficulty <= DIFFICULTY_THRESHOLD_HIGH {
            "🟢 Simple".to_string()
        } else if difficulty <= MAGIC_15 {
            "🟡 Moderate".to_string()
        } else {
            "🔴 Complex".to_string()
        }
    }

    /// Emoji assessment for effort (`GetEffortAssessment`).
    #[must_use]
    pub fn effort_assessment(&self, effort: f64) -> String {
        if effort <= EFFORT_THRESHOLD_HIGH {
            "🟢 Low".to_string()
        } else if effort <= MAGIC_10000 {
            "🟡 Medium".to_string()
        } else {
            "🔴 High".to_string()
        }
    }

    /// Emoji assessment for delivered bugs (`GetBugAssessment`).
    #[must_use]
    pub fn bug_assessment(&self, bugs: f64) -> String {
        if bugs <= BUGS_THRESHOLD_MIN {
            "🟢 Low Risk".to_string()
        } else if bugs <= BUGS_THRESHOLD_MEDIUM {
            "🟡 Medium Risk".to_string()
        } else {
            "🔴 High Risk".to_string()
        }
    }

    /// Serializes the computed metrics as indented JSON (`FormatReportJSON`):
    /// `json.MarshalIndent(metrics, "", "  ")` — two-space indent, HTML escaping
    /// ON, no trailing newline (MarshalIndent does not append one).
    pub fn write_json<W: Write>(&self, metrics: &ComputedMetrics, w: &mut W) -> io::Result<()> {
        let value = metrics.to_go_value();
        let bytes = gojson::Encoder::new()
            .with_indent(Some("  "))
            .with_escape_html(true)
            .with_trailing_newline(false)
            .encode(&value)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("formatreportjson: {e}")))?;
        w.write_all(&bytes)
    }

    /// Serializes the computed metrics as a CFB1 binary envelope
    /// (`FormatReportBinary`): `b"CFB1"` + u32-LE length + compact Go JSON.
    pub fn write_binary<W: Write>(&self, metrics: &ComputedMetrics, w: &mut W) -> io::Result<()> {
        let value = metrics.to_go_value();
        gojson::encode_binary_envelope(&value, w)
    }
}

// NOTE: YAML output (`FormatReportYAML` -> yaml.Marshal) is intentionally NOT
// implemented here. It must route through the shared `cf-goyaml` emitter
// (DESIGN §2.4), which is a bare scaffold at port time. Wiring it in is a
// blocked item tracked in the crate todos; emitting YAML via any other library
// would violate the byte-identity rule.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assessments() {
        let f = ReportFormatter::new();
        assert_eq!(f.volume_assessment(100.0), "🟢 Low");
        assert_eq!(f.volume_assessment(6000.0), "🟡 Medium");
        assert_eq!(f.volume_assessment(20000.0), "🔴 High");

        assert_eq!(f.difficulty_assessment(3.0), "🟢 Simple");
        assert_eq!(f.difficulty_assessment(10.0), "🟡 Moderate");
        assert_eq!(f.difficulty_assessment(40.0), "🔴 Complex");

        assert_eq!(f.effort_assessment(500.0), "🟢 Low");
        assert_eq!(f.effort_assessment(5000.0), "🟡 Medium");
        assert_eq!(f.effort_assessment(60000.0), "🔴 High");

        assert_eq!(f.bug_assessment(0.05), "🟢 Low Risk");
        assert_eq!(f.bug_assessment(0.3), "🟡 Medium Risk");
        assert_eq!(f.bug_assessment(1.2), "🔴 High Risk");
    }

    #[test]
    fn message_tiers() {
        let f = ReportFormatter::new();
        assert_eq!(
            f.halstead_message(50.0, 3.0, 500.0),
            "Excellent complexity - code is simple and maintainable"
        );
        assert_eq!(
            f.halstead_message(500.0, 10.0, 5000.0),
            "Good complexity - code is reasonably complex"
        );
        assert_eq!(
            f.halstead_message(3000.0, 25.0, 40000.0),
            "Fair complexity - consider simplifying some functions"
        );
        assert_eq!(
            f.halstead_message(6000.0, 35.0, 60000.0),
            "High complexity - code should be refactored for better maintainability"
        );
    }
}
