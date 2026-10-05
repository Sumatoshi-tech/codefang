//! Vocabulary of the info-only IMPORTS report section.
//!
//! The section renderers (text / json) live in the command layer; this
//! module owns the labels, keys and status line they and the raw report
//! share, so the strings exist exactly once.

/// Section title.
pub const SECTION_TITLE: &str = "IMPORTS";
/// Score label of an info-only section.
pub const SCORE_LABEL_INFO: &str = "Info";
/// Score of an info-only section.
pub const SCORE_INFO: f64 = -1.0;
/// Label for the unique-imports metric.
pub const METRIC_UNIQUE_IMPORTS: &str = "Unique Imports";
/// Label for the total-files metric.
pub const METRIC_TOTAL_FILES: &str = "Total Files";
/// Severity label of an import issue.
pub const SEVERITY_INFO: &str = "info";

/// Raw-report key for the imports list.
pub const KEY_IMPORTS: &str = "imports";
/// Raw-report key for the per-import file counts map.
pub const KEY_IMPORT_COUNTS: &str = "import_counts";
/// Raw-report key for the unique-import count.
pub const KEY_COUNT: &str = "count";
/// Raw-report key for the analyzed-file count.
pub const KEY_TOTAL_FILES: &str = "total_files";

/// Status line when no imports were found.
pub const DEFAULT_STATUS_MESSAGE: &str = "No import data available";
const STATUS_MESSAGE_PREFIX: &str = "Found ";
const STATUS_MESSAGE_SUFFIX: &str = " unique imports";

/// The section status line for a unique-import count.
///
/// ```
/// use cf_imports::section::{status_message, DEFAULT_STATUS_MESSAGE};
///
/// assert_eq!(status_message(0), DEFAULT_STATUS_MESSAGE);
/// assert_eq!(status_message(4), "Found 4 unique imports");
/// ```
#[must_use]
pub fn status_message(unique_imports: i64) -> String {
    if unique_imports == 0 {
        return DEFAULT_STATUS_MESSAGE.to_string();
    }
    format!("{STATUS_MESSAGE_PREFIX}{unique_imports}{STATUS_MESSAGE_SUFFIX}")
}
