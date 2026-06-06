//! Metric computation for the devs analyzer.
//!
//! Faithful port of `internal/analyzers/devs/metrics.go`. All map iteration that
//! feeds an ordered result is done over sorted keys so output is deterministic
//! (Go uses `sort.Slice` / `mapx.SortedKeys`); no wall-clock is consulted.

use std::collections::BTreeMap;

use cf_alg_hll::Sketch;

use crate::model::{
    ActivityData, AggregateData, BusFactorData, ChurnData, ComputedMetrics, DeveloperCommits,
    DeveloperData, DevTick, LanguageData, LanguageStatsEntry, LineStats,
};

/// HyperLogLog precision for developer cardinality (Go `hllPrecision = 14`).
/// p=14 → 16384 registers, ~0.8% standard error.
pub const HLL_PRECISION: u8 = 14;

/// CHAOSS contribution-coverage threshold (Go `busFactorThreshold = 0.5`).
pub const BUS_FACTOR_THRESHOLD: f64 = 0.5;

/// Risk threshold: CRITICAL (Go `ThresholdCritical = 90.0`).
pub const THRESHOLD_CRITICAL: f64 = 90.0;
/// Risk threshold: HIGH (Go `ThresholdHigh = 80.0`).
pub const THRESHOLD_HIGH: f64 = 80.0;
/// Risk threshold: MEDIUM (Go `ThresholdMedium = 60.0`).
pub const THRESHOLD_MEDIUM: f64 = 60.0;

/// Fallback "recent" fraction of the analysis period (Go `ActiveThresholdRatio`).
pub const ACTIVE_THRESHOLD_RATIO: f64 = 0.7;
/// Time-based active-developer window in days (Go `DefaultActiveDays = 90`).
pub const DEFAULT_ACTIVE_DAYS: i64 = 90;
/// Hours per day used in tick math (Go `defaultTickHours = 24`).
pub const DEFAULT_TICK_HOURS: i64 = 24;

/// One hour expressed in nanoseconds, matching Go's `time.Hour`.
const NANOS_PER_HOUR: i64 = 3_600_000_000_000;

/// Configurable thresholds for devs metric computation (`MetricOptions`).
///
/// Zero-valued fields mean "use the package-level default" exactly as Go does
/// (the analyzer stores configured overrides as zero when unset).
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
    /// Mirrors `DefaultMetricOptions()`.
    fn default() -> Self {
        MetricOptions {
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

/// Parsed input for metric computation (`TickData`), already aggregated to
/// per-tick / per-developer granularity.
#[derive(Debug, Clone, Default)]
pub struct TickData {
    /// `tick → (dev id → DevTick)`.
    pub ticks: BTreeMap<i64, BTreeMap<i64, DevTick>>,
    /// Reversed people dict (`dev id` index → `"Name <email>"`).
    pub names: Vec<String>,
    /// Tick size in nanoseconds (`time.Duration`).
    pub tick_size: i64,
    /// `tick → (start_time, end_time)` already formatted as Go
    /// `time.RFC3339` strings (empty string == Go zero time, i.e. omit).
    ///
    /// Port of `TickData.TickBounds map[int]analyze.TickBounds` combined with
    /// `TickBounds.FormatStartTime/FormatEndTime` (the metrics layer only ever
    /// reads the formatted strings). When a tick has no entry, activity/churn
    /// emit no `start_time`/`end_time` (their `omitempty` JSON tags), exactly
    /// as Go does when `input.TickBounds[tick]` is absent.
    pub tick_bounds: BTreeMap<i64, TickBounds>,
}

/// Pre-formatted time boundaries of a single tick (port of
/// `analyze.TickBounds` *after* `FormatStartTime`/`FormatEndTime`).
///
/// Each field holds the Go `time.RFC3339` rendering of the corresponding
/// `time.Time`, or the empty string when that time was the Go zero value
/// (`FormatStartTime`/`FormatEndTime` return `""` for a zero time, which the
/// `start_time,omitempty` / `end_time,omitempty` JSON tags then drop).
#[derive(Debug, Clone, Default)]
pub struct TickBounds {
    /// Formatted start time (`FormatStartTime`), or `""` for a zero time.
    pub start_time: String,
    /// Formatted end time (`FormatEndTime`), or `""` for a zero time.
    pub end_time: String,
}

/// Converts a developer id to the deterministic byte slice HLL hashes
/// (Go `devIDBytes` = `strconv.AppendInt(nil, id, 10)` = decimal ASCII).
#[must_use]
pub fn dev_id_bytes(id: i64) -> Vec<u8> {
    id.to_string().into_bytes()
}

/// Resolves the absolute value of a float (helper used in accuracy tests).
#[must_use]
fn abs64(x: f64) -> f64 {
    if x < 0.0 {
        -x
    } else {
        x
    }
}

/// Computes per-developer statistics (`DevelopersMetric.Compute`).
///
/// Aggregates across all ticks, finalizes net lines + sorted languages, then
/// sorts developers by commit count descending. Go's `sort.Slice` is not stable;
/// to keep deterministic byte output we break ties by developer id ascending,
/// which matches the Go reference for the disjoint-key inputs exercised in the
/// golden manifest (see DESIGN §6 / todos).
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

/// Converts an internal per-language map into a sorted `Vec`
/// (`DeveloperData.finalizeLanguages`). Empty language name → `"Other"`.
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

/// Computes per-language statistics (`LanguagesMetric.Compute`).
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

            let ld = lang_map.entry(lang.clone()).or_insert_with(|| LanguageData {
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

/// Input for bus-factor computation (`BusFactorInput`).
pub struct BusFactorInput<'a> {
    /// Per-language data.
    pub languages: &'a [LanguageData],
    /// Reversed people dict.
    pub names: &'a [String],
}

/// Computes bus-factor risk per language (`BusFactorMetric.ComputeWithOptions`).
///
/// Sorted by risk priority ascending (CRITICAL first). Ties broken by language
/// name ascending for determinism.
#[must_use]
pub fn compute_bus_factor(input: &BusFactorInput, opts: &MetricOptions) -> Vec<BusFactorData> {
    let mut result: Vec<BusFactorData> = Vec::with_capacity(input.languages.len());

    for ld in input.languages {
        if ld.total_contribution == 0 {
            continue;
        }

        // (id, lines) sorted descending by lines, tie-break id ascending.
        let mut contribs: Vec<(i64, i64)> =
            ld.contributors.iter().map(|(&id, &lines)| (id, lines)).collect();
        contribs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

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

    result.sort_by(|a, b| {
        let pa = cf_metrics::risk_priority(&cf_metrics::RiskLevel::from(a.risk_level.as_str()));
        let pb = cf_metrics::risk_priority(&cf_metrics::RiskLevel::from(b.risk_level.as_str()));
        pa.cmp(&pb).then(a.language.cmp(&b.language))
    });
    result
}

/// Smallest number of (descending-sorted) contributors covering at least
/// `threshold` of `total` (`computeBusFactorFromSortedWithThreshold`).
#[must_use]
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

/// Computes per-tick activity time series (`ActivityMetric.Compute`).
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
            ad.start_time = bounds.start_time.clone();
            ad.end_time = bounds.end_time.clone();
        }

        result.push(ad);
    }

    result
}

/// Computes per-tick churn time series (`ChurnMetric.Compute`).
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
            cd.start_time = bounds.start_time.clone();
            cd.end_time = bounds.end_time.clone();
        }

        result.push(cd);
    }

    result
}

/// Input for aggregate computation (`AggregateInput`).
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

/// Computes aggregate summary statistics (`AggregateMetric.ComputeWithOptions`).
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
        // Sorted keys → last is max tick (Go `mapx.SortedKeys`).
        let max_tick = *input.ticks.keys().next_back().unwrap_or(&0);
        agg.analysis_period_ticks = max_tick;

        let recent_threshold = compute_active_threshold(max_tick, input.tick_size, opts);
        let active_sketch = build_active_dev_sketch(input.ticks, recent_threshold, opts.hll_precision);

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

/// Builds an HLL sketch over all developer ids
/// (`buildTotalDevSketchWithPrecision`). Returns `None` for empty input,
/// matching Go (no sketch → estimate stays 0).
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

/// Builds an HLL sketch over active developer ids
/// (`buildActiveDevSketchWithPrecision`). Unlike the total sketch this is built
/// even when no tick qualifies (Go returns a non-nil empty sketch → estimate 0).
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

/// Returns the active-window tick threshold (`computeActiveThresholdWithOptions`).
#[must_use]
pub fn compute_active_threshold(max_tick: i64, tick_size: i64, opts: &MetricOptions) -> i64 {
    if tick_size > 0 {
        let active_days = opts.default_active_days;
        // time.Duration(activeDays) * defaultTickHours * time.Hour
        let active_duration = active_days * DEFAULT_TICK_HOURS * NANOS_PER_HOUR;
        let ticks_for_active = active_duration / tick_size; // integer division (Go int())
        let threshold = max_tick - ticks_for_active;
        if threshold < 0 {
            return 0;
        }
        return threshold;
    }

    // Ratio fallback: int(float64(maxTick) * ratio) truncates toward zero.
    (max_tick as f64 * opts.active_threshold_ratio) as i64
}

/// Computes the project-wide bus factor (`computeProjectBusFactorWithThreshold`).
#[must_use]
pub fn compute_project_bus_factor(developers: &[DeveloperData], threshold: f64) -> i64 {
    if developers.is_empty() {
        return 0;
    }

    let mut contribs: Vec<i64> = developers.iter().map(|d| d.added + d.removed).collect();
    let total: i64 = contribs.iter().sum();
    // Descending sort (Go sorts contribution amounts only).
    contribs.sort_by(|a, b| b.cmp(a));

    compute_bus_factor_from_sorted(&contribs, total, threshold)
}

/// Resolves an HLL estimate's relative error against an exact count (test util).
#[must_use]
pub fn relative_error(estimated: u64, exact: u64) -> f64 {
    abs64(estimated as f64 - exact as f64) / exact as f64
}

/// Resolves a developer's display name and email (`devNameAndEmail`).
///
/// `AuthorMissing` → `(AuthorMissingName, "")`; in-range id → split identity;
/// out-of-range id → `("dev_<id>", "")`.
#[must_use]
pub fn dev_name_and_email(id: i64, names: &[String]) -> (String, String) {
    if id == i64::from(cf_identity::AUTHOR_MISSING) {
        return (cf_identity::AUTHOR_MISSING_NAME.to_string(), String::new());
    }

    if id >= 0 && (id as usize) < names.len() {
        return cf_identity::split_identity(&names[id as usize]);
    }

    (format!("dev_{id}"), String::new())
}

/// Runs all devs metrics in dependency order (`ComputeAllMetricsWithOptions`).
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
