//! `static/complexity --format yaml`: the walk products of
//! [`super::static_complexity::collect`] serialized as the
//! [`cf_complexity::report::computed_metrics`] value
//! (`function_complexity`, `distribution`, `high_risk_functions`,
//! `aggregate`) through cf-goyaml. `--per-file` is a JSON-only enrichment
//! (the run dispatcher forwards the flag to the JSON handler alone).

use cf_complexity::report::computed_metrics;
use cf_goyaml::marshal;

use super::static_complexity::collect;
use super::StaticFilter;

/// Builds the `static/complexity --format yaml` report bytes for
/// `root_path`, or `None` when the path does not exist.
#[must_use]
pub fn complexity_report_yaml(root_path: &str, filter: &StaticFilter) -> Option<Vec<u8>> {
    let collected = collect(root_path, filter)?;
    Some(marshal(&computed_metrics(&collected.functions())))
}
