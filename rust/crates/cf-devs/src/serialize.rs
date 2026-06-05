//! Machine-format serialization, routed exclusively through [`cf_gojson`].
//!
//! Every wrapper struct is emitted as a **struct-origin** [`GoMap`] built in Go
//! field-declaration order (honoring `omitempty`); every dynamic `map[...]`
//! payload is built as a **map-origin** `GoMap` that the encoder byte-sorts on
//! encode (DESIGN §2.2). This module never touches `serde_json`.
//!
//! Go encodes integer map keys (`map[int]int`, `map[int][]Hash`) as decimal
//! strings sorted lexicographically; we reproduce that by inserting stringified
//! keys into a map-origin `GoMap`.

use std::collections::BTreeMap;

use cf_gojson::{GoMap, GoValue};

use crate::model::{
    ActivityData, AggregateData, BusFactorData, ChurnData, CommitDevData, ComputedMetrics,
    DeveloperCommits, DeveloperData, LanguageData, LanguageStatsEntry, LineStats,
};

/// Builds the `languages` value for [`CommitDevData`] / `DevTick`: a map-origin
/// object whose values are struct-origin `{added, removed, changed}`.
fn line_stats_map_value(map: &BTreeMap<String, LineStats>) -> GoValue {
    let mut obj = GoMap::new_map();
    for (lang, stats) in map {
        obj.insert(lang.clone(), line_stats_struct_value(*stats));
    }
    GoValue::Object(obj)
}

/// Struct-origin `{added, removed, changed}` value (Go `LineStats` JSON tags).
fn line_stats_struct_value(stats: LineStats) -> GoValue {
    let mut obj = GoMap::new_struct();
    obj.insert("added".to_string(), GoValue::Int(stats.added));
    obj.insert("removed".to_string(), GoValue::Int(stats.removed));
    obj.insert("changed".to_string(), GoValue::Int(stats.changed));
    GoValue::Object(obj)
}

/// [`CommitDevData`] → struct-origin GoValue
/// (`commits, lines_added, lines_removed, lines_changed, author_id,
/// languages(omitempty)`).
#[must_use]
pub fn commit_dev_data_to_go(cdd: &CommitDevData) -> GoValue {
    let mut obj = GoMap::new_struct();
    obj.insert("commits".to_string(), GoValue::Int(cdd.commits));
    obj.insert("lines_added".to_string(), GoValue::Int(cdd.added));
    obj.insert("lines_removed".to_string(), GoValue::Int(cdd.removed));
    obj.insert("lines_changed".to_string(), GoValue::Int(cdd.changed));
    obj.insert("author_id".to_string(), GoValue::Int(cdd.author_id));
    if !cdd.languages.is_empty() {
        obj.insert("languages".to_string(), line_stats_map_value(&cdd.languages));
    }
    GoValue::Object(obj)
}

/// [`LanguageStatsEntry`] → struct-origin GoValue
/// (`language, added, removed, changed`).
fn language_stats_entry_to_go(e: &LanguageStatsEntry) -> GoValue {
    let mut obj = GoMap::new_struct();
    obj.insert("language".to_string(), GoValue::Str(e.language.clone()));
    obj.insert("added".to_string(), GoValue::Int(e.added));
    obj.insert("removed".to_string(), GoValue::Int(e.removed));
    obj.insert("changed".to_string(), GoValue::Int(e.changed));
    GoValue::Object(obj)
}

/// [`DeveloperData`] → struct-origin GoValue.
#[must_use]
pub fn developer_data_to_go(d: &DeveloperData) -> GoValue {
    let mut obj = GoMap::new_struct();
    obj.insert("id".to_string(), GoValue::Int(d.id));
    obj.insert("name".to_string(), GoValue::Str(d.name.clone()));
    if !d.email.is_empty() {
        obj.insert("email".to_string(), GoValue::Str(d.email.clone()));
    }
    obj.insert("commits".to_string(), GoValue::Int(d.commits));
    obj.insert("lines_added".to_string(), GoValue::Int(d.added));
    obj.insert("lines_removed".to_string(), GoValue::Int(d.removed));
    obj.insert("lines_changed".to_string(), GoValue::Int(d.changed));
    obj.insert("net_lines".to_string(), GoValue::Int(d.net_lines));
    // Go: `Languages []LanguageStatsEntry json:"languages"` — non-omitempty.
    // A nil slice marshals to `null`; an empty (non-nil) slice to `[]`.
    // finalizeLanguages leaves it nil when there are no languages.
    let langs: Vec<GoValue> = d.languages.iter().map(language_stats_entry_to_go).collect();
    obj.insert(
        "languages".to_string(),
        if d.languages.is_empty() {
            GoValue::Null
        } else {
            GoValue::Array(langs)
        },
    );
    obj.insert("first_tick".to_string(), GoValue::Int(d.first_tick));
    obj.insert("last_tick".to_string(), GoValue::Int(d.last_tick));
    obj.insert("active_ticks".to_string(), GoValue::Int(d.active_ticks));
    GoValue::Object(obj)
}

/// [`LanguageData`] → struct-origin GoValue. `contributors` is a `map[int]int`
/// → map-origin object with decimal-string keys, byte-sorted on encode.
#[must_use]
pub fn language_data_to_go(l: &LanguageData) -> GoValue {
    let mut obj = GoMap::new_struct();
    obj.insert("name".to_string(), GoValue::Str(l.name.clone()));
    obj.insert("total_lines".to_string(), GoValue::Int(l.total_lines));
    obj.insert(
        "total_contribution".to_string(),
        GoValue::Int(l.total_contribution),
    );

    let mut contribs = GoMap::new_map();
    for (id, lines) in &l.contributors {
        contribs.insert(id.to_string(), GoValue::Int(*lines));
    }
    obj.insert("contributors".to_string(), GoValue::Object(contribs));
    GoValue::Object(obj)
}

/// [`BusFactorData`] → struct-origin GoValue with the Go field order and
/// `omitempty` rules.
#[must_use]
pub fn bus_factor_data_to_go(b: &BusFactorData) -> GoValue {
    let mut obj = GoMap::new_struct();
    obj.insert("language".to_string(), GoValue::Str(b.language.clone()));
    obj.insert("bus_factor".to_string(), GoValue::Int(b.bus_factor));
    obj.insert(
        "total_contributors".to_string(),
        GoValue::Int(b.total_contributors),
    );
    obj.insert("primary_dev_id".to_string(), GoValue::Int(b.primary_dev_id));
    obj.insert(
        "primary_dev_name".to_string(),
        GoValue::Str(b.primary_dev_name.clone()),
    );
    if !b.primary_dev_email.is_empty() {
        obj.insert(
            "primary_dev_email".to_string(),
            GoValue::Str(b.primary_dev_email.clone()),
        );
    }
    obj.insert("primary_percentage".to_string(), GoValue::Float(b.primary_pct));
    if b.secondary_dev_id != 0 {
        obj.insert(
            "secondary_dev_id".to_string(),
            GoValue::Int(b.secondary_dev_id),
        );
    }
    if !b.secondary_dev_name.is_empty() {
        obj.insert(
            "secondary_dev_name".to_string(),
            GoValue::Str(b.secondary_dev_name.clone()),
        );
    }
    if !b.secondary_dev_email.is_empty() {
        obj.insert(
            "secondary_dev_email".to_string(),
            GoValue::Str(b.secondary_dev_email.clone()),
        );
    }
    if b.secondary_pct != 0.0 {
        obj.insert(
            "secondary_percentage".to_string(),
            GoValue::Float(b.secondary_pct),
        );
    }
    obj.insert("risk_level".to_string(), GoValue::Str(b.risk_level.clone()));
    GoValue::Object(obj)
}

/// [`DeveloperCommits`] → struct-origin GoValue (`dev_id, commits`).
fn developer_commits_to_go(c: &DeveloperCommits) -> GoValue {
    let mut obj = GoMap::new_struct();
    obj.insert("dev_id".to_string(), GoValue::Int(c.dev_id));
    obj.insert("commits".to_string(), GoValue::Int(c.commits));
    GoValue::Object(obj)
}

/// [`ActivityData`] → struct-origin GoValue.
#[must_use]
pub fn activity_data_to_go(a: &ActivityData) -> GoValue {
    let mut obj = GoMap::new_struct();
    obj.insert("tick".to_string(), GoValue::Int(a.tick));
    if !a.start_time.is_empty() {
        obj.insert("start_time".to_string(), GoValue::Str(a.start_time.clone()));
    }
    if !a.end_time.is_empty() {
        obj.insert("end_time".to_string(), GoValue::Str(a.end_time.clone()));
    }
    // Go: `ByDeveloper []DeveloperCommits json:"by_developer"`. ActivityMetric
    // always makes a non-nil slice → `[]` when empty.
    let by: Vec<GoValue> = a.by_developer.iter().map(developer_commits_to_go).collect();
    obj.insert("by_developer".to_string(), GoValue::Array(by));
    obj.insert("total_commits".to_string(), GoValue::Int(a.total_commits));
    GoValue::Object(obj)
}

/// [`ChurnData`] → struct-origin GoValue.
#[must_use]
pub fn churn_data_to_go(c: &ChurnData) -> GoValue {
    let mut obj = GoMap::new_struct();
    obj.insert("tick".to_string(), GoValue::Int(c.tick));
    if !c.start_time.is_empty() {
        obj.insert("start_time".to_string(), GoValue::Str(c.start_time.clone()));
    }
    if !c.end_time.is_empty() {
        obj.insert("end_time".to_string(), GoValue::Str(c.end_time.clone()));
    }
    obj.insert("lines_added".to_string(), GoValue::Int(c.added));
    obj.insert("lines_removed".to_string(), GoValue::Int(c.removed));
    obj.insert("net_change".to_string(), GoValue::Int(c.net));
    GoValue::Object(obj)
}

/// [`AggregateData`] → struct-origin GoValue. Estimated fields are `uint64`.
#[must_use]
pub fn aggregate_data_to_go(a: &AggregateData) -> GoValue {
    let mut obj = GoMap::new_struct();
    obj.insert("total_commits".to_string(), GoValue::Int(a.total_commits));
    obj.insert(
        "total_lines_added".to_string(),
        GoValue::Int(a.total_lines_added),
    );
    obj.insert(
        "total_lines_removed".to_string(),
        GoValue::Int(a.total_lines_removed),
    );
    obj.insert(
        "total_developers".to_string(),
        GoValue::Int(a.total_developers),
    );
    obj.insert(
        "active_developers".to_string(),
        GoValue::Int(a.active_developers),
    );
    obj.insert(
        "estimated_total_developers".to_string(),
        GoValue::Uint(a.estimated_total_developers),
    );
    obj.insert(
        "estimated_active_developers".to_string(),
        GoValue::Uint(a.estimated_active_developers),
    );
    obj.insert(
        "analysis_period_ticks".to_string(),
        GoValue::Int(a.analysis_period_ticks),
    );
    obj.insert(
        "project_bus_factor".to_string(),
        GoValue::Int(a.project_bus_factor),
    );
    obj.insert("total_languages".to_string(), GoValue::Int(a.total_languages));
    GoValue::Object(obj)
}

/// [`ComputedMetrics`] → struct-origin GoValue
/// (`aggregate, developers, languages, busfactor, activity, churn`).
///
/// This is the value the `run --analyzers history/devs --format json|bin|...`
/// path marshals. Slices use `null` when their Go origin would be a nil slice;
/// `ComputeAllMetrics` always allocates them, so they marshal to `[]` when
/// empty here.
#[must_use]
pub fn computed_metrics_to_go(m: &ComputedMetrics) -> GoValue {
    let mut obj = GoMap::new_struct();
    obj.insert("aggregate".to_string(), aggregate_data_to_go(&m.aggregate));
    obj.insert(
        "developers".to_string(),
        GoValue::Array(m.developers.iter().map(developer_data_to_go).collect()),
    );
    obj.insert(
        "languages".to_string(),
        GoValue::Array(m.languages.iter().map(language_data_to_go).collect()),
    );
    obj.insert(
        "busfactor".to_string(),
        GoValue::Array(m.busfactor.iter().map(bus_factor_data_to_go).collect()),
    );
    obj.insert(
        "activity".to_string(),
        GoValue::Array(m.activity.iter().map(activity_data_to_go).collect()),
    );
    obj.insert(
        "churn".to_string(),
        GoValue::Array(m.churn.iter().map(churn_data_to_go).collect()),
    );
    GoValue::Object(obj)
}
