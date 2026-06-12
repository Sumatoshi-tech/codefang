//! Core data types for the devs analyzer.
//!
//! Field declaration order is part of the report-format contract: the wrapper
//! structs are emitted in source order by [`cf_gojson`] (a `GoMap` built
//! field-by-field reproduces struct-origin ordering, while dynamic map
//! payloads are byte-sorted on encode). Pinned by `rust/tests/compat`.

use std::collections::BTreeMap;

/// Line statistics for a change.
///
/// Serializes with lowercase keys (`added`/`removed`/`changed`); when this
/// appears as a per-language map value it is emitted as a struct-origin
/// object in that field order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LineStats {
    /// Lines added.
    pub added: i64,
    /// Lines removed.
    pub removed: i64,
    /// Lines changed.
    pub changed: i64,
}

impl LineStats {
    /// Adds another [`LineStats`] component-wise (additive merge).
    #[must_use]
    pub const fn plus(self, other: Self) -> Self {
        Self {
            added: self.added + other.added,
            removed: self.removed + other.removed,
            changed: self.changed + other.changed,
        }
    }
}

/// Aggregate dev stats for a single commit.
///
/// JSON field order: `commits, lines_added, lines_removed, lines_changed,
/// author_id, languages` (with `languages` omitted when empty). `languages`
/// is keyed by language name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommitDevData {
    /// Number of commits represented (1 for a single commit, summed on merge).
    pub commits: i64,
    /// Lines added across all changes in the commit.
    pub added: i64,
    /// Lines removed across all changes in the commit.
    pub removed: i64,
    /// Lines changed across all changes in the commit.
    pub changed: i64,
    /// Resolved author identity id.
    pub author_id: i64,
    /// Per-language line stats. Map-origin → byte-sorted on encode.
    pub languages: BTreeMap<String, LineStats>,
}

impl CommitDevData {
    /// Additively merges `incoming` into `self`.
    pub fn merge(&mut self, incoming: &Self) {
        self.commits += incoming.commits;
        self.added += incoming.added;
        self.removed += incoming.removed;
        self.changed += incoming.changed;

        for (lang, stats) in &incoming.languages {
            let entry = self.languages.entry(lang.clone()).or_default();
            *entry = entry.plus(*stats);
        }
    }
}

/// Per-tick, per-developer accumulated statistics.
///
/// Aggregate [`LineStats`] plus a per-language breakdown and a commit counter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DevTick {
    /// Embedded aggregate line stats.
    pub line_stats: LineStats,
    /// Per-language line stats for this developer in this tick.
    pub languages: BTreeMap<String, LineStats>,
    /// Number of commits by this developer in this tick.
    pub commits: i64,
}

/// Line stats for a single language inside a developer record.
/// JSON order: `language, added, removed, changed`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LanguageStatsEntry {
    /// Language name (empty → rendered as `"Other"` upstream of this type).
    pub language: String,
    /// Lines added in this language.
    pub added: i64,
    /// Lines removed in this language.
    pub removed: i64,
    /// Lines changed in this language.
    pub changed: i64,
}

/// Computed per-developer statistics.
///
/// JSON field order: `id, name, email(omitempty), commits, lines_added,
/// lines_removed, lines_changed, net_lines, languages, first_tick, last_tick,
/// active_ticks`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeveloperData {
    /// Developer id.
    pub id: i64,
    /// Resolved display name.
    pub name: String,
    /// Email (omitted when empty).
    pub email: String,
    /// Total commits.
    pub commits: i64,
    /// Total lines added.
    pub added: i64,
    /// Total lines removed.
    pub removed: i64,
    /// Total lines changed.
    pub changed: i64,
    /// Net lines (`added - removed`).
    pub net_lines: i64,
    /// Per-language stats, sorted by language name ascending.
    pub languages: Vec<LanguageStatsEntry>,
    /// First tick in which the developer was active.
    pub first_tick: i64,
    /// Last tick in which the developer was active.
    pub last_tick: i64,
    /// Number of distinct ticks the developer was active in.
    pub active_ticks: i64,
}

/// Computed per-language statistics.
///
/// JSON field order: `name, total_lines, total_contribution, contributors`.
/// `contributors` has integer keys, which serialize as decimal strings sorted
/// lexicographically (report-format contract).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LanguageData {
    /// Language name.
    pub name: String,
    /// Total lines added in this language across all developers.
    pub total_lines: i64,
    /// Total contribution (`added + removed`) across all developers.
    pub total_contribution: i64,
    /// Per-developer contribution (`dev id` → `added + removed`).
    pub contributors: BTreeMap<i64, i64>,
}

/// Bus-factor / knowledge-concentration risk for a language.
///
/// JSON field order is fixed by the report-format contract; several secondary
/// fields are omitted when empty/zero.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BusFactorData {
    /// Language name.
    pub language: String,
    /// Bus factor (smallest contributor count covering the threshold share).
    pub bus_factor: i64,
    /// Total contributor count for the language.
    pub total_contributors: i64,
    /// Primary (largest) contributor id.
    pub primary_dev_id: i64,
    /// Primary contributor display name.
    pub primary_dev_name: String,
    /// Primary contributor email (omitted when empty).
    pub primary_dev_email: String,
    /// Primary contributor percentage of total contribution.
    pub primary_pct: f64,
    /// Secondary contributor id (omitted when zero).
    pub secondary_dev_id: i64,
    /// Secondary contributor name (omitted when empty).
    pub secondary_dev_name: String,
    /// Secondary contributor email (omitted when empty).
    pub secondary_dev_email: String,
    /// Secondary contributor percentage (omitted when zero).
    pub secondary_pct: f64,
    /// Risk level label (`CRITICAL`/`HIGH`/`MEDIUM`/`LOW`).
    pub risk_level: String,
}

/// A developer's commit count within a single tick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeveloperCommits {
    /// Developer id.
    pub dev_id: i64,
    /// Commits in the tick.
    pub commits: i64,
}

/// Per-tick commit activity.
///
/// JSON order: `tick, start_time(omitempty), end_time(omitempty), by_developer,
/// total_commits`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActivityData {
    /// Tick index.
    pub tick: i64,
    /// Tick window start (RFC3339, omitted when empty).
    pub start_time: String,
    /// Tick window end (RFC3339, omitted when empty).
    pub end_time: String,
    /// Per-developer commit counts, sorted by developer id ascending.
    pub by_developer: Vec<DeveloperCommits>,
    /// Total commits in the tick.
    pub total_commits: i64,
}

/// Per-tick code churn.
///
/// JSON order: `tick, start_time(omitempty), end_time(omitempty), lines_added,
/// lines_removed, net_change`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChurnData {
    /// Tick index.
    pub tick: i64,
    /// Tick window start (RFC3339, omitted when empty).
    pub start_time: String,
    /// Tick window end (RFC3339, omitted when empty).
    pub end_time: String,
    /// Lines added in the tick.
    pub added: i64,
    /// Lines removed in the tick.
    pub removed: i64,
    /// Net change (`added - removed`).
    pub net: i64,
}

/// Aggregate summary statistics.
///
/// JSON field order is fixed by the report-format contract. `estimated_*`
/// fields are HLL-derived unsigned integers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AggregateData {
    /// Total commits across all developers.
    pub total_commits: i64,
    /// Total lines added across all developers.
    pub total_lines_added: i64,
    /// Total lines removed across all developers.
    pub total_lines_removed: i64,
    /// Exact total developer count.
    pub total_developers: i64,
    /// Exact active developer count.
    pub active_developers: i64,
    /// HLL-estimated total developer cardinality.
    pub estimated_total_developers: u64,
    /// HLL-estimated active developer cardinality.
    pub estimated_active_developers: u64,
    /// Analysis period length in ticks (max tick index).
    pub analysis_period_ticks: i64,
    /// Project-wide bus factor.
    pub project_bus_factor: i64,
    /// Distinct language count.
    pub total_languages: i64,
}

/// All computed metric results for the devs analyzer.
///
/// JSON order: `aggregate, developers, languages, busfactor, activity, churn`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ComputedMetrics {
    /// Aggregate summary.
    pub aggregate: AggregateData,
    /// Per-developer records (sorted by commits descending).
    pub developers: Vec<DeveloperData>,
    /// Per-language records (sorted by total lines descending).
    pub languages: Vec<LanguageData>,
    /// Per-language bus-factor records (sorted by risk priority).
    pub busfactor: Vec<BusFactorData>,
    /// Per-tick activity records (sorted by tick).
    pub activity: Vec<ActivityData>,
    /// Per-tick churn records (sorted by tick).
    pub churn: Vec<ChurnData>,
}

impl ComputedMetrics {
    /// Short analyzer identifier used in report metadata.
    #[must_use]
    pub const fn analyzer_name(&self) -> &'static str {
        "devs"
    }
}
