//! `static/complexity --format bin`: the walk products of
//! [`super::static_complexity::collect`] serialized as the
//! [`cf_complexity::report::computed_metrics`] value inside the CFB1
//! envelope (cf-reportutil: magic + LE u32 length + compact payload).
//! `--per-file` is a JSON-only enrichment.

use cf_complexity::report::computed_metrics;
use cf_reportutil::encode_binary_envelope;

use super::static_complexity::collect;
use super::StaticFilter;

/// Builds the `static/complexity --format bin` report bytes for `root_path`.
/// `None` when the path does not exist, or when the payload exceeds the
/// envelope limit (reported on stderr; the run then fails instead of
/// panicking).
#[must_use]
pub fn complexity_report_bin(root_path: &str, filter: &StaticFilter) -> Option<Vec<u8>> {
    let collected = collect(root_path, filter)?;
    match encode_binary_envelope(&computed_metrics(&collected.functions())) {
        Ok(bytes) => Some(bytes),
        Err(e) => {
            eprintln!("Error: static/complexity --format bin: {e}");
            None
        }
    }
}
