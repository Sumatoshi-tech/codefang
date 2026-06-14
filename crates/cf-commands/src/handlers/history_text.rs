//! Per-leaf `--format text` bodies for the six history leaves the reference implementation wires with a
//! `SerializeTextFn` hook — sentiment, shotness, burndown, couples, devs,
//! file-history. Reproduces each leaf's reference text renderer, drawing on
//! the shared [`cf_terminal`] helpers.
//!
//! Each function returns ONLY the bytes the leaf's reference `Serialize(result,
//! "text", w)` call writes; the `codefang (v2):` header and the `"<Name>:\n"`
//! section line are emitted by [`super::history_formats::history_text`].
//!
//! # Width / color / padding semantics
//!
//! * Colors are unconditional: the reference implementation never checks isatty — `terminal.NewConfig`
//!   only honors the `NO_COLOR` env var — so the bytes contain ANSI escapes
//!   even when piped. Width comes from `COLUMNS` (default 80). Both are read
//!   through [`cf_terminal::Config::new`].
//! * `terminal.PadRight` / `TruncateWithEllipsis` / `DrawHeader` measure
//!   string length in BYTES, while the reference `fmt` width specifiers
//!   (`%-*s`, `%6s`, …) pad to a minimum number of RUNES
//!   (`utf8.RuneCountInString`) — e.g. `"máximo cuadros"` (15 bytes, 14 runes)
//!   gets 4 padding spaces under `%-18s` but only 3 under `PadRight(_, 18)`.
//!   Rust `format!` width counts `char`s, which equals the reference implementation's rune count, so
//!   `fmt`-style padding maps to `format!("{:<w$}")` and the byte-length
//!   helpers stay in [`cf_terminal`].
//! * reference float verbs (`%.1f`, `%5.1f`, `%3.0f`, `%.2f`) and Rust `{:.1}` /
//!   `{:5.1}` / `{:3.0}` / `{:.2}` both emit the correctly-rounded fixed
//!   decimal of the binary value with ties-to-even — identical bytes.
//!   The reference `float32` operands are widened exactly (`f64::from`).

use std::fmt::Write as _;

use cf_terminal::{
    color_for_score, draw_header, draw_percent_bar, draw_progress_bar, draw_separator,
    format_score_bar, truncate_with_ellipsis, Color, Config,
};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// The reference `formatUint`: groups of three
/// digits joined by `,` via tail recursion (`formatUint(n/1000) + "," + %03d`).
fn format_uint_thousands(n: u64) -> String {
    if n < 1000 {
        return n.to_string();
    }
    format!("{},{:03}", format_uint_thousands(n / 1000), n % 1000)
}

/// The reference `formatInt` / `formatInt64`:
/// thousands separators, `-` prefix for negatives.
fn format_int_thousands(n: i64) -> String {
    if n < 0 {
        return format!("-{}", format_uint_thousands(n.unsigned_abs()));
    }
    format_uint_thousands(n as u64)
}

/// The reference `filepath.Base` for the slash-separated repo paths shotness emits:
/// empty → `"."`, trailing slashes stripped, all-slash → `"/"`, else the last
/// path element.
fn go_filepath_base(path: &str) -> &str {
    if path.is_empty() {
        return ".";
    }
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return "/";
    }
    match trimmed.rfind('/') {
        Some(i) => &trimmed[i + 1..],
        None => trimmed,
    }
}

// ---------------------------------------------------------------------------
// sentiment
// ---------------------------------------------------------------------------

/// `SentimentPositiveThreshold`.
const SENTIMENT_POSITIVE_THRESHOLD: f64 = 0.6;
/// `SentimentNegativeThreshold`.
const SENTIMENT_NEGATIVE_THRESHOLD: f64 = 0.4;
/// `termWidth` — sentiment renders at a FIXED width of 60,
/// ignoring `Config.Width`.
const SENTIMENT_TERM_WIDTH: i64 = 60;
/// `terminalBarWidth`.
const SENTIMENT_BAR_WIDTH: i64 = 20;
/// `terminalLabelWidth + labelPaddingExtra`.
const SENTIMENT_DIST_LABEL_WIDTH: i64 = 18 + 4;
/// `maxRiskPeriodsToShow`.
const SENTIMENT_MAX_RISK: usize = 5;
/// `sparklineLabelGap`.
const SENTIMENT_SPARK_GAP: usize = 14;
/// `sparklineChars`.
const SPARKLINE_CHARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// `sentimentColor`.
fn sentiment_color(score: f64) -> Color {
    if score >= SENTIMENT_POSITIVE_THRESHOLD {
        Color::Green
    } else if score <= SENTIMENT_NEGATIVE_THRESHOLD {
        Color::Red
    } else {
        Color::Yellow
    }
}

/// `sentimentLabel`.
fn sentiment_label(score: f64) -> &'static str {
    if score >= SENTIMENT_POSITIVE_THRESHOLD {
        "😊"
    } else if score <= SENTIMENT_NEGATIVE_THRESHOLD {
        "😟"
    } else {
        "😐"
    }
}

/// The `history/sentiment` text body — the reference `generateText` (sentiment/
/// reference dispatch) → `RenderTerminal`.
#[must_use]
pub fn sentiment_text(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let metrics = super::history::sentiment_metrics(sub)?;
    let cfg = Config::new();
    let mut s = String::new();

    s.push_str(&draw_header("SENTIMENT ANALYSIS", "💬", SENTIMENT_TERM_WIDTH));
    s.push_str("\n\n");

    // renderSummarySection. The Blue colorize wraps the
    // trailing newline, so the reset lands at the start of the next line.
    s.push_str(&cfg.colorize("  Summary\n", Color::Blue));
    s.push_str(&draw_separator(SENTIMENT_TERM_WIDTH));
    s.push('\n');
    let avg = f64::from(metrics.aggregate.average_sentiment);
    let _ = writeln!(
        s,
        "  Average Sentiment: {} {}",
        cfg.colorize(&format_score_bar(avg, SENTIMENT_BAR_WIDTH), sentiment_color(avg)),
        sentiment_label(avg)
    );
    let _ = writeln!(s, "  Total Ticks:       {}", metrics.aggregate.total_ticks);
    let _ = writeln!(s, "  Total Comments:    {}", metrics.aggregate.total_comments);
    let _ = writeln!(s, "  Total Commits:     {}", metrics.aggregate.total_commits);
    s.push('\n');

    // renderDistributionSection.
    if metrics.aggregate.total_ticks != 0 {
        s.push_str(&cfg.colorize("  Distribution\n", Color::Blue));
        s.push_str(&draw_separator(SENTIMENT_TERM_WIDTH));
        s.push('\n');
        let total = metrics.aggregate.total_ticks as f64;
        let items: [(&str, i64, Color, &str); 3] = [
            ("Positive (≥0.6)", metrics.aggregate.positive_ticks, Color::Green, "😊"),
            ("Neutral", metrics.aggregate.neutral_ticks, Color::Yellow, "😐"),
            ("Negative (≤0.4)", metrics.aggregate.negative_ticks, Color::Red, "😟"),
        ];
        for (label, count, color, emoji) in items {
            let pct = count as f64 / total;
            let bar = draw_percent_bar(
                &format!("  {emoji} {label}"),
                pct,
                count,
                SENTIMENT_DIST_LABEL_WIDTH,
                SENTIMENT_BAR_WIDTH,
            );
            s.push_str(&cfg.colorize(&bar, color));
            s.push('\n');
        }
        s.push('\n');
    }

    // renderTrendSection.
    if !metrics.trend.trend_direction.is_empty() {
        s.push_str(&cfg.colorize("  Trend\n", Color::Blue));
        s.push_str(&draw_separator(SENTIMENT_TERM_WIDTH));
        s.push('\n');
        let (arrow, color) = match metrics.trend.trend_direction.as_str() {
            "improving" => ("↗", Color::Green),
            "declining" => ("↘", Color::Red),
            _ => ("→", Color::Yellow),
        };
        let _ = writeln!(
            s,
            "  Direction: {} {}",
            cfg.colorize(arrow, color),
            cfg.colorize(&metrics.trend.trend_direction, color)
        );
        let _ = writeln!(
            s,
            "  Start (tick {}): {:.2}  →  End (tick {}): {:.2}",
            metrics.trend.start_tick,
            f64::from(metrics.trend.start_sentiment),
            metrics.trend.end_tick,
            f64::from(metrics.trend.end_sentiment)
        );
        let sign = if metrics.trend.change_percent < 0.0 { "" } else { "+" };
        let _ = writeln!(s, "  Change: {}{:.1}%", sign, metrics.trend.change_percent);
        s.push('\n');
    }

    // renderSparklineSection.
    if !metrics.time_series.is_empty() {
        s.push_str(&cfg.colorize("  Sentiment Timeline\n", Color::Blue));
        s.push_str(&draw_separator(SENTIMENT_TERM_WIDTH));
        s.push('\n');
        // buildSparkline: idx = max(int(min(score*8, 7)), 0).
        let mut sparkline = String::new();
        for ts in &metrics.time_series {
            let score = f64::from(ts.sentiment);
            let levels = SPARKLINE_CHARS.len() as f64;
            let idx = (((score * levels).min(levels - 1.0)) as i64).max(0) as usize;
            sparkline.push_str(
                &cfg.colorize(&SPARKLINE_CHARS[idx].to_string(), sentiment_color(score)),
            );
        }
        let _ = writeln!(s, "  {sparkline}");
        let _ = writeln!(
            s,
            "  {}{}{}",
            cfg.colorize("neg", Color::Red),
            " ".repeat(SENTIMENT_SPARK_GAP),
            cfg.colorize("pos", Color::Green)
        );
        s.push('\n');
    }

    // renderRiskSection. The colorize wraps the newline, so
    // each reset code starts the FOLLOWING line.
    if !metrics.low_sentiment_periods.is_empty() {
        s.push_str(&cfg.colorize("  Risk Periods\n", Color::Blue));
        s.push_str(&draw_separator(SENTIMENT_TERM_WIDTH));
        s.push('\n');
        let shown = metrics.low_sentiment_periods.len().min(SENTIMENT_MAX_RISK);
        for period in &metrics.low_sentiment_periods[..shown] {
            let (emoji, color) = if period.risk_level == "HIGH" {
                ("🔴", Color::Red)
            } else {
                ("🟡", Color::Yellow)
            };
            s.push_str(&cfg.colorize(
                &format!(
                    "  {} Tick {}: {:.2} ({})\n",
                    emoji,
                    period.tick,
                    f64::from(period.sentiment),
                    period.risk_level
                ),
                color,
            ));
        }
        if metrics.low_sentiment_periods.len() > SENTIMENT_MAX_RISK {
            let remaining = metrics.low_sentiment_periods.len() - SENTIMENT_MAX_RISK;
            s.push_str(&cfg.colorize(&format!("  ... and {remaining} more\n"), Color::Gray));
        }
        s.push('\n');
    }

    Some(s.into_bytes())
}

// ---------------------------------------------------------------------------
// shotness
// ---------------------------------------------------------------------------

/// `formatNodeLabel`: `"name (base(file))"`.
fn shotness_node_label(name: &str, file: &str) -> String {
    if file.is_empty() {
        return name.to_string();
    }
    format!("{} ({})", name, go_filepath_base(file))
}

/// `riskLevelColor`.
fn shotness_risk_color(level: &str) -> Color {
    match level {
        "HIGH" => Color::Red,
        "MEDIUM" => Color::Yellow,
        _ => Color::Green,
    }
}

/// The `history/shotness` text body — the reference `generateText`.
#[must_use]
pub fn shotness_text(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    const BAR_WIDTH: i64 = 20; // textBarWidth
    const LABEL_WIDTH: i64 = 24; // textLabelWidth
    const HALF_LABEL: i64 = 12; // textHalfLabel
    const MAX_ROWS: usize = 10; // textMaxHot / textMaxCouplings / textMaxHotspots
    const SUMMARY_LABEL_WIDTH: usize = 22; // summaryLabelWidth

    let metrics = super::shotness_run::shotness_run_metrics(sub)?;
    let cfg = Config::new();
    let w = cfg.width;
    let mut s = String::new();

    let agg = &metrics.aggregate;
    s.push_str(&draw_header("Shotness Analysis", &format!("{} nodes", agg.total_nodes), w));
    s.push_str("\n\n");

    // writeSummarySection.
    let _ = writeln!(s, "  {}", cfg.colorize("Summary", Color::Blue));
    let _ = writeln!(s, "  {}", draw_separator(w - 4));
    let _ = writeln!(s, "  {:<SUMMARY_LABEL_WIDTH$} {}", "Total Nodes", agg.total_nodes);
    let _ = writeln!(s, "  {:<SUMMARY_LABEL_WIDTH$} {}", "Total Changes", agg.total_changes);
    let _ = writeln!(
        s,
        "  {:<SUMMARY_LABEL_WIDTH$} {:.1}",
        "Avg Changes/Node", agg.avg_changes_per_node
    );
    let _ = writeln!(s, "  {:<SUMMARY_LABEL_WIDTH$} {}", "Total Couplings", agg.total_couplings);
    let strength_color = color_for_score(1.0 - agg.avg_coupling_strength);
    let _ = writeln!(
        s,
        "  {:<SUMMARY_LABEL_WIDTH$} {}",
        "Avg Coupling Strength",
        cfg.colorize(&format!("{:.0}%", agg.avg_coupling_strength * 100.0), strength_color)
    );
    let hot_color = if agg.hot_nodes > 0 { Color::Red } else { Color::None };
    let _ = writeln!(
        s,
        "  {:<SUMMARY_LABEL_WIDTH$} {}",
        "Hot Nodes",
        cfg.colorize(&agg.hot_nodes.to_string(), hot_color)
    );

    // writeHottestFunctions.
    if !metrics.node_hotness.is_empty() {
        s.push('\n');
        let _ = writeln!(s, "  {}", cfg.colorize("Hottest Functions", Color::Blue));
        let _ = writeln!(s, "  {}", draw_separator(w - 4));
        let shown = metrics.node_hotness.len().min(MAX_ROWS);
        for n in &metrics.node_hotness[..shown] {
            let label =
                truncate_with_ellipsis(&shotness_node_label(&n.name, &n.file), LABEL_WIDTH);
            let bar = draw_progress_bar(n.hotness_score, BAR_WIDTH);
            // hotnessColor: inverted score.
            let score_color = color_for_score(1.0 - n.hotness_score);
            let _ = writeln!(
                s,
                "  {:<width$} [{}] {}  ({} changes)",
                label,
                bar,
                cfg.colorize(&format!("{:.1}", n.hotness_score), score_color),
                n.change_count,
                width = LABEL_WIDTH as usize
            );
        }
        if metrics.node_hotness.len() > MAX_ROWS {
            let _ = writeln!(
                s,
                "  {}",
                cfg.colorize(
                    &format!("  ... and {} more", metrics.node_hotness.len() - MAX_ROWS),
                    Color::Gray
                )
            );
        }
    }

    // writeRiskNodes.
    if !metrics.hotspot_nodes.is_empty() {
        s.push('\n');
        let _ = writeln!(s, "  {}", cfg.colorize("Risk Assessment", Color::Blue));
        let _ = writeln!(s, "  {}", draw_separator(w - 4));
        let shown = metrics.hotspot_nodes.len().min(MAX_ROWS);
        for n in &metrics.hotspot_nodes[..shown] {
            let label =
                truncate_with_ellipsis(&shotness_node_label(&n.name, &n.file), LABEL_WIDTH);
            let _ = writeln!(
                s,
                "  {:<width$} {}  ({} changes)",
                label,
                cfg.colorize(&format!("{:<6}", n.risk_level), shotness_risk_color(&n.risk_level)),
                n.change_count,
                width = LABEL_WIDTH as usize
            );
        }
        if metrics.hotspot_nodes.len() > MAX_ROWS {
            let _ = writeln!(
                s,
                "  {}",
                cfg.colorize(
                    &format!("  ... and {} more", metrics.hotspot_nodes.len() - MAX_ROWS),
                    Color::Gray
                )
            );
        }
    }

    // writeStrongestCouplings.
    if !metrics.node_coupling.is_empty() {
        s.push('\n');
        let _ = writeln!(s, "  {}", cfg.colorize("Strongest Couplings", Color::Blue));
        let _ = writeln!(s, "  {}", draw_separator(w - 4));
        let shown = metrics.node_coupling.len().min(MAX_ROWS);
        for c in &metrics.node_coupling[..shown] {
            let left = truncate_with_ellipsis(&c.node1_name, HALF_LABEL);
            let right = truncate_with_ellipsis(&c.node2_name, HALF_LABEL);
            // couplingStrengthColor: inverted strength.
            let strength_color = color_for_score(1.0 - c.strength);
            let _ = writeln!(
                s,
                "  {:<width$} {} {:<width$} {}  ({} co-changes)",
                left,
                cfg.colorize("↔", Color::Gray),
                right,
                cfg.colorize(&format!("{:3.0}%", c.strength * 100.0), strength_color),
                c.co_changes,
                width = HALF_LABEL as usize
            );
        }
        if metrics.node_coupling.len() > MAX_ROWS {
            let _ = writeln!(
                s,
                "  {}",
                cfg.colorize(
                    &format!("  ... and {} more", metrics.node_coupling.len() - MAX_ROWS),
                    Color::Gray
                )
            );
        }
    }

    s.push('\n');
    Some(s.into_bytes())
}

// ---------------------------------------------------------------------------
// burndown
// ---------------------------------------------------------------------------

/// One display band of `buildAgeBands`.
struct AgeBand {
    label: &'static str,
    lines: i64,
}

/// `buildAgeBands`: groups dense history bands (band
/// `i` ≈ age `i+1` months) into at most five labeled age buckets, dropping
/// empty ones.
fn build_age_bands(breakdown: &[i64], num_bands: i64) -> Vec<AgeBand> {
    if num_bands == 0 {
        return Vec::new();
    }
    // (maxMonths, label); maxMonths 0 = unbounded.
    const BUCKETS: [(i64, &str); 5] = [
        (1, "< 1 month"),
        (3, "1-3 months"),
        (6, "3-6 months"),
        (12, "6-12 months"),
        (0, "> 12 months"),
    ];
    let mut lines = [0i64; BUCKETS.len()];
    for (i, &val) in breakdown.iter().enumerate() {
        if val <= 0 {
            continue;
        }
        let age_months = i as i64 + 1;
        let mut bucket_idx = BUCKETS.len() - 1;
        for (j, (max_months, _)) in BUCKETS.iter().enumerate() {
            if *max_months > 0 && age_months <= *max_months {
                bucket_idx = j;
                break;
            }
        }
        lines[bucket_idx] += val;
    }
    BUCKETS
        .iter()
        .zip(lines)
        .filter(|(_, l)| *l > 0)
        .map(|((_, label), lines)| AgeBand { label, lines })
        .collect()
}

/// The `history/burndown` text body — the reference `generateText`.
#[must_use]
pub fn burndown_text(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    const BAR_WIDTH: i64 = 20; // textBarWidth
    const LABEL_WIDTH: i64 = 14; // textLabelWidth
    const DEV_NAME_WIDTH: i64 = 16; // textDevNameWidth
    const MAX_DEVS: usize = 5; // textMaxDevs

    let metrics = super::burndown_ndjson::burndown_run_metrics(sub)?;
    let cfg = Config::new();
    let w = cfg.width;
    let mut s = String::new();

    let agg = &metrics.aggregate;
    // extractProjectName: the run path's ticksToReport
    // never sets report["ProjectName"], so the title always
    // falls back to "project".
    s.push_str(&draw_header(
        "Burndown: project",
        &format!("{}d", agg.analysis_period_days),
        w,
    ));
    s.push_str("\n\n");

    // writeSummary.
    let _ = writeln!(s, "  {}", cfg.colorize("Summary", Color::Blue));
    let _ = writeln!(s, "  {}", draw_separator(w - 4));
    let survival_pct = agg.overall_survival_rate;
    let survival_color = color_for_score(survival_pct);
    let bar = draw_progress_bar(survival_pct, BAR_WIDTH);
    let _ = writeln!(
        s,
        "  {:<18} {}",
        "Current Lines",
        format_int_thousands(agg.total_current_lines)
    );
    let _ = writeln!(s, "  {:<18} {}", "Peak Lines", format_int_thousands(agg.total_peak_lines));
    let _ = writeln!(
        s,
        "  {:<18} [{}] {}",
        "Survival Rate",
        bar,
        cfg.colorize(&format!("{:.1}%", survival_pct * 100.0), survival_color)
    );

    // writeAgeDistribution: the section title prints before the
    // empty-sample early return.
    if !metrics.global_survival.is_empty() {
        s.push('\n');
        let _ = writeln!(s, "  {}", cfg.colorize("Code Age Distribution", Color::Blue));
        let _ = writeln!(s, "  {}", draw_separator(w - 4));
        let last_sample = &metrics.global_survival[metrics.global_survival.len() - 1];
        if last_sample.total_lines != 0 {
            for band in build_age_bands(&last_sample.band_breakdown, agg.num_bands) {
                let pct = band.lines as f64 / last_sample.total_lines as f64;
                let _ = writeln!(
                    s,
                    "  {}",
                    draw_percent_bar(band.label, pct, band.lines, LABEL_WIDTH, BAR_WIDTH)
                );
            }
        }
    }

    // writeTopDevelopers. The run path never carries people
    // histories (peopleNumber == 0 in ticksToReport), so developer_survival is
    // empty and the section is skipped; the port still mirrors the reference logic
    // over the report's GoValue rows for fidelity.
    let dev_rows: Vec<(i64, String, i64, f64)> = metrics
        .developer_survival
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .filter_map(|v| {
            let m = match v {
                cf_gojson::GoValue::Map(m) => m,
                _ => return None,
            };
            let get = |k: &str| m.iter().find(|(key, _)| key.as_str() == k).map(|(_, val)| val);
            let id = match get("id") {
                Some(cf_gojson::GoValue::Int(i)) => *i,
                _ => 0,
            };
            let name = match get("name") {
                Some(cf_gojson::GoValue::Str(n)) => n.clone(),
                _ => String::new(),
            };
            let current = match get("current_lines") {
                Some(cf_gojson::GoValue::Int(i)) => *i,
                _ => 0,
            };
            let rate = match get("survival_rate") {
                Some(cf_gojson::GoValue::Float(f)) => *f,
                Some(cf_gojson::GoValue::Int(i)) => *i as f64,
                _ => 0.0,
            };
            Some((id, name, current, rate))
        })
        .collect();
    if !dev_rows.is_empty() {
        s.push('\n');
        let _ = writeln!(s, "  {}", cfg.colorize("Top Developers (by surviving lines)", Color::Blue));
        let _ = writeln!(s, "  {}", draw_separator(w - 4));
        // The reference implementation sorts a copy with sort.Slice (unstable pdqsort) by CurrentLines
        // descending; replicate the exact permutation for ties.
        let mut devs = dev_rows;
        super::go_sort::slice(&mut devs, |a, b| a.2 > b.2);
        let shown = devs.len().min(MAX_DEVS);
        for (id, name, current_lines, survival_rate) in &devs[..shown] {
            let name = if name.is_empty() { format!("dev#{id}") } else { name.clone() };
            let name = truncate_with_ellipsis(&name, DEV_NAME_WIDTH);
            let survival_color = color_for_score(*survival_rate);
            let bar = draw_progress_bar(*survival_rate, BAR_WIDTH);
            let _ = writeln!(
                s,
                "  {:<16} {:8}  [{}] {}",
                name,
                current_lines,
                bar,
                cfg.colorize(&format!("{:.1}%", survival_rate * 100.0), survival_color)
            );
        }
        if devs.len() > MAX_DEVS {
            let _ = writeln!(
                s,
                "  {}",
                cfg.colorize(&format!("  and {} more...", devs.len() - MAX_DEVS), Color::Gray)
            );
        }
    }

    s.push('\n');
    Some(s.into_bytes())
}

// ---------------------------------------------------------------------------
// couples
// ---------------------------------------------------------------------------

/// `colorForStrength`.
fn couples_strength_color(strength: f64) -> Color {
    if strength >= 0.7 {
        Color::Red
    } else if strength >= 0.4 {
        Color::Yellow
    } else {
        Color::Green
    }
}

/// `formatPct`: `%.0f%%` of `v * 100`.
fn couples_format_pct(v: f64) -> String {
    format!("{:.0}%", v * 100.0)
}

/// `writeCoupleRows`: one titled section of coupling
/// pairs `(left, right, count, strength)`.
fn couples_write_rows(
    s: &mut String,
    cfg: Config,
    title: &str,
    rows: &[(String, String, i64, f64)],
    max_rows: usize,
) {
    const NAME_WIDTH: i64 = 25; // textNameWidth

    let _ = writeln!(s, "  {}", cfg.colorize(title, Color::Blue));
    let _ = writeln!(s, "  {}", draw_separator(cfg.width - 4));
    let shown = rows.len().min(max_rows);
    for (left, right, count, strength) in &rows[..shown] {
        let left = truncate_with_ellipsis(left, NAME_WIDTH);
        let right = truncate_with_ellipsis(right, NAME_WIDTH);
        let _ = writeln!(
            s,
            "  {:<25} {} {:<25} {:4}×  {}",
            left,
            cfg.colorize("↔", Color::Gray),
            right,
            count,
            cfg.colorize(&couples_format_pct(*strength), couples_strength_color(*strength))
        );
    }
    if rows.len() > max_rows {
        let _ = writeln!(
            s,
            "  {}",
            cfg.colorize(&format!("  and {} more...", rows.len() - max_rows), Color::Gray)
        );
    }
}

/// The `history/couples` text body — the reference `generateText`.
#[must_use]
pub fn couples_text(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    const MAX_ROWS: usize = 7; // textMaxFileCouples / textMaxDevCouples / textMaxOwnership
    const OWNERSHIP_WIDTH: i64 = 25 * 2 + 3; // textOwnershipWidth

    let metrics = super::couples_run::couples_run_metrics(sub)?;
    let cfg = Config::new();
    let w = cfg.width;
    let mut s = String::new();

    let agg = &metrics.aggregate;
    s.push_str(&draw_header("Couples", &format!("{} files", agg.total_files), w));
    s.push_str("\n\n");

    // writeCouplesSummary.
    let _ = writeln!(s, "  {}", cfg.colorize("Summary", Color::Blue));
    let _ = writeln!(s, "  {}", draw_separator(w - 4));
    let _ = writeln!(
        s,
        "  {:<22} {:<12}{:<22} {}",
        "Total Files", agg.total_files, "Total Developers", agg.total_developers
    );
    let _ = writeln!(
        s,
        "  {:<22} {:<12}{:<22} {}",
        "Total Co-Changes", agg.total_co_changes, "Highly Coupled Pairs", agg.highly_coupled_pairs
    );
    let _ = writeln!(
        s,
        "  {:<22} {}",
        "Avg Coupling",
        couples_format_pct(agg.avg_coupling_strength)
    );

    // writeFileCouples.
    if !metrics.file_coupling.is_empty() {
        s.push('\n');
        let rows: Vec<(String, String, i64, f64)> = metrics
            .file_coupling
            .iter()
            .map(|c| (c.file1.clone(), c.file2.clone(), c.co_changes, c.strength))
            .collect();
        couples_write_rows(&mut s, cfg, "Top File Couples", &rows, MAX_ROWS);
    }

    // writeDevCouples.
    if !metrics.developer_coupling.is_empty() {
        s.push('\n');
        let rows: Vec<(String, String, i64, f64)> = metrics
            .developer_coupling
            .iter()
            .map(|c| (c.developer1.clone(), c.developer2.clone(), c.shared_files, c.strength))
            .collect();
        couples_write_rows(&mut s, cfg, "Top Developer Couples", &rows, MAX_ROWS);
    }

    // writeOwnershipRisk.
    if !metrics.file_ownership.is_empty() {
        s.push('\n');
        let _ = writeln!(s, "  {}", cfg.colorize("File Ownership Risk", Color::Blue));
        let _ = writeln!(s, "  {}", draw_separator(w - 4));
        // SortOwnershipByRisk: sort.Slice (unstable
        // pdqsort) on a copy by contributors ascending — go_sort replicates the
        // tie permutation.
        let mut sorted = metrics.file_ownership.clone();
        super::go_sort::slice(&mut sorted, |a, b| a.contributors < b.contributors);
        let shown = sorted.len().min(MAX_ROWS);
        for fo in &sorted[..shown] {
            let file = truncate_with_ellipsis(&fo.file, OWNERSHIP_WIDTH);
            let risk = if fo.contributors <= 1 {
                cfg.colorize(" !!", Color::Red)
            } else {
                String::new()
            };
            let _ = writeln!(
                s,
                "  {:<53} {:5} lines  {} contributors{}",
                file, fo.lines, fo.contributors, risk
            );
        }
        if sorted.len() > MAX_ROWS {
            let _ = writeln!(
                s,
                "  {}",
                cfg.colorize(&format!("  and {} more...", sorted.len() - MAX_ROWS), Color::Gray)
            );
        }
    }

    s.push('\n');
    Some(s.into_bytes())
}

// ---------------------------------------------------------------------------
// devs
// ---------------------------------------------------------------------------

/// `findPrimaryLanguage`: the language with
/// the most added lines, `"Other"` when none (or when the winner is unnamed).
fn devs_primary_language(dev: &cf_devs::DeveloperData) -> &str {
    let mut primary = "Other";
    let mut max_lines = 0i64;
    for entry in &dev.languages {
        if entry.added > max_lines {
            max_lines = entry.added;
            primary = if entry.language.is_empty() { "Other" } else { &entry.language };
        }
    }
    primary
}

/// `riskToColor`: CRITICAL/HIGH → red, MEDIUM → yellow,
/// else green (pkg/metrics RiskLevel strings).
fn devs_risk_color(level: &str) -> Color {
    match level {
        "CRITICAL" | "HIGH" => Color::Red,
        "MEDIUM" => Color::Yellow,
        _ => Color::Green,
    }
}

/// The `history/devs` text body — the reference `generateText`.
#[must_use]
pub fn devs_text(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    const MAX_CONTRIBUTORS: usize = 7; // textMaxContributors
    const MAX_BUS_FACTORS: usize = 7; // textMaxBusFactors
    const DEV_NAME_WIDTH: i64 = 18; // textDevNameWidth

    let metrics = super::history::devs_run_metrics(sub)?;
    let cfg = Config::new();
    let w = cfg.width;
    let mut s = String::new();

    let agg = &metrics.aggregate;
    s.push_str(&draw_header(
        "Developers",
        &format!("{} ticks", agg.analysis_period_ticks),
        w,
    ));
    s.push_str("\n\n");

    // writeSummarySection.
    let _ = writeln!(s, "  {}", cfg.colorize("Summary", Color::Blue));
    let _ = writeln!(s, "  {}", draw_separator(w - 4));
    let _ = writeln!(s, "  {:<22} {}", "Total Commits", format_int_thousands(agg.total_commits));
    let _ = writeln!(s, "  {:<22} {}", "Developers", format_int_thousands(agg.total_developers));
    let _ = writeln!(
        s,
        "  {:<22} {}",
        "Active Developers",
        format_int_thousands(agg.active_developers)
    );
    let _ = writeln!(
        s,
        "  {:<22} {}",
        "Project Bus Factor",
        format_int_thousands(agg.project_bus_factor)
    );
    let _ = writeln!(s, "  {:<22} {}", "Languages", format_int_thousands(agg.total_languages));

    // writeContributors. The empty Colorize calls still emit the
    // color-code + reset pairs around the +/- counters.
    if !metrics.developers.is_empty() {
        s.push('\n');
        let _ = writeln!(s, "  {}", cfg.colorize("Top Contributors", Color::Blue));
        let _ = writeln!(s, "  {}", draw_separator(w - 4));
        let shown = metrics.developers.len().min(MAX_CONTRIBUTORS);
        for dev in &metrics.developers[..shown] {
            let name = if dev.name.is_empty() { format!("dev#{}", dev.id) } else { dev.name.clone() };
            let name = truncate_with_ellipsis(&name, DEV_NAME_WIDTH);
            let primary_lang = devs_primary_language(dev);
            let _ = writeln!(
                s,
                "  {:<18} {:>6} commits  {}+{}{} / {}-{}{}  net {}  {}",
                name,
                format_int_thousands(dev.commits),
                cfg.colorize("", Color::Green),
                format_int_thousands(dev.added),
                cfg.colorize("", Color::None),
                cfg.colorize("", Color::Red),
                format_int_thousands(dev.removed),
                cfg.colorize("", Color::None),
                format_int_thousands(dev.net_lines),
                cfg.colorize(primary_lang, Color::Gray)
            );
        }
        if metrics.developers.len() > MAX_CONTRIBUTORS {
            let _ = writeln!(
                s,
                "  {}",
                cfg.colorize(
                    &format!("  and {} more...", metrics.developers.len() - MAX_CONTRIBUTORS),
                    Color::Gray
                )
            );
        }
    }

    // writeBusFactorRisk.
    if !metrics.busfactor.is_empty() {
        s.push('\n');
        let _ = writeln!(s, "  {}", cfg.colorize("Bus Factor Risk", Color::Blue));
        let _ = writeln!(s, "  {}", draw_separator(w - 4));
        let shown = metrics.busfactor.len().min(MAX_BUS_FACTORS);
        for bf in &metrics.busfactor[..shown] {
            let lang = truncate_with_ellipsis(&bf.language, DEV_NAME_WIDTH);
            let _ = writeln!(
                s,
                "  {:<18} {}  owner {:5.1}%  bf={}/{}",
                lang,
                cfg.colorize(&format!("{:<8}", bf.risk_level), devs_risk_color(&bf.risk_level)),
                bf.primary_pct,
                bf.bus_factor,
                bf.total_contributors
            );
        }
        if metrics.busfactor.len() > MAX_BUS_FACTORS {
            let _ = writeln!(
                s,
                "  {}",
                cfg.colorize(
                    &format!("  and {} more...", metrics.busfactor.len() - MAX_BUS_FACTORS),
                    Color::Gray
                )
            );
        }
    }

    // writeChurnSummary.
    if !metrics.churn.is_empty() {
        s.push('\n');
        let _ = writeln!(s, "  {}", cfg.colorize("Churn Summary", Color::Blue));
        let _ = writeln!(s, "  {}", draw_separator(w - 4));
        let mut total_added = 0i64;
        let mut total_removed = 0i64;
        for c in &metrics.churn {
            total_added += c.added;
            total_removed += c.removed;
        }
        let net = total_added - total_removed;
        let _ = writeln!(s, "  {:<22} {}", "Lines Added", format_int_thousands(total_added));
        let _ = writeln!(s, "  {:<22} {}", "Lines Removed", format_int_thousands(total_removed));
        let _ = writeln!(s, "  {:<22} {}", "Net Change", format_int_thousands(net));
    }

    s.push('\n');
    Some(s.into_bytes())
}

// ---------------------------------------------------------------------------
// file-history
// ---------------------------------------------------------------------------

/// `buildBar`: `int(pct/100*30)` clamped to
/// `[0, 30]` pipes.
fn file_history_build_bar(pct: f64, max_width: i64) -> String {
    let filled = ((pct / 100.0 * max_width as f64) as i64).clamp(0, max_width);
    "|".repeat(filled as usize)
}

/// The `history/file-history` text body — the reference `generateText`
///.
#[must_use]
pub fn file_history_text(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    const MAX_FILES: usize = 10; // textMaxFiles
    const BAR_WIDTH: i64 = 30; // textBarWidth
    const CATEGORY_WIDTH: usize = 16; // textCategoryWidth

    let metrics = super::history::file_history_run_metrics(sub)?;
    let cfg = Config::new();
    let w = cfg.width;
    let mut s = String::new();

    let agg = &metrics.aggregate;
    s.push_str(&draw_header("File History", &format!("{} files", agg.total_files), w));
    s.push_str("\n\n");

    // writeFileSummary.
    let _ = writeln!(s, "  {}", cfg.colorize("Summary", Color::Blue));
    let _ = writeln!(s, "  {}", draw_separator(w - 4));
    let _ = writeln!(s, "  {:<26} {}", "Total Files", agg.total_files);
    let _ = writeln!(s, "  {:<26} {}", "Total Commits", agg.total_commits);
    let _ = writeln!(s, "  {:<26} {}", "Total Contributors", agg.total_contributors);
    let _ = writeln!(s, "  {:<26} {:.1}", "Avg Commits/File", agg.avg_commits_per_file);
    let _ = writeln!(s, "  {:<26} {}", "High Churn Files", agg.high_churn_files);

    // writeComposition, iterating AllCategories in canonical order.
    if !metrics.composition.breakdown.is_empty() {
        s.push('\n');
        let _ = writeln!(s, "  {}", cfg.colorize("File Composition", Color::Blue));
        let _ = writeln!(s, "  {}", draw_separator(w - 4));
        for cat in cf_file_history::ALL_CATEGORIES {
            let count = metrics.composition.breakdown.get(cat.as_str()).copied().unwrap_or(0);
            let pct = metrics.composition.percentages.get(cat.as_str()).copied().unwrap_or(0.0);
            if count == 0 {
                continue;
            }
            let bar = file_history_build_bar(pct, BAR_WIDTH);
            let _ = writeln!(
                s,
                "  {:<CATEGORY_WIDTH$} {:5} ({:5.1}%) {}",
                cat.as_str(),
                count,
                pct,
                bar
            );
        }
    }

    // writeTopFiles.
    if !metrics.file_churn.is_empty() {
        s.push('\n');
        let _ = writeln!(s, "  {}", cfg.colorize("Most Modified Files", Color::Blue));
        let _ = writeln!(s, "  {}", draw_separator(w - 4));
        let limit = metrics.file_churn.len().min(MAX_FILES);
        for f in &metrics.file_churn[..limit] {
            let _ = writeln!(s, "  {:4} commits  {}", f.commit_count, f.path);
        }
        if metrics.file_churn.len() > limit {
            let _ = writeln!(s, "  ... and {} more files", metrics.file_churn.len() - limit);
        }
    }

    s.push('\n');
    Some(s.into_bytes())
}
