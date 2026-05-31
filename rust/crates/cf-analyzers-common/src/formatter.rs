//! Human-readable report formatting.
//!
//! Port of `formatter.go`. [`Formatter`] renders an analyzer [`Report`] as a
//! text block: an optional header, a summary of message + numeric metrics,
//! optional unicode progress bars, optional tables, and optional sorted details.
//!
//! # Byte-identity
//!
//! Per DESIGN §2.7 the `go-pretty` `StyleLight` table output is explicitly
//! **NON-BINDING / cosmetic** — byte-identity is *not* required. The table
//! writer here ([`render_style_light_table`]) reproduces the structure and the
//! byte-width padding behaviour of the Go `StyleLight` style (column dividers,
//! header underline, footer) on a best-effort basis. The machine JSON path lives
//! in [`crate::reporter`] and routes through `cf-gojson`, not through this module.

use crate::report::{Item, Report, Value};
use std::collections::BTreeMap;

/// Message returned for a nil report, mirroring the Go `msgNoReportData`.
pub(crate) const MSG_NO_REPORT_DATA: &str = "No report data available";

const PERCENTAGE_VALUE: f64 = 100.0;
const SCORE_THRESHOLD_HIGH: f64 = 0.8;
const SCORE_THRESHOLD_MEDIUM: f64 = 0.6;

/// Configuration for a [`Formatter`].
///
/// Field names and defaults mirror the Go `FormatConfig` struct.
#[derive(Debug, Clone, Default)]
pub struct FormatConfig {
    /// Key to sort collection rows by (empty = no sorting).
    pub sort_by: String,
    /// Sort direction: `"desc"` for descending, anything else ascending.
    pub sort_order: String,
    /// Maximum number of rows to render per collection (0 = unlimited).
    pub max_items: usize,
    /// Render unicode progress bars for 0–1 numeric scores.
    pub show_progress_bars: bool,
    /// Render collections as tables.
    pub show_tables: bool,
    /// Render a sorted details section of all non-collection fields.
    pub show_details: bool,
    /// Suppress the `=== NAME ===` header.
    pub skip_header: bool,
}

/// Formats analyzer reports for human-readable (text) display.
///
/// Mirrors the Go `Formatter`.
#[derive(Debug, Clone, Default)]
pub struct Formatter {
    config: FormatConfig,
}

impl Formatter {
    /// Creates a formatter with the given configuration. Mirrors `NewFormatter`.
    pub fn new(config: FormatConfig) -> Self {
        Formatter { config }
    }

    /// Formats a report for display. Mirrors `FormatReport`.
    ///
    /// A `None` report yields [`MSG_NO_REPORT_DATA`]. Sections are joined by a
    /// blank line (`"\n\n"`), exactly like the Go implementation.
    pub fn format_report(&self, report: Option<&Report>) -> String {
        let report = match report {
            Some(r) => r,
            None => return MSG_NO_REPORT_DATA.to_string(),
        };

        let mut parts: Vec<String> = Vec::new();

        // Header (unless skipped).
        if !self.config.skip_header {
            if let Some(Value::String(name)) = report.get("analyzer_name") {
                parts.push(format!("=== {} ===", name.to_uppercase()));
            }
        }

        let summary = self.format_summary(report);
        if !summary.is_empty() {
            parts.push(summary);
        }

        if self.config.show_progress_bars {
            let bars = self.format_progress_bars(report);
            if !bars.is_empty() {
                parts.push(bars);
            }
        }

        if self.config.show_tables {
            let tables = self.format_tables(report);
            if !tables.is_empty() {
                parts.push(tables);
            }
        }

        if self.config.show_details {
            let details = self.format_details(report);
            if !details.is_empty() {
                parts.push(details);
            }
        }

        parts.join("\n\n")
    }

    /// Formats the message + sorted numeric metrics summary.
    fn format_summary(&self, report: &Report) -> String {
        let mut summary: Vec<String> = Vec::new();

        if let Some(Value::String(message)) = report.get("message") {
            if !message.is_empty() {
                summary.push(message.clone());
            }
        }

        let metrics = extract_all_numeric_metrics(report);
        if !metrics.is_empty() {
            // BTreeMap iterates sorted by key, then we format and sort the
            // resulting "key: value" lines to match Go's sort.Strings.
            let mut metric_lines: Vec<String> = metrics
                .iter()
                .map(|(key, value)| format!("{key}: {value:.2}"))
                .collect();
            metric_lines.sort();
            summary.push(metric_lines.join(" | "));
        }

        summary.join("\n")
    }

    /// Formats progress bars for numeric values in the 0–1 range.
    fn format_progress_bars(&self, report: &Report) -> String {
        // Count metrics that must not be shown as progress bars.
        const COUNT_METRICS: &[&str] = &[
            "total_comments",
            "good_comments",
            "bad_comments",
            "total_functions",
            "documented_functions",
            "total_comment_details",
        ];

        let mut bars: Vec<String> = Vec::new();
        for (key, value) in report {
            if COUNT_METRICS.contains(&key.as_str()) {
                continue;
            }
            if let Some(score) = value.to_float64() {
                if (0.0..=1.0).contains(&score) {
                    let bar = self.create_progress_bar(key, score);
                    if !bar.is_empty() {
                        bars.push(bar);
                    }
                }
            }
        }

        if bars.is_empty() {
            return String::new();
        }

        format!("Progress:\n{}", bars.join("\n"))
    }

    /// Formats every collection in the report as a table.
    fn format_tables(&self, report: &Report) -> String {
        let mut tables: Vec<String> = Vec::new();
        for (key, value) in report {
            if let Value::Collection(collection) = value {
                if !collection.is_empty() {
                    let table_str = self.format_collection_table(key, collection);
                    if !table_str.is_empty() {
                        tables.push(table_str);
                    }
                }
            }
        }
        tables.join("\n\n")
    }

    /// Formats all non-collection fields as a sorted details section.
    fn format_details(&self, report: &Report) -> String {
        let mut details: Vec<String> = Vec::new();
        for (key, value) in report {
            if !matches!(value, Value::Collection(_)) {
                details.push(format!("{key}: {value}"));
            }
        }

        if details.is_empty() {
            return String::new();
        }

        details.sort();
        format!("Details:\n{}", details.join("\n"))
    }

    /// Formats a single collection as a `StyleLight` table.
    fn format_collection_table(&self, collection_key: &str, collection: &[Item]) -> String {
        if collection.is_empty() {
            return String::new();
        }

        // Limit rows if configured.
        let mut rows: Vec<&Item> = collection.iter().collect();
        if self.config.max_items > 0 && rows.len() > self.config.max_items {
            rows.truncate(self.config.max_items);
        }

        // Sort if configured.
        if !self.config.sort_by.is_empty() {
            self.sort_collection(&mut rows);
        }

        let keys = collection_keys(&rows);
        if keys.is_empty() {
            return String::new();
        }

        let header: Vec<String> = keys.clone();

        let body: Vec<Vec<String>> = rows
            .iter()
            .map(|item| {
                keys.iter()
                    .map(|key| match item.get(key) {
                        None | Some(Value::Null) => String::new(),
                        Some(v) => v.to_string(),
                    })
                    .collect()
            })
            .collect();

        let footer = format!("Total: {} items", rows.len());

        let table = render_style_light_table(&header, &body, &footer);
        format!("{collection_key}:\n{table}")
    }

    /// Creates a unicode progress bar for a score in 0–1.
    fn create_progress_bar(&self, label: &str, score: f64) -> String {
        const BAR_LENGTH: i64 = 20;

        let filled = (score * BAR_LENGTH as f64) as i64;
        let filled = filled.clamp(0, BAR_LENGTH);
        let empty = BAR_LENGTH - filled;

        let bar: String = "\u{2588}".repeat(filled as usize) + &"\u{2591}".repeat(empty as usize);
        let percentage = score * PERCENTAGE_VALUE;

        let status = if score >= SCORE_THRESHOLD_HIGH {
            "\u{1F7E2} Good"
        } else if score >= SCORE_THRESHOLD_MEDIUM {
            "\u{1F7E1} Fair"
        } else {
            "\u{1F534} Poor"
        };

        format!("{label}: [{bar}] {percentage:.1}% {status}")
    }

    /// Sorts collection rows by the configured key using [`to_comparable`].
    fn sort_collection(&self, rows: &mut [&Item]) {
        let sort_by = &self.config.sort_by;
        let desc = self.config.sort_order == "desc";
        rows.sort_by(|a, b| {
            let ca = to_comparable(a.get(sort_by));
            let cb = to_comparable(b.get(sort_by));
            let ord = ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal);
            if desc {
                ord.reverse()
            } else {
                ord
            }
        });
    }
}

/// Extracts every numeric value from a report as `f64`.
///
/// Shared by the formatter summary and [`crate::reporter::Reporter`].
pub(crate) fn extract_all_numeric_metrics(report: &Report) -> BTreeMap<String, f64> {
    let mut metrics = BTreeMap::new();
    for (key, value) in report {
        if let Some(score) = value.to_float64() {
            metrics.insert(key.clone(), score);
        }
    }
    metrics
}

/// Returns every distinct key across a collection's items, sorted (mirrors the
/// Go `getCollectionKeys` + `mapx.SortedKeys`).
fn collection_keys(collection: &[&Item]) -> Vec<String> {
    let mut set = std::collections::BTreeSet::new();
    for item in collection {
        for key in item.keys() {
            set.insert(key.clone());
        }
    }
    set.into_iter().collect()
}

/// Converts a value to a comparable `f64` for sorting, mirroring the Go
/// `toComparable`: numbers map to their value, strings to their byte length, and
/// everything else to `0`.
fn to_comparable(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::Float(n)) => *n,
        Some(Value::Float32(n)) => f64::from(*n),
        Some(Value::Int(n)) => *n as f64,
        Some(Value::Int32(n)) => f64::from(*n),
        Some(Value::String(s)) => s.len() as f64,
        _ => 0.0,
    }
}

/// Renders a table in the `go-pretty` `StyleLight` style with row/column
/// separators, border, and header separators all disabled (matching the
/// `formatter.go` configuration).
///
/// Columns are padded to their widest cell using **byte** length (matching Go's
/// `text.RuneWidth`-free path noted in DESIGN §2.7), cells separated by a single
/// space, with a light box-drawing rule (`─`) under the header and above the
/// footer. This output is NON-BINDING (cosmetic).
pub fn render_style_light_table(header: &[String], body: &[Vec<String>], footer: &str) -> String {
    let cols = header.len();
    let mut widths: Vec<usize> = header.iter().map(|h| h.len()).collect();
    for row in body {
        for (i, cell) in row.iter().enumerate() {
            if i < cols && cell.len() > widths[i] {
                widths[i] = cell.len();
            }
        }
    }

    let pad = |cell: &str, width: usize| -> String {
        let mut s = String::from(cell);
        // Byte-width padding, per DESIGN §2.7.
        if cell.len() < width {
            s.push_str(&" ".repeat(width - cell.len()));
        }
        s
    };

    let render_row = |cells: &[String]| -> String {
        cells
            .iter()
            .enumerate()
            .map(|(i, c)| pad(c, *widths.get(i).unwrap_or(&0)))
            .collect::<Vec<_>>()
            .join(" ")
            .trim_end()
            .to_string()
    };

    let total_width: usize = widths.iter().sum::<usize>() + cols.saturating_sub(1);
    let rule = "\u{2500}".repeat(total_width);

    let mut out: Vec<String> = Vec::with_capacity(body.len() + 3);
    out.push(render_row(header));
    out.push(rule.clone());
    for row in body {
        out.push(render_row(row));
    }
    if !footer.is_empty() {
        out.push(rule);
        out.push(footer.to_string());
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(pairs: &[(&str, Value)]) -> Report {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    // Ported from formatter_test.go: TestFormatter_FormatReport_Nil
    #[test]
    fn format_report_nil() {
        let f = Formatter::new(FormatConfig::default());
        assert_eq!(f.format_report(None), "No report data available");
    }

    // Ported from formatter_test.go: TestFormatter_FormatReport_Basic
    #[test]
    fn format_report_basic() {
        let f = Formatter::new(FormatConfig::default());
        let r = report(&[
            ("analyzer_name", Value::String("test_analyzer".into())),
            ("message", Value::String("Analysis complete".into())),
        ]);
        let out = f.format_report(Some(&r));
        assert!(out.contains("TEST_ANALYZER"));
        assert!(out.contains("Analysis complete"));
    }

    // Ported from formatter_test.go: TestFormatter_FormatReport_WithMetrics
    #[test]
    fn format_report_with_metrics() {
        let f = Formatter::new(FormatConfig::default());
        let r = report(&[
            ("analyzer_name", Value::String("test".into())),
            ("score", Value::Float(0.85)),
            ("quality", Value::Float(0.92)),
        ]);
        let out = f.format_report(Some(&r));
        assert!(out.contains("score"));
        assert!(out.contains("quality"));
    }

    // Ported from formatter_test.go: TestFormatter_FormatReport_SkipHeader
    #[test]
    fn format_report_skip_header() {
        let f = Formatter::new(FormatConfig {
            skip_header: true,
            ..Default::default()
        });
        let r = report(&[
            ("analyzer_name", Value::String("test_analyzer".into())),
            ("message", Value::String("Test".into())),
        ]);
        let out = f.format_report(Some(&r));
        assert!(!out.contains("==="));
    }

    // Ported from formatter_test.go: TestFormatter_FormatReport_WithProgressBars
    #[test]
    fn format_report_with_progress_bars() {
        let f = Formatter::new(FormatConfig {
            show_progress_bars: true,
            ..Default::default()
        });
        let r = report(&[
            ("analyzer_name", Value::String("test".into())),
            ("score", Value::Float(0.75)),
        ]);
        let out = f.format_report(Some(&r));
        assert!(out.contains("Progress:"));
    }

    // Ported from formatter_test.go: TestFormatter_FormatReport_WithTables
    #[test]
    fn format_report_with_tables() {
        let f = Formatter::new(FormatConfig {
            show_tables: true,
            ..Default::default()
        });
        let items = vec![
            report(&[("name", Value::String("item1".into())), ("value", Value::Int(10))]),
            report(&[("name", Value::String("item2".into())), ("value", Value::Int(20))]),
        ];
        let r = report(&[
            ("analyzer_name", Value::String("test".into())),
            ("items", Value::Collection(items)),
        ]);
        let out = f.format_report(Some(&r));
        assert!(out.contains("items"));
        assert!(out.contains("Total: 2 items"));
    }

    // Ported from formatter_test.go: TestFormatter_FormatReport_WithDetails
    #[test]
    fn format_report_with_details() {
        let f = Formatter::new(FormatConfig {
            show_details: true,
            ..Default::default()
        });
        let r = report(&[
            ("analyzer_name", Value::String("test".into())),
            ("detail_field", Value::String("detail_value".into())),
        ]);
        let out = f.format_report(Some(&r));
        assert!(out.contains("Details:"));
    }

    // Ported from formatter_test.go: TestFormatter_CreateProgressBar
    #[test]
    fn create_progress_bar_status() {
        let f = Formatter::new(FormatConfig::default());
        assert!(f.create_progress_bar("test", 0.9).contains("\u{1F7E2} Good"));
        assert!(f.create_progress_bar("test", 0.7).contains("\u{1F7E1} Fair"));
        assert!(f.create_progress_bar("test", 0.3).contains("\u{1F534} Poor"));
    }

    // Ported from formatter_test.go: TestFormatter_FormatReport_SortedDetails
    #[test]
    fn format_report_sorted_details() {
        let f = Formatter::new(FormatConfig {
            show_details: true,
            ..Default::default()
        });
        let r = report(&[
            ("zebra", Value::String("last".into())),
            ("alpha", Value::String("first".into())),
        ]);
        let out = f.format_report(Some(&r));
        let alpha = out.find("alpha").unwrap();
        let zebra = out.find("zebra").unwrap();
        assert!(alpha < zebra);
    }

    // Ported from formatter_test.go: TestFormatter_SortCollection
    #[test]
    fn sort_collection_desc() {
        let f = Formatter::new(FormatConfig {
            show_tables: true,
            sort_by: "value".into(),
            sort_order: "desc".into(),
            ..Default::default()
        });
        let items = vec![
            report(&[("name", Value::String("low".into())), ("value", Value::Int(10))]),
            report(&[("name", Value::String("high".into())), ("value", Value::Int(30))]),
            report(&[("name", Value::String("mid".into())), ("value", Value::Int(20))]),
        ];
        let r = report(&[("items", Value::Collection(items))]);
        let out = f.format_report(Some(&r));
        let high = out.find("high").unwrap();
        let low = out.find("low").unwrap();
        assert!(high < low);
    }
}
