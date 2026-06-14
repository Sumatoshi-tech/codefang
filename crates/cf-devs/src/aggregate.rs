//! Commit-to-tick aggregation, report parsing, and merge helpers.
//!
//! All merges are additive and clock-free.

use std::collections::BTreeMap;

use crate::metrics::TickData;
use crate::model::{CommitDevData, DevTick, LineStats};

/// One hour in nanoseconds.
const NANOS_PER_HOUR: i64 = 3_600_000_000_000;
/// Hours per day.
const DEFAULT_TICK_HOURS: i64 = 24;

/// Builds per-tick / per-developer data from per-commit data grouped by the
/// `commits_by_tick` mapping.
///
/// Returns an empty map when either input is empty.
#[must_use]
pub fn aggregate_commits_to_ticks(
    commit_dev_data: &BTreeMap<String, CommitDevData>,
    commits_by_tick: &BTreeMap<i64, Vec<String>>,
) -> BTreeMap<i64, BTreeMap<i64, DevTick>> {
    let mut result: BTreeMap<i64, BTreeMap<i64, DevTick>> = BTreeMap::new();

    if commit_dev_data.is_empty() || commits_by_tick.is_empty() {
        return result;
    }

    for (&tick, hashes) in commits_by_tick {
        let dev_ticks = aggregate_dev_tick_from_commits(hashes, commit_dev_data);
        if !dev_ticks.is_empty() {
            result.insert(tick, dev_ticks);
        }
    }

    result
}

/// Merges commit-level dev data into per-author [`DevTick`] entries for a
/// single tick.
fn aggregate_dev_tick_from_commits(
    hashes: &[String],
    commit_dev_data: &BTreeMap<String, CommitDevData>,
) -> BTreeMap<i64, DevTick> {
    let mut dev_ticks: BTreeMap<i64, DevTick> = BTreeMap::new();

    for hash in hashes {
        let Some(cdd) = commit_dev_data.get(hash) else {
            continue;
        };

        let dt = dev_ticks.entry(cdd.author_id).or_default();
        dt.commits += cdd.commits;
        dt.line_stats.added += cdd.added;
        dt.line_stats.removed += cdd.removed;
        dt.line_stats.changed += cdd.changed;

        for (lang, lang_st) in &cdd.languages {
            let ls = dt.languages.entry(lang.clone()).or_default();
            *ls = ls.plus(*lang_st);
        }
    }

    dev_ticks
}

/// Resolves the tick size from a possibly-zero configured value, defaulting
/// to 24 hours for non-positive inputs.
#[must_use]
pub const fn resolve_tick_size(tick_size: i64) -> i64 {
    if tick_size > 0 {
        tick_size
    } else {
        DEFAULT_TICK_HOURS * NANOS_PER_HOUR
    }
}

/// Builds [`TickData`] from raw inputs, applying the standard aggregation and
/// tick-size default.
#[must_use]
pub fn parse_tick_data(
    commit_dev_data: &BTreeMap<String, CommitDevData>,
    commits_by_tick: &BTreeMap<i64, Vec<String>>,
    names: Vec<String>,
    tick_size: i64,
) -> TickData {
    let ticks = if !commit_dev_data.is_empty() && !commits_by_tick.is_empty() {
        aggregate_commits_to_ticks(commit_dev_data, commits_by_tick)
    } else {
        BTreeMap::new()
    };

    TickData {
        ticks,
        names,
        tick_size: resolve_tick_size(tick_size),
        tick_bounds: BTreeMap::new(),
    }
}

/// Builds [`TickData`] from raw inputs together with per-tick time bounds.
///
/// Same aggregation + tick-size default as [`parse_tick_data`], plus the
/// `tick → (start,end)` bounds copied from the report's `tick_bounds` key.
/// `tick_bounds` values are already-RFC3339-formatted strings (`""` == unset
/// time → omitted from activity/churn records).
#[must_use]
pub fn parse_tick_data_with_bounds(
    commit_dev_data: &BTreeMap<String, CommitDevData>,
    commits_by_tick: &BTreeMap<i64, Vec<String>>,
    names: Vec<String>,
    tick_size: i64,
    tick_bounds: BTreeMap<i64, crate::metrics::TickBounds>,
) -> TickData {
    let mut td = parse_tick_data(commit_dev_data, commits_by_tick, names, tick_size);
    td.tick_bounds = tick_bounds;
    td
}

/// Additively merges two `commit hash → CommitDevData` maps.
pub fn merge_dev_data(
    existing: &mut BTreeMap<String, CommitDevData>,
    incoming: &BTreeMap<String, CommitDevData>,
) {
    for (hash, cdd) in incoming {
        match existing.get_mut(hash) {
            Some(ext) => ext.merge(cdd),
            None => {
                existing.insert(hash.clone(), cdd.clone());
            }
        }
    }
}

/// Accumulates per-commit line stats from per-change stats keyed by blob hash,
/// resolving each blob's language.
///
/// `changes` is `(blob_hash, LineStats)`; `languages` maps `blob_hash → lang`.
/// This is the clock-free scoring path used while consuming commits.
pub fn accumulate_line_stats(
    cdd: &mut CommitDevData,
    changes: &[(String, LineStats)],
    languages: &BTreeMap<String, String>,
) {
    for (blob_hash, stats) in changes {
        cdd.added += stats.added;
        cdd.removed += stats.removed;
        cdd.changed += stats.changed;

        let lang = languages.get(blob_hash).cloned().unwrap_or_default();
        let ls = cdd.languages.entry(lang).or_default();
        *ls = ls.plus(*stats);
    }
}
