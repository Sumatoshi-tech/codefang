//! `history/burndown` plot sections.
//! (`GenerateStoreSections` → `buildStoreSections` over the `chart_data` and
//! `metrics` store kinds).
//!
//! The store reader reconstructs the chart from `ChartData` (dense global
//! history + sampling/granularity/tick-size/end-time/project-name) — the year
//! aggregation (`aggregateByYear`) does calendar arithmetic on `time.Unix(0,
//! EndTime)` in the process TZ. The oracle pins `TZ=UTC`, and these civil-time
//! helpers implement exactly the UTC calendar (days-from-civil), so the year
//! bands match the reference implementation's under the pinned environment.

use cf_analyzer_burndown::DenseHistory;
use cf_gojson::GoValue;
use cf_plotpage::components::BadgeColor;
use cf_plotpage::echarts::{Chart, ChartKind, Grid, Legend, LineData, TextStyle};
use cf_plotpage::{ChartOpts, Hint, Section};

use crate::handlers::plot_sections::history_shared::{format_int64, GridStats};

/// Reference burndown plot-section constants.
const HOURS_PER_DAY: i64 = 24;
const DAYS_PER_MONTH: i64 = 30;
const MONTHS_PER_YEAR: i64 = 12;
const MIN_YEARS_FOR_AGGREGATION: usize = 2;
const CHART_HEIGHT: &str = "600px";
const AREA_OPACITY: f64 = 0.5;
const ROUNDING_OFFSET: f64 = 0.5;
const MIN_INTERPOLATION_LEN: usize = 2;
const PLOT_MAX_STATS_COLUMNS: usize = 4;
const NS_PER_HOUR: i64 = 3_600_000_000_000;

/// The `chart_data` store kind.
pub struct ChartData {
    /// Dense global history (negatives clamped to zero, the reference `buildChartData`).
    pub global_history: DenseHistory,
    /// Ticks per sample.
    pub sampling: i64,
    /// Ticks per band.
    pub granularity: i64,
    /// Tick size, nanoseconds.
    pub tick_size_ns: i64,
    /// End time, Unix nanoseconds.
    pub end_time_ns: i64,
    /// Project display name (empty → `"project"`).
    pub project_name: String,
}

/// The reference `GenerateStoreSections` → `buildStoreSections`: the summary stats grid,
/// plus the burndown chart when the dense history is non-empty.
pub fn sections(
    chart_data: &ChartData,
    metrics: &cf_analyzer_burndown::ComputedMetrics,
) -> Vec<Section> {
    let mut result = vec![build_summary_section(metrics)];

    if !chart_data.global_history.is_empty() {
        result.push(Section {
            title: "Code Burndown Chart".to_string(),
            subtitle:
                "Shows how code written at different times survives over the project's lifetime."
                    .to_string(),
            chart: Some(Box::new(build_chart(chart_data))),
            hint: Hint {
                title: "How to interpret:".to_string(),
                items: vec![
                    "Stacked layers = code written in different time periods".to_string(),
                    "Bottom layers = oldest code still surviving".to_string(),
                    "Narrowing layers = code being deleted or rewritten".to_string(),
                    "Flat layers = stable code that rarely changes".to_string(),
                    "Rapid decrease in recent layers indicates instability".to_string(),
                ],
            },
        });
    }

    result
}

/// The reference `buildStoreSummarySection`.
fn build_summary_section(metrics: &cf_analyzer_burndown::ComputedMetrics) -> Section {
    let agg = &metrics.aggregate;
    let survival_pct = format!("{:.1}%", agg.overall_survival_rate * 100.0);
    let survival_color = survival_badge_color(agg.overall_survival_rate);

    let mut grid = GridStats::new(PLOT_MAX_STATS_COLUMNS)
        .stat("Current Lines", &format_int64(agg.total_current_lines))
        .stat("Peak Lines", &format_int64(agg.total_peak_lines))
        .stat_with_trend("Survival Rate", &survival_pct, &survival_pct, survival_color)
        .stat("Analysis Period", &format!("{} days", agg.analysis_period_days));

    if agg.tracked_developers > 0 {
        grid = grid.stat("Developers", &agg.tracked_developers.to_string());
    }
    if agg.tracked_files > 0 {
        grid = grid.stat("Tracked Files", &agg.tracked_files.to_string());
    }

    Section {
        title: "Burndown Summary".to_string(),
        subtitle: "Aggregate statistics from code burndown analysis.".to_string(),
        chart: Some(Box::new(grid.into_grid())),
        hint: Hint::default(),
    }
}

/// The reference `survivalBadgeColor`.
fn survival_badge_color(rate: f64) -> BadgeColor {
    if rate >= 0.7 {
        BadgeColor::Success
    } else if rate >= 0.5 {
        BadgeColor::Warning
    } else {
        BadgeColor::Error
    }
}

/// The reference `buildChartFromStoreData` → `createLineChart` + `addSeries`.
fn build_chart(data: &ChartData) -> Chart {
    let project_name =
        if data.project_name.is_empty() { "project" } else { &data.project_name };

    let co = ChartOpts::default_dark();
    let x_labels = build_x_labels(data);

    let max_lines = compute_max_lines(&data.global_history);
    let title = format!(
        "{} x {} (granularity {}, sampling {})",
        project_name, max_lines, data.granularity, data.sampling
    );

    let mut line = Chart::new(ChartKind::Line);
    let (w, h, bg, theme) = co.init("100%", CHART_HEIGHT);
    line.set_init(&w, &h, &bg, &theme);
    line.colors = Some(color_palette());
    line.title = co.title(&title, "");
    line.tooltip = co.tooltip("axis");
    line.legend = Legend {
        show: Some(true),
        type_: "scroll".to_string(),
        top: "5%".to_string(),
        left: "5%".to_string(),
        text_style: Some(TextStyle {
            color: co.text_muted_color().to_string(),
            ..TextStyle::default()
        }),
        ..Legend::default()
    };
    line.grid = vec![Grid {
        top: "20%".to_string(),
        bottom: "15%".to_string(),
        left: "10%".to_string(),
        right: "5%".to_string(),
        contain_label: Some(true),
    }];
    line.data_zoom = co.data_zoom();
    line.x_axis = co.x_axis("Time (days)");
    line.y_axis = co.y_axis("Lines of Code");
    line.set_x_axis_labels(&x_labels);

    add_series(&mut line, data);

    line
}

/// The reference `getColorPalette`.
fn color_palette() -> Vec<String> {
    [
        "#8B4513", "#2f4554", "#9370DB", "#808080", "#DAA520", "#90EE90", "#FFB6C1", "#c23531",
        "#37a2da", "#6B8E23", "#4B0082", "#ffdb5c", "#749f83", "#fb7293", "#e5323e",
    ]
    .iter()
    .map(|c| (*c).to_string())
    .collect()
}

/// The reference `buildXLabels` (interpolationFactor == 1).
fn build_x_labels(data: &ChartData) -> Vec<String> {
    let n = data.global_history.len() as i64;
    let points = ((n - 1) + 1).max(1);
    (0..points)
        .map(|i| {
            let ticks = i * data.sampling;
            let days = (ticks * data.tick_size_ns) as f64 / NS_PER_HOUR as f64 / HOURS_PER_DAY as f64;
            format!("{}d", days as i64)
        })
        .collect()
}

/// The reference `computeMaxLines`.
fn compute_max_lines(history: &DenseHistory) -> i64 {
    let mut max_lines = 0i64;
    for sample in history {
        let sum: i64 = sample.iter().filter(|v| **v > 0).sum();
        if sum > max_lines {
            max_lines = sum;
        }
    }
    max_lines
}

/// The reference `addSeries`: year aggregation when the span allows it, else raw bands.
fn add_series(line: &mut Chart, data: &ChartData) {
    if let Some((years, year_data)) = aggregate_by_year(data) {
        for (year, values) in years.iter().zip(year_data) {
            push_band_series(line, &year.to_string(), &interpolate(&values));
        }
        return;
    }

    let num_bands = data.global_history[0].len();
    for rev in (0..num_bands).rev() {
        let raw: Vec<f64> = data
            .global_history
            .iter()
            .map(|sample| {
                let v = sample.get(rev).copied().unwrap_or(0);
                if v > 0 { v as f64 } else { 0.0 }
            })
            .collect();
        let label = band_label(rev as i64, data);
        push_band_series(line, &label, &interpolate(&raw));
    }
}

/// One stacked smooth area series.
fn push_band_series(line: &mut Chart, name: &str, values: &[i64]) {
    let data = GoValue::Array(
        values
            .iter()
            .map(|v| {
                LineData {
                    value: Some(GoValue::Int(*v)),
                    ..LineData::default()
                }
                .value()
            })
            .collect(),
    );
    let series = line.add_series(name, data);
    series.stack = "total".to_string();
    series.smooth = Some(true);
    series.show_symbol = Some(false);
    series.area_style = Some(cf_plotpage::echarts::AreaStyle {
        opacity: Some(AREA_OPACITY),
        ..cf_plotpage::echarts::AreaStyle::default()
    });
}

/// The reference `interpolate` (interpolationFactor == 1: every point lands on a sample,
/// so the result is `int64(v + 0.5)` per sample).
fn interpolate(values: &[f64]) -> Vec<i64> {
    if values.len() < MIN_INTERPOLATION_LEN {
        return values.iter().map(|v| (*v + ROUNDING_OFFSET) as i64).collect();
    }
    let n = values.len();
    (0..n)
        .map(|i| {
            // interpolatePoint with factor 1: subIdx == i.
            let val = if i >= n - 1 { values[n - 1] } else { values[i] };
            let val = if val < 0.0 { 0.0 } else { val };
            (val + ROUNDING_OFFSET) as i64
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Civil-time helpers (UTC calendar over Unix nanoseconds).
// ---------------------------------------------------------------------------

/// Days from civil epoch (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Civil date from days since the Unix epoch.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `time.Unix(0, ns).Year()` under UTC.
fn year_of_ns(ns: i64) -> i64 {
    let days = ns.div_euclid(86_400_000_000_000);
    civil_from_days(days).0
}

/// `time.Date(year, 1, 1, 0, 0, 0, 0, time.UTC).UnixNano()`.
fn year_start_ns(year: i64) -> i64 {
    days_from_civil(year, 1, 1) * 86_400_000_000_000
}

/// The reference `aggregateByYear`: returns `(years, per-year sample data)` or `None`
/// when aggregation does not apply.
fn aggregate_by_year(data: &ChartData) -> Option<(Vec<i64>, Vec<Vec<f64>>)> {
    // canAggregateByYear.
    if data.end_time_ns == 0
        || data.global_history.is_empty()
        || data.global_history[0].is_empty()
    {
        return None;
    }

    let num_bands = data.global_history[0].len() as i64;
    let num_samples = data.global_history.len() as i64;

    // computeStartTime.
    let last_tick = (num_samples - 1) * data.sampling;
    let start_ns = data.end_time_ns - last_tick * data.tick_size_ns;

    // computeBandWeights.
    let band_dur_ns = data.granularity * data.tick_size_ns;
    let mut band_weights: Vec<Vec<(i64, f64)>> = Vec::with_capacity(num_bands as usize);
    let mut year_set: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();
    for band_idx in 0..num_bands {
        let band_start = start_ns + band_idx * data.granularity * data.tick_size_ns;
        let band_end = band_start + band_dur_ns;
        let mut weights: Vec<(i64, f64)> = Vec::new();
        for year in year_of_ns(band_start)..=year_of_ns(band_end) {
            let year_start = year_start_ns(year);
            let year_end = year_start_ns(year + 1);
            let start = band_start.max(year_start);
            let end = band_end.min(year_end);
            if end > start {
                let w = (end - start) as f64 / band_dur_ns as f64;
                if w > 0.0 {
                    weights.push((year, w));
                    year_set.insert(year);
                }
            }
        }
        band_weights.push(weights);
    }

    let years: Vec<i64> = year_set.into_iter().collect();
    if years.len() < MIN_YEARS_FOR_AGGREGATION {
        return None;
    }

    // computeYearData.
    let year_idx: std::collections::HashMap<i64, usize> =
        years.iter().enumerate().map(|(i, y)| (*y, i)).collect();
    let mut year_data: Vec<Vec<f64>> = vec![vec![0.0; num_samples as usize]; years.len()];
    for (sample_idx, sample) in data.global_history.iter().enumerate() {
        for (band_idx, val) in sample.iter().enumerate() {
            if *val <= 0 {
                continue;
            }
            for (year, w) in &band_weights[band_idx] {
                year_data[year_idx[year]][sample_idx] += *val as f64 * w;
            }
        }
    }

    Some((years, year_data))
}

/// The reference `bandLabel`.
fn band_label(band_idx: i64, data: &ChartData) -> String {
    let upper_ticks = (band_idx + 1) * data.granularity;
    let age_ns = upper_ticks * data.tick_size_ns;
    let age_days = age_ns as f64 / NS_PER_HOUR as f64 / HOURS_PER_DAY as f64;
    let age_months = ((age_days as i64) / DAYS_PER_MONTH).max(1);

    let max_band_idx = data.global_history[0].len() as i64 - 1;
    let max_days = ((max_band_idx + 1) * data.granularity * data.tick_size_ns) as f64
        / NS_PER_HOUR as f64
        / HOURS_PER_DAY as f64;
    let max_months = (max_days as i64) / DAYS_PER_MONTH;

    if max_months >= MONTHS_PER_YEAR && data.end_time_ns != 0 {
        return year_of_ns(data.end_time_ns - age_ns).to_string();
    }

    if max_months >= MONTHS_PER_YEAR {
        let y = age_months / MONTHS_PER_YEAR;
        if y > 0 {
            return format!("{y}y");
        }
    }

    format!("{age_months}mo")
}
