//! Metric computation for the devs analyzer.
//!
//! All map iteration that feeds an ordered result is done over sorted keys so
//! output is deterministic; no wall-clock is consulted.

use std::collections::BTreeMap;

use cf_alg_hll::Sketch;

use crate::model::{
    ActivityData, AggregateData, BusFactorData, ChurnData, ComputedMetrics, DevTick,
    DeveloperCommits, DeveloperData, LanguageData, LanguageStatsEntry, LineStats,
};

/// `HyperLogLog` precision for developer cardinality.
/// p=14 → 16384 registers, ~0.8% standard error.
pub const HLL_PRECISION: u8 = 14;

/// CHAOSS contribution-coverage threshold.
pub const BUS_FACTOR_THRESHOLD: f64 = 0.5;

/// Risk threshold: CRITICAL.
pub const THRESHOLD_CRITICAL: f64 = 90.0;
/// Risk threshold: HIGH.
pub const THRESHOLD_HIGH: f64 = 80.0;
/// Risk threshold: MEDIUM.
pub const THRESHOLD_MEDIUM: f64 = 60.0;

/// Fallback "recent" fraction of the analysis period.
pub const ACTIVE_THRESHOLD_RATIO: f64 = 0.7;
/// Time-based active-developer window in days.
pub const DEFAULT_ACTIVE_DAYS: i64 = 90;
/// Hours per day used in tick math.
pub const DEFAULT_TICK_HOURS: i64 = 24;

/// One hour expressed in nanoseconds.
const NANOS_PER_HOUR: i64 = 3_600_000_000_000;

/// Configurable thresholds for devs metric computation.
///
/// Zero-valued fields mean "use the crate-level default" (the analyzer stores
/// configured overrides as zero when unset).
#[derive(Debug, Clone, Copy)]
pub struct MetricOptions {
    /// Bus-factor coverage threshold (fraction).
    pub bus_factor_threshold: f64,
    /// CRITICAL risk percentage cutoff.
    pub risk_threshold_critical: f64,
    /// HIGH risk percentage cutoff.
    pub risk_threshold_high: f64,
    /// MEDIUM risk percentage cutoff.
    pub risk_threshold_medium: f64,
    /// Ratio fallback for the active window.
    pub active_threshold_ratio: f64,
    /// Active window in days.
    pub default_active_days: i64,
    /// HLL precision.
    pub hll_precision: u8,
}

impl Default for MetricOptions {
    fn default() -> Self {
        Self {
            bus_factor_threshold: BUS_FACTOR_THRESHOLD,
            risk_threshold_critical: THRESHOLD_CRITICAL,
            risk_threshold_high: THRESHOLD_HIGH,
            risk_threshold_medium: THRESHOLD_MEDIUM,
            active_threshold_ratio: ACTIVE_THRESHOLD_RATIO,
            default_active_days: DEFAULT_ACTIVE_DAYS,
            hll_precision: HLL_PRECISION,
        }
    }
}

/// Parsed input for metric computation, already aggregated to per-tick /
/// per-developer granularity.
#[derive(Debug, Clone, Default)]
pub struct TickData {
    /// `tick → (dev id → DevTick)`.
    pub ticks: BTreeMap<i64, BTreeMap<i64, DevTick>>,
    /// Reversed people dict (`dev id` index → `"Name <email>"`).
    pub names: Vec<String>,
    /// Tick size in nanoseconds (`time.Duration`).
    pub tick_size: i64,
    /// `tick → (start_time, end_time)` already formatted as RFC3339 strings
    /// (empty string == zero/unset time, i.e. omit).
    ///
    /// The metrics layer only ever reads the pre-formatted strings. When a
    /// tick has no entry, activity/churn emit no `start_time`/`end_time`
    /// (those report fields are omit-when-empty).
    pub tick_bounds: BTreeMap<i64, TickBounds>,
}

/// Pre-formatted time boundaries of a single tick.
///
/// Each field holds the RFC3339 rendering of the corresponding instant, or
/// the empty string when that time was unset (an empty string is dropped from
/// the report via the omit-when-empty rule).
#[derive(Debug, Clone, Default)]
pub struct TickBounds {
    /// Formatted start time, or `""` for an unset time.
    pub start_time: String,
    /// Formatted end time, or `""` for an unset time.
    pub end_time: String,
}

/// Converts a developer id to the deterministic byte slice the HLL sketch
/// hashes (decimal ASCII; part of the cardinality-estimate contract).
///
/// ```
/// use cf_devs::dev_id_bytes;
///
/// // The id is hashed as its decimal-ASCII representation, not raw bytes.
/// assert_eq!(dev_id_bytes(42), b"42".to_vec());
/// assert_eq!(dev_id_bytes(0), b"0".to_vec());
/// ```
#[must_use]
pub fn dev_id_bytes(id: i64) -> Vec<u8> {
    id.to_string().into_bytes()
}

/// Computes per-developer statistics.
///
/// Aggregates across all ticks, finalizes net lines + sorted languages, then
/// sorts developers by commit count descending. Ties are broken by developer
/// id ascending for deterministic byte output, which matches the reference
/// binary for the disjoint-key inputs exercised in the golden manifest.
#[must_use]
pub fn compute_developers(input: &TickData) -> Vec<DeveloperData> {
    // dev id → (developer accumulator, internal per-language map).
    let mut dev_map: BTreeMap<i64, (DeveloperData, BTreeMap<String, LineStats>)> = BTreeMap::new();

    for (&tick, dev_ticks) in &input.ticks {
        for (&dev_id, dt) in dev_ticks {
            let entry = dev_map.entry(dev_id).or_insert_with(|| {
                let (name, email) = dev_name_and_email(dev_id, &input.names);
                (
                    DeveloperData {
                        id: dev_id,
                        name,
                        email,
                        first_tick: tick,
                        last_tick: tick,
                        ..DeveloperData::default()
                    },
                    BTreeMap::new(),
                )
            });

            let (dev, lang_map) = entry;
            dev.commits += dt.commits;
            dev.added += dt.line_stats.added;
            dev.removed += dt.line_stats.removed;
            dev.changed += dt.line_stats.changed;
            dev.active_ticks += 1;

            if tick < dev.first_tick {
                dev.first_tick = tick;
            }
            if tick > dev.last_tick {
                dev.last_tick = tick;
            }

            for (lang, stats) in &dt.languages {
                let ls = lang_map.entry(lang.clone()).or_default();
                *ls = ls.plus(*stats);
            }
        }
    }

    let mut result: Vec<DeveloperData> = dev_map
        .into_values()
        .map(|(mut dev, lang_map)| {
            dev.net_lines = dev.added - dev.removed;
            dev.languages = finalize_languages(&lang_map);
            dev
        })
        .collect();

    // Sort by commits descending; tie-break by id ascending for determinism.
    result.sort_by(|a, b| b.commits.cmp(&a.commits).then(a.id.cmp(&b.id)));
    result
}

/// Converts an internal per-language map into a sorted `Vec`.
/// Empty language name → `"Other"`.
fn finalize_languages(lang_map: &BTreeMap<String, LineStats>) -> Vec<LanguageStatsEntry> {
    if lang_map.is_empty() {
        return Vec::new();
    }

    let mut out: Vec<LanguageStatsEntry> = lang_map
        .iter()
        .map(|(lang, stats)| {
            let language = if lang.is_empty() {
                "Other".to_string()
            } else {
                lang.clone()
            };
            LanguageStatsEntry {
                language,
                added: stats.added,
                removed: stats.removed,
                changed: stats.changed,
            }
        })
        .collect();

    out.sort_by(|a, b| a.language.cmp(&b.language));
    out
}

/// Computes per-language statistics.
///
/// Sorted by total lines descending; ties broken by name ascending for
/// determinism.
#[must_use]
pub fn compute_languages(developers: &[DeveloperData]) -> Vec<LanguageData> {
    let mut lang_map: BTreeMap<String, LanguageData> = BTreeMap::new();

    for dev in developers {
        for lang_entry in &dev.languages {
            let lang = if lang_entry.language.is_empty() {
                "Other".to_string()
            } else {
                lang_entry.language.clone()
            };

            let ld = lang_map
                .entry(lang.clone())
                .or_insert_with(|| LanguageData {
                    name: lang.clone(),
                    ..LanguageData::default()
                });

            ld.total_lines += lang_entry.added;
            let contribution = lang_entry.added + lang_entry.removed;
            ld.total_contribution += contribution;
            *ld.contributors.entry(dev.id).or_default() += contribution;
        }
    }

    let mut result: Vec<LanguageData> = lang_map.into_values().collect();
    result.sort_by(|a, b| b.total_lines.cmp(&a.total_lines).then(a.name.cmp(&b.name)));
    result
}

/// Input for bus-factor computation.
pub struct BusFactorInput<'a> {
    /// Per-language data.
    pub languages: &'a [LanguageData],
    /// Reversed people dict.
    pub names: &'a [String],
}

/// Computes bus-factor risk per language.
///
/// Sorted by risk priority ascending (CRITICAL first); see the tie-handling
/// notes inline (the sort is intentionally unstable to match the reference
/// binary's permutation).
#[must_use]
#[allow(clippy::cast_precision_loss)] // contractual float math on line counts
pub fn compute_bus_factor(input: &BusFactorInput, opts: &MetricOptions) -> Vec<BusFactorData> {
    let mut result: Vec<BusFactorData> = Vec::with_capacity(input.languages.len());

    for ld in input.languages {
        if ld.total_contribution == 0 {
            continue;
        }

        // (id, lines) sorted descending by lines via the unstable pdqsort port
        // (reference-implementation behavior, pinned by the differential gate).
        // The contributor input order is the BTreeMap id-ascending order
        // (deterministic; this only affects the order of equal-line
        // contributors, where the reference binary is itself nondeterministic).
        let mut contribs: Vec<(i64, i64)> = ld
            .contributors
            .iter()
            .map(|(&id, &lines)| (id, lines))
            .collect();
        cf_gosort::go_sort_slice(&mut contribs, |a, b| a.1 > b.1);

        let sorted_amounts: Vec<i64> = contribs.iter().map(|c| c.1).collect();

        let mut bf = BusFactorData {
            language: ld.name.clone(),
            total_contributors: i64::try_from(contribs.len()).unwrap_or(i64::MAX),
            bus_factor: compute_bus_factor_from_sorted(
                &sorted_amounts,
                ld.total_contribution,
                opts.bus_factor_threshold,
            ),
            ..BusFactorData::default()
        };

        if let Some(&(id, lines)) = contribs.first() {
            bf.primary_dev_id = id;
            let (n, e) = dev_name_and_email(id, input.names);
            bf.primary_dev_name = n;
            bf.primary_dev_email = e;
            bf.primary_pct = cf_alg_stats::to_percent(lines as f64 / ld.total_contribution as f64);
        }

        if let Some(&(id, lines)) = contribs.get(1) {
            bf.secondary_dev_id = id;
            let (n, e) = dev_name_and_email(id, input.names);
            bf.secondary_dev_name = n;
            bf.secondary_dev_email = e;
            bf.secondary_pct =
                cf_alg_stats::to_percent(lines as f64 / ld.total_contribution as f64);
        }

        bf.risk_level = if bf.primary_pct >= opts.risk_threshold_critical {
            cf_metrics::RISK_CRITICAL.to_string()
        } else if bf.primary_pct >= opts.risk_threshold_high {
            cf_metrics::RISK_HIGH.to_string()
        } else if bf.primary_pct >= opts.risk_threshold_medium {
            cf_metrics::RISK_MEDIUM.to_string()
        } else {
            cf_metrics::RISK_LOW.to_string()
        };

        result.push(bf);
    }

    // An UNSTABLE pdqsort keyed ONLY on risk priority (no secondary key).
    // The exact tie permutation (equal-priority runs) reproduces the reference
    // binary by running the same unstable sort over the same input order
    // (the `input.languages` slice order, total_lines desc).
    cf_gosort::go_sort_slice(&mut result, |a, b| {
        let pa = cf_metrics::risk_priority(&cf_metrics::RiskLevel::from(a.risk_level.as_str()));
        let pb = cf_metrics::risk_priority(&cf_metrics::RiskLevel::from(b.risk_level.as_str()));
        pa < pb
    });
    result
}

/// Smallest number of (descending-sorted) contributors covering at least
/// `threshold` of `total`.
///
/// ```
/// use cf_devs::compute_bus_factor_from_sorted;
///
/// // Contributions [50, 30, 20] (already descending), total 100, 50% threshold:
/// // the top contributor alone (50) reaches the target.
/// assert_eq!(compute_bus_factor_from_sorted(&[50, 30, 20], 100, 0.5), 1);
/// // 80% threshold needs the top two (50 + 30 = 80).
/// assert_eq!(compute_bus_factor_from_sorted(&[50, 30, 20], 100, 0.8), 2);
/// // Empty / zero-total guards return 0.
/// assert_eq!(compute_bus_factor_from_sorted(&[], 100, 0.5), 0);
/// ```
#[must_use]
#[allow(clippy::cast_precision_loss)] // contractual float math on line counts
pub fn compute_bus_factor_from_sorted(sorted: &[i64], total: i64, threshold: f64) -> i64 {
    if total == 0 || sorted.is_empty() {
        return 0;
    }

    let target = total as f64 * threshold;
    let mut cumulative = 0i64;

    for (i, &amount) in sorted.iter().enumerate() {
        cumulative += amount;
        if cumulative as f64 >= target {
            return i64::try_from(i + 1).unwrap_or(i64::MAX);
        }
    }

    i64::try_from(sorted.len()).unwrap_or(i64::MAX)
}

/// Computes the per-tick activity time series.
///
/// Ordered by tick ascending; developers within a tick ordered by id ascending.
#[must_use]
pub fn compute_activity(input: &TickData) -> Vec<ActivityData> {
    let mut result = Vec::with_capacity(input.ticks.len());

    for (&tick, dev_ticks) in &input.ticks {
        let mut ad = ActivityData {
            tick,
            ..ActivityData::default()
        };

        for (&dev_id, dt) in dev_ticks {
            ad.by_developer.push(DeveloperCommits {
                dev_id,
                commits: dt.commits,
            });
            ad.total_commits += dt.commits;
        }

        if let Some(bounds) = input.tick_bounds.get(&tick) {
            ad.start_time.clone_from(&bounds.start_time);
            ad.end_time.clone_from(&bounds.end_time);
        }

        result.push(ad);
    }

    result
}

/// Computes the per-tick churn time series.
///
/// Ordered by tick ascending.
#[must_use]
pub fn compute_churn(input: &TickData) -> Vec<ChurnData> {
    let mut result = Vec::with_capacity(input.ticks.len());

    for (&tick, dev_ticks) in &input.ticks {
        let mut cd = ChurnData {
            tick,
            ..ChurnData::default()
        };

        for dt in dev_ticks.values() {
            cd.added += dt.line_stats.added;
            cd.removed += dt.line_stats.removed;
        }

        cd.net = cd.added - cd.removed;

        if let Some(bounds) = input.tick_bounds.get(&tick) {
            cd.start_time.clone_from(&bounds.start_time);
            cd.end_time.clone_from(&bounds.end_time);
        }

        result.push(cd);
    }

    result
}

/// Input for aggregate computation.
pub struct AggregateInput<'a> {
    /// Per-developer data.
    pub developers: &'a [DeveloperData],
    /// Per-language data (only its length is used: `total_languages`).
    pub languages: &'a [LanguageData],
    /// Per-tick / per-developer data.
    pub ticks: &'a BTreeMap<i64, BTreeMap<i64, DevTick>>,
    /// Tick size in nanoseconds.
    pub tick_size: i64,
}

/// Computes aggregate summary statistics.
#[must_use]
pub fn compute_aggregate(input: &AggregateInput, opts: &MetricOptions) -> AggregateData {
    let mut agg = AggregateData {
        total_developers: i64::try_from(input.developers.len()).unwrap_or(i64::MAX),
        total_languages: i64::try_from(input.languages.len()).unwrap_or(i64::MAX),
        ..AggregateData::default()
    };

    let total_sketch = build_total_dev_sketch(input.developers, opts.hll_precision);

    for d in input.developers {
        agg.total_commits += d.commits;
        agg.total_lines_added += d.added;
        agg.total_lines_removed += d.removed;
    }

    if let Some(sketch) = total_sketch {
        agg.estimated_total_developers = sketch.count();
    }

    if !input.ticks.is_empty() {
        // Sorted keys → last is max tick.
        let max_tick = *input.ticks.keys().next_back().unwrap_or(&0);
        agg.analysis_period_ticks = max_tick;

        let recent_threshold = compute_active_threshold(max_tick, input.tick_size, opts);
        let active_sketch =
            build_active_dev_sketch(input.ticks, recent_threshold, opts.hll_precision);

        let mut active_devs: BTreeMap<i64, bool> = BTreeMap::new();
        for (&tick, dev_ticks) in input.ticks {
            if tick >= recent_threshold {
                for &dev_id in dev_ticks.keys() {
                    active_devs.insert(dev_id, true);
                }
            }
        }
        agg.active_developers = i64::try_from(active_devs.len()).unwrap_or(i64::MAX);

        if let Some(sketch) = active_sketch {
            agg.estimated_active_developers = sketch.count();
        }
    }

    agg.project_bus_factor =
        compute_project_bus_factor(input.developers, opts.bus_factor_threshold);

    agg
}

/// Builds an HLL sketch over all developer ids. Returns `None` for empty
/// input (no sketch → estimate stays 0).
fn build_total_dev_sketch(developers: &[DeveloperData], precision: u8) -> Option<Sketch> {
    if developers.is_empty() {
        return None;
    }

    let mut sketch = Sketch::new(precision).ok()?;
    for d in developers {
        sketch.add(&dev_id_bytes(d.id));
    }
    Some(sketch)
}

/// Builds an HLL sketch over active developer ids. Unlike the total sketch
/// this is built even when no tick qualifies (an empty sketch → estimate 0).
fn build_active_dev_sketch(
    ticks: &BTreeMap<i64, BTreeMap<i64, DevTick>>,
    threshold: i64,
    precision: u8,
) -> Option<Sketch> {
    let mut sketch = Sketch::new(precision).ok()?;
    for (&tick, dev_ticks) in ticks {
        if tick >= threshold {
            for &dev_id in dev_ticks.keys() {
                sketch.add(&dev_id_bytes(dev_id));
            }
        }
    }
    Some(sketch)
}

/// Returns the active-window tick threshold.
#[must_use]
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
// The ratio fallback's truncating float→int round-trip is part of the
// reference-implementation behavior (pinned by the differential gate).
pub fn compute_active_threshold(max_tick: i64, tick_size: i64, opts: &MetricOptions) -> i64 {
    if tick_size > 0 {
        let active_days = opts.default_active_days;
        let active_duration = active_days * DEFAULT_TICK_HOURS * NANOS_PER_HOUR;
        let ticks_for_active = active_duration / tick_size; // integer division
        let threshold = max_tick - ticks_for_active;
        if threshold < 0 {
            return 0;
        }
        return threshold;
    }

    // Ratio fallback: the float product truncates toward zero.
    (max_tick as f64 * opts.active_threshold_ratio) as i64
}

/// Computes the project-wide bus factor.
#[must_use]
pub fn compute_project_bus_factor(developers: &[DeveloperData], threshold: f64) -> i64 {
    if developers.is_empty() {
        return 0;
    }

    let mut contribs: Vec<i64> = developers.iter().map(|d| d.added + d.removed).collect();
    let total: i64 = contribs.iter().sum();
    // Descending sort over contribution amounts only.
    contribs.sort_by(|a, b| b.cmp(a));

    compute_bus_factor_from_sorted(&contribs, total, threshold)
}

/// Resolves an HLL estimate's relative error against an exact count (test util).
#[must_use]
#[allow(clippy::cast_precision_loss)] // test utility; counts are small
pub fn relative_error(estimated: u64, exact: u64) -> f64 {
    (estimated as f64 - exact as f64).abs() / exact as f64
}

/// Resolves a developer's display name and email.
///
/// `AUTHOR_MISSING` → `(AUTHOR_MISSING_NAME, "")`; in-range id → split
/// identity; out-of-range id → `("dev_<id>", "")`.
///
/// ```
/// use cf_devs::dev_name_and_email;
///
/// let names = vec!["Ada Lovelace|ada@example.com".to_string()];
/// // In-range id splits the "name|email" identity.
/// assert_eq!(
///     dev_name_and_email(0, &names),
///     ("Ada Lovelace".to_string(), "ada@example.com".to_string()),
/// );
/// // An id past the end of the name list falls back to "dev_<id>".
/// assert_eq!(dev_name_and_email(7, &names), ("dev_7".to_string(), String::new()));
/// ```
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0 <= id < len checked
pub fn dev_name_and_email(id: i64, names: &[String]) -> (String, String) {
    if id == i64::from(cf_identity::AUTHOR_MISSING) {
        return (cf_identity::AUTHOR_MISSING_NAME.to_string(), String::new());
    }

    if id >= 0 && (id as usize) < names.len() {
        return cf_identity::split_identity(&names[id as usize]);
    }

    (format!("dev_{id}"), String::new())
}

/// Runs all devs metrics in dependency order.
#[must_use]
pub fn compute_all_metrics(input: &TickData, opts: &MetricOptions) -> ComputedMetrics {
    let developers = compute_developers(input);
    let languages = compute_languages(&developers);
    let busfactor = compute_bus_factor(
        &BusFactorInput {
            languages: &languages,
            names: &input.names,
        },
        opts,
    );
    let activity = compute_activity(input);
    let churn = compute_churn(input);
    let aggregate = compute_aggregate(
        &AggregateInput {
            developers: &developers,
            languages: &languages,
            ticks: &input.ticks,
            tick_size: input.tick_size,
        },
        opts,
    );

    ComputedMetrics {
        aggregate,
        developers,
        languages,
        busfactor,
        activity,
        churn,
    }
}
