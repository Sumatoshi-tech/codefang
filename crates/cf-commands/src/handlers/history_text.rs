//! Per-analyzer `--format text` bodies: a human summary (a title box, the
//! key figures, the top items) for every `history/*` analyzer, drawn with the
//! shared [`cf_terminal`] helpers.
//!
//! Each function returns ONLY the analyzer's body; the `codefang (v2):`
//! header and the `"<id>:\n"` section line are emitted by
//! [`super::history_formats::history_text`], which renders every selected
//! analyzer before anything is written.
//!
//! # Width / color / padding semantics
//!
//! * Colors follow `NO_COLOR` only (never isatty), so piped bytes carry ANSI
//!   escapes unless `NO_COLOR` (or `--no-color`) is set. Width comes from
//!   `COLUMNS` (default 80). Both are read through [`cf_terminal::Config::new`].
//! * [`cf_terminal::pad_right`] / [`truncate_with_ellipsis`] /
//!   [`draw_header`] measure BYTES, while `format!` width specifiers
//!   (`{:<18}`) pad to a number of `char`s, so a non-ASCII label is padded
//!   by `format!`, never by the byte-length helpers.
//! * Rankings sort on an explicit total key (the ranked value, then the name)
//!   with a stable sort, so equal values always list in the same order.

use std::fmt::Write as _;

use cf_terminal::{
    color_for_score, draw_header, draw_percent_bar, draw_progress_bar, draw_separator,
    format_score_bar, truncate_with_ellipsis, Color, Config,
};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// `n` with its digits grouped in threes by `,` (`1234567` → `1,234,567`).
fn format_uint_thousands(n: u64) -> String {
    if n < 1000 {
        return n.to_string();
    }
    format!("{},{:03}", format_uint_thousands(n / 1000), n % 1000)
}

/// [`format_uint_thousands`] for a signed value (`-` prefix for negatives).
fn format_int_thousands(n: i64) -> String {
    if n < 0 {
        return format!("-{}", format_uint_thousands(n.unsigned_abs()));
    }
    format_uint_thousands(n as u64)
}

/// The last element of a slash-separated repository path: empty → `"."`,
/// trailing slashes stripped, all-slash → `"/"`.
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

/// Writes a blue section title and the separator under it.
fn write_section_title(s: &mut String, cfg: Config, title: &str) {
    let _ = writeln!(s, "  {}", cfg.colorize(title, Color::Blue));
    let _ = writeln!(s, "  {}", draw_separator(cfg.width - 4));
}

/// Writes the gray `and N more...` line under a truncated list (nothing when
/// `remaining` is zero).
fn write_more(s: &mut String, cfg: Config, remaining: usize) {
    if remaining > 0 {
        let _ = writeln!(
            s,
            "  {}",
            cfg.colorize(&format!("  and {remaining} more..."), Color::Gray)
        );
    }
}

// ---------------------------------------------------------------------------
// sentiment
// ---------------------------------------------------------------------------

/// Score at/above which a tick is positive.
const SENTIMENT_POSITIVE_THRESHOLD: f64 = 0.6;
/// Score at/below which a tick is negative.
const SENTIMENT_NEGATIVE_THRESHOLD: f64 = 0.4;
/// Sentiment renders at a FIXED width of 60, ignoring `Config::width`.
const SENTIMENT_TERM_WIDTH: i64 = 60;
/// Score / distribution bar width.
const SENTIMENT_BAR_WIDTH: i64 = 20;
/// Distribution label column width (label + emoji padding).
const SENTIMENT_DIST_LABEL_WIDTH: i64 = 18 + 4;
/// Risk periods listed before `... and N more`.
const SENTIMENT_MAX_RISK: usize = 5;
/// Gap between the `neg` and `pos` sparkline labels.
const SENTIMENT_SPARK_GAP: usize = 14;
/// The eight sparkline levels, lowest first.
const SPARKLINE_CHARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Green for a positive score, red for a negative one, else yellow.
fn sentiment_color(score: f64) -> Color {
    if score >= SENTIMENT_POSITIVE_THRESHOLD {
        Color::Green
    } else if score <= SENTIMENT_NEGATIVE_THRESHOLD {
        Color::Red
    } else {
        Color::Yellow
    }
}

/// The emoji of a score's class.
fn sentiment_label(score: f64) -> &'static str {
    if score >= SENTIMENT_POSITIVE_THRESHOLD {
        "😊"
    } else if score <= SENTIMENT_NEGATIVE_THRESHOLD {
        "😟"
    } else {
        "😐"
    }
}

/// The `history/sentiment` text body: the average score, the positive /
/// neutral / negative tick distribution, the trend, a sparkline timeline and
/// the low-sentiment risk periods.
#[must_use]
pub fn sentiment_text(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let metrics = super::history::sentiment_metrics(sub)?;
    let cfg = Config::new();
    let mut s = String::new();

    s.push_str(&draw_header(
        "SENTIMENT ANALYSIS",
        "💬",
        SENTIMENT_TERM_WIDTH,
    ));
    s.push_str("\n\n");

    // Summary. The Blue colorize wraps the trailing newline, so the reset
    // lands at the start of the next line.
    s.push_str(&cfg.colorize("  Summary\n", Color::Blue));
    s.push_str(&draw_separator(SENTIMENT_TERM_WIDTH));
    s.push('\n');
    let avg = f64::from(metrics.aggregate.average_sentiment);
    let _ = writeln!(
        s,
        "  Average Sentiment: {} {}",
        cfg.colorize(
            &format_score_bar(avg, SENTIMENT_BAR_WIDTH),
            sentiment_color(avg)
        ),
        sentiment_label(avg)
    );
    let _ = writeln!(s, "  Total Ticks:       {}", metrics.aggregate.total_ticks);
    let _ = writeln!(
        s,
        "  Total Comments:    {}",
        metrics.aggregate.total_comments
    );
    let _ = writeln!(
        s,
        "  Total Commits:     {}",
        metrics.aggregate.total_commits
    );
    s.push('\n');

    // Distribution of the scored ticks.
    if metrics.aggregate.total_ticks != 0 {
        s.push_str(&cfg.colorize("  Distribution\n", Color::Blue));
        s.push_str(&draw_separator(SENTIMENT_TERM_WIDTH));
        s.push('\n');
        let total = metrics.aggregate.total_ticks as f64;
        let items: [(&str, i64, Color, &str); 3] = [
            (
                "Positive (≥0.6)",
                metrics.aggregate.positive_ticks,
                Color::Green,
                "😊",
            ),
            (
                "Neutral",
                metrics.aggregate.neutral_ticks,
                Color::Yellow,
                "😐",
            ),
            (
                "Negative (≤0.4)",
                metrics.aggregate.negative_ticks,
                Color::Red,
                "😟",
            ),
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

    // Trend.
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
        let sign = if metrics.trend.change_percent < 0.0 {
            ""
        } else {
            "+"
        };
        let _ = writeln!(s, "  Change: {}{:.1}%", sign, metrics.trend.change_percent);
        s.push('\n');
    }

    // Sparkline timeline.
    if !metrics.time_series.is_empty() {
        s.push_str(&cfg.colorize("  Sentiment Timeline\n", Color::Blue));
        s.push_str(&draw_separator(SENTIMENT_TERM_WIDTH));
        s.push('\n');
        // One of eight levels: idx = max(int(min(score*8, 7)), 0).
        let mut sparkline = String::new();
        for ts in &metrics.time_series {
            let score = f64::from(ts.sentiment);
            let levels = SPARKLINE_CHARS.len() as f64;
            let idx = (((score * levels).min(levels - 1.0)) as i64).max(0) as usize;
            sparkline
                .push_str(&cfg.colorize(&SPARKLINE_CHARS[idx].to_string(), sentiment_color(score)));
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

    // Risk periods. The colorize wraps the newline, so each reset code
    // starts the FOLLOWING line.
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

/// `"name (base(file))"`, or the bare name without a file.
fn shotness_node_label(name: &str, file: &str) -> String {
    if file.is_empty() {
        return name.to_string();
    }
    format!("{} ({})", name, go_filepath_base(file))
}

/// Red for HIGH, yellow for MEDIUM, else green.
fn shotness_risk_color(level: &str) -> Color {
    match level {
        "HIGH" => Color::Red,
        "MEDIUM" => Color::Yellow,
        _ => Color::Green,
    }
}

/// The `history/shotness` text body: the summary, the hottest functions, the
/// risk assessment and the strongest function couplings.
#[must_use]
pub fn shotness_text(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    const BAR_WIDTH: i64 = 20;
    const LABEL_WIDTH: i64 = 24;
    const HALF_LABEL: i64 = 12;
    const MAX_ROWS: usize = 10;
    const SUMMARY_LABEL_WIDTH: usize = 22;

    let metrics = super::shotness_run::shotness_run_metrics(sub)?;
    let cfg = Config::new();
    let w = cfg.width;
    let mut s = String::new();

    let agg = &metrics.aggregate;
    s.push_str(&draw_header(
        "Shotness Analysis",
        &format!("{} nodes", agg.total_nodes),
        w,
    ));
    s.push_str("\n\n");

    write_section_title(&mut s, cfg, "Summary");
    let _ = writeln!(
        s,
        "  {:<SUMMARY_LABEL_WIDTH$} {}",
        "Total Nodes", agg.total_nodes
    );
    let _ = writeln!(
        s,
        "  {:<SUMMARY_LABEL_WIDTH$} {}",
        "Total Changes", agg.total_changes
    );
    let _ = writeln!(
        s,
        "  {:<SUMMARY_LABEL_WIDTH$} {:.1}",
        "Avg Changes/Node", agg.avg_changes_per_node
    );
    let _ = writeln!(
        s,
        "  {:<SUMMARY_LABEL_WIDTH$} {}",
        "Total Couplings", agg.total_couplings
    );
    let strength_color = color_for_score(1.0 - agg.avg_coupling_strength);
    let _ = writeln!(
        s,
        "  {:<SUMMARY_LABEL_WIDTH$} {}",
        "Avg Coupling Strength",
        cfg.colorize(
            &format!("{:.0}%", agg.avg_coupling_strength * 100.0),
            strength_color
        )
    );
    let hot_color = if agg.hot_nodes > 0 {
        Color::Red
    } else {
        Color::None
    };
    let _ = writeln!(
        s,
        "  {:<SUMMARY_LABEL_WIDTH$} {}",
        "Hot Nodes",
        cfg.colorize(&agg.hot_nodes.to_string(), hot_color)
    );
    if metrics.ended_tracked {
        let _ = writeln!(
            s,
            "  {:<SUMMARY_LABEL_WIDTH$} {}",
            "Ended Nodes", agg.ended_nodes
        );
    }

    if !metrics.node_hotness.is_empty() {
        s.push('\n');
        write_section_title(&mut s, cfg, "Hottest Functions");
        let shown = metrics.node_hotness.len().min(MAX_ROWS);
        for n in &metrics.node_hotness[..shown] {
            let label = truncate_with_ellipsis(&shotness_node_label(&n.name, &n.file), LABEL_WIDTH);
            let bar = draw_progress_bar(n.hotness_score, BAR_WIDTH);
            // A hotter node is colored as a lower score.
            let score_color = color_for_score(1.0 - n.hotness_score);
            let ended = if n.ended { ", ended" } else { "" };
            let _ = writeln!(
                s,
                "  {:<width$} [{}] {}  ({} changes{ended})",
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

    if !metrics.hotspot_nodes.is_empty() {
        s.push('\n');
        write_section_title(&mut s, cfg, "Risk Assessment");
        let shown = metrics.hotspot_nodes.len().min(MAX_ROWS);
        for n in &metrics.hotspot_nodes[..shown] {
            let label = truncate_with_ellipsis(&shotness_node_label(&n.name, &n.file), LABEL_WIDTH);
            let _ = writeln!(
                s,
                "  {:<width$} {}  ({} changes)",
                label,
                cfg.colorize(
                    &format!("{:<6}", n.risk_level),
                    shotness_risk_color(&n.risk_level)
                ),
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

    if !metrics.node_coupling.is_empty() {
        s.push('\n');
        write_section_title(&mut s, cfg, "Strongest Couplings");
        let shown = metrics.node_coupling.len().min(MAX_ROWS);
        for c in &metrics.node_coupling[..shown] {
            let left = truncate_with_ellipsis(&c.node1_name, HALF_LABEL);
            let right = truncate_with_ellipsis(&c.node2_name, HALF_LABEL);
            // A stronger coupling is colored as a lower score.
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

/// One display band of the age distribution.
struct AgeBand {
    label: &'static str,
    lines: i64,
}

/// Days per month in the age labels.
const DAYS_PER_MONTH: i64 = 30;
/// Hours per day in the age labels.
const HOURS_PER_DAY: i64 = 24;

/// The age in months of band `index` of `num_bands` at the last sample: band
/// 0 is the oldest, the newest band is one granularity old.
fn band_age_months(index: i64, num_bands: i64, granularity: i64, tick_size_hours: i64) -> i64 {
    (num_bands - index) * granularity * tick_size_hours / HOURS_PER_DAY / DAYS_PER_MONTH
}

/// Groups the last sample's bands into at most five labeled age buckets by
/// [`band_age_months`], dropping empty ones.
fn build_age_bands(breakdown: &[i64], granularity: i64, tick_size_hours: i64) -> Vec<AgeBand> {
    // (maxMonths, label); maxMonths 0 = unbounded.
    const BUCKETS: [(i64, &str); 5] = [
        (1, "< 1 month"),
        (3, "1-3 months"),
        (6, "3-6 months"),
        (12, "6-12 months"),
        (0, "> 12 months"),
    ];
    let num_bands = breakdown.len() as i64;
    let mut lines = [0i64; BUCKETS.len()];
    for (i, &val) in breakdown.iter().enumerate() {
        if val <= 0 {
            continue;
        }
        let age_months = band_age_months(i as i64, num_bands, granularity, tick_size_hours);
        let bucket_idx = BUCKETS
            .iter()
            .position(|(max_months, _)| *max_months > 0 && age_months <= *max_months)
            .unwrap_or(BUCKETS.len() - 1);
        lines[bucket_idx] += val;
    }
    BUCKETS
        .iter()
        .zip(lines)
        .filter(|(_, l)| *l > 0)
        .map(|((_, label), lines)| AgeBand { label, lines })
        .collect()
}

/// The `history/burndown` text body: the summary, the code-age distribution
/// of the last sample and, with `--burndown-people`, the top developers by
/// surviving lines.
#[must_use]
pub fn burndown_text(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    const BAR_WIDTH: i64 = 20;
    const LABEL_WIDTH: i64 = 14;
    const DEV_NAME_WIDTH: i64 = 16;
    const MAX_DEVS: usize = 5;
    const NANOS_PER_HOUR: i64 = 3_600 * 1_000_000_000;

    let run = super::burndown_ndjson::burndown_run_aggregate(sub)?;
    let metrics = &run.metrics;
    let cfg = Config::new();
    let w = cfg.width;
    let mut s = String::new();

    let agg = &metrics.aggregate;
    let project = if run.project_name.is_empty() {
        "project"
    } else {
        run.project_name.as_str()
    };
    s.push_str(&draw_header(
        &format!("Burndown: {project}"),
        &format!("{}d", agg.analysis_period_days),
        w,
    ));
    s.push_str("\n\n");

    write_section_title(&mut s, cfg, "Summary");
    let survival_pct = agg.overall_survival_rate;
    let survival_color = color_for_score(survival_pct);
    let bar = draw_progress_bar(survival_pct, BAR_WIDTH);
    let _ = writeln!(
        s,
        "  {:<18} {}",
        "Current Lines",
        format_int_thousands(agg.total_current_lines)
    );
    let _ = writeln!(
        s,
        "  {:<18} {}",
        "Peak Lines",
        format_int_thousands(agg.total_peak_lines)
    );
    let _ = writeln!(
        s,
        "  {:<18} [{}] {}",
        "Survival Rate",
        bar,
        cfg.colorize(&format!("{:.1}%", survival_pct * 100.0), survival_color)
    );

    if let Some(last_sample) = metrics.global_survival.last() {
        s.push('\n');
        write_section_title(&mut s, cfg, "Code Age Distribution");
        if last_sample.total_lines != 0 {
            let tick_size_hours = run.tick_size_ns / NANOS_PER_HOUR;
            for band in build_age_bands(
                &last_sample.band_breakdown,
                run.grid.granularity,
                tick_size_hours,
            ) {
                let pct = band.lines as f64 / last_sample.total_lines as f64;
                let _ = writeln!(
                    s,
                    "  {}",
                    draw_percent_bar(band.label, pct, band.lines, LABEL_WIDTH, BAR_WIDTH)
                );
            }
        }
    }

    // developer_survival is already sorted by surviving lines (id on a tie).
    if !metrics.developer_survival.is_empty() {
        s.push('\n');
        write_section_title(&mut s, cfg, "Top Developers (by surviving lines)");
        let devs = &metrics.developer_survival;
        let shown = devs.len().min(MAX_DEVS);
        for dev in &devs[..shown] {
            let name = if dev.name.is_empty() {
                format!("dev#{}", dev.id)
            } else {
                dev.name.clone()
            };
            let name = truncate_with_ellipsis(&name, DEV_NAME_WIDTH);
            let survival_color = color_for_score(dev.survival_rate);
            let bar = draw_progress_bar(dev.survival_rate, BAR_WIDTH);
            let _ = writeln!(
                s,
                "  {:<16} {:8}  [{}] {}",
                name,
                dev.current_lines,
                bar,
                cfg.colorize(
                    &format!("{:.1}%", dev.survival_rate * 100.0),
                    survival_color
                )
            );
        }
        write_more(&mut s, cfg, devs.len().saturating_sub(MAX_DEVS));
    }

    s.push('\n');
    Some(s.into_bytes())
}

// ---------------------------------------------------------------------------
// couples
// ---------------------------------------------------------------------------

/// Red from 70% coupling, yellow from 40%, else green.
fn couples_strength_color(strength: f64) -> Color {
    if strength >= 0.7 {
        Color::Red
    } else if strength >= 0.4 {
        Color::Yellow
    } else {
        Color::Green
    }
}

/// `v` as a whole percentage (`0.654` → `65%`).
fn couples_format_pct(v: f64) -> String {
    format!("{:.0}%", v * 100.0)
}

/// One titled section of coupling pairs `(left, right, count, strength)`.
fn couples_write_rows(
    s: &mut String,
    cfg: Config,
    title: &str,
    rows: &[(String, String, i64, f64)],
    max_rows: usize,
) {
    const NAME_WIDTH: i64 = 25;

    write_section_title(s, cfg, title);
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
            cfg.colorize(
                &couples_format_pct(*strength),
                couples_strength_color(*strength)
            )
        );
    }
    write_more(s, cfg, rows.len().saturating_sub(max_rows));
}

/// The files ranked by ownership risk: fewest contributors first, then by
/// path — a stable sort on a total key, so equal counts always list in path
/// order.
fn rank_ownership_by_risk(
    ownership: &[cf_couples::FileOwnershipData],
) -> Vec<&cf_couples::FileOwnershipData> {
    let mut ranked: Vec<&cf_couples::FileOwnershipData> = ownership.iter().collect();
    ranked.sort_by(|a, b| {
        a.contributors
            .cmp(&b.contributors)
            .then_with(|| a.file.cmp(&b.file))
    });
    ranked
}

/// The "File Ownership Risk" section: the `max_rows` riskiest files with
/// their line and contributor counts, a single-owner file flagged `!!`.
fn couples_write_ownership(
    s: &mut String,
    cfg: Config,
    ownership: &[cf_couples::FileOwnershipData],
    max_rows: usize,
) {
    const OWNERSHIP_WIDTH: i64 = 25 * 2 + 3;

    write_section_title(s, cfg, "File Ownership Risk");
    let ranked = rank_ownership_by_risk(ownership);
    for fo in ranked.iter().take(max_rows) {
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
    write_more(s, cfg, ranked.len().saturating_sub(max_rows));
}

/// The `history/couples` text body: the summary, the top file and developer
/// couples and the file-ownership risk (fewest contributors first).
#[must_use]
pub fn couples_text(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    const MAX_ROWS: usize = 7;

    let metrics = super::couples_run::couples_run_metrics(sub)?;
    let cfg = Config::new();
    let w = cfg.width;
    let mut s = String::new();

    let agg = &metrics.aggregate;
    s.push_str(&draw_header(
        "Couples",
        &format!("{} files", agg.total_files),
        w,
    ));
    s.push_str("\n\n");

    write_section_title(&mut s, cfg, "Summary");
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

    if !metrics.file_coupling.is_empty() {
        s.push('\n');
        let rows: Vec<(String, String, i64, f64)> = metrics
            .file_coupling
            .iter()
            .map(|c| (c.file1.clone(), c.file2.clone(), c.co_changes, c.strength))
            .collect();
        couples_write_rows(&mut s, cfg, "Top File Couples", &rows, MAX_ROWS);
    }

    if !metrics.developer_coupling.is_empty() {
        s.push('\n');
        let rows: Vec<(String, String, i64, f64)> = metrics
            .developer_coupling
            .iter()
            .map(|c| {
                (
                    c.developer1.clone(),
                    c.developer2.clone(),
                    c.shared_files,
                    c.strength,
                )
            })
            .collect();
        couples_write_rows(&mut s, cfg, "Top Developer Couples", &rows, MAX_ROWS);
    }

    if !metrics.file_ownership.is_empty() {
        s.push('\n');
        couples_write_ownership(&mut s, cfg, &metrics.file_ownership, MAX_ROWS);
    }

    s.push('\n');
    Some(s.into_bytes())
}

// ---------------------------------------------------------------------------
// devs
// ---------------------------------------------------------------------------

/// The language with the most added lines, `"Other"` when none (or when the
/// winner is unnamed).
fn devs_primary_language(dev: &cf_devs::DeveloperData) -> &str {
    let mut primary = "Other";
    let mut max_lines = 0i64;
    for entry in &dev.languages {
        if entry.added > max_lines {
            max_lines = entry.added;
            primary = if entry.language.is_empty() {
                "Other"
            } else {
                &entry.language
            };
        }
    }
    primary
}

/// CRITICAL / HIGH → red, MEDIUM → yellow, else green.
fn devs_risk_color(level: &str) -> Color {
    match level {
        "CRITICAL" | "HIGH" => Color::Red,
        "MEDIUM" => Color::Yellow,
        _ => Color::Green,
    }
}

/// The `history/devs` text body: the summary, the top contributors, the
/// per-language bus-factor risk and the churn totals.
#[must_use]
pub fn devs_text(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    const MAX_CONTRIBUTORS: usize = 7;
    const MAX_BUS_FACTORS: usize = 7;
    const DEV_NAME_WIDTH: i64 = 18;

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

    write_section_title(&mut s, cfg, "Summary");
    let _ = writeln!(
        s,
        "  {:<22} {}",
        "Total Commits",
        format_int_thousands(agg.total_commits)
    );
    let _ = writeln!(
        s,
        "  {:<22} {}",
        "Developers",
        format_int_thousands(agg.total_developers)
    );
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
    let _ = writeln!(
        s,
        "  {:<22} {}",
        "Languages",
        format_int_thousands(agg.total_languages)
    );

    // Top contributors. The empty colorize calls emit the color-code +
    // reset pairs around the +/- counters.
    if !metrics.developers.is_empty() {
        s.push('\n');
        write_section_title(&mut s, cfg, "Top Contributors");
        let shown = metrics.developers.len().min(MAX_CONTRIBUTORS);
        for dev in &metrics.developers[..shown] {
            let name = if dev.name.is_empty() {
                format!("dev#{}", dev.id)
            } else {
                dev.name.clone()
            };
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
        write_more(
            &mut s,
            cfg,
            metrics.developers.len().saturating_sub(MAX_CONTRIBUTORS),
        );
    }

    if !metrics.busfactor.is_empty() {
        s.push('\n');
        write_section_title(&mut s, cfg, "Bus Factor Risk");
        let shown = metrics.busfactor.len().min(MAX_BUS_FACTORS);
        for bf in &metrics.busfactor[..shown] {
            let lang = truncate_with_ellipsis(&bf.language, DEV_NAME_WIDTH);
            let _ = writeln!(
                s,
                "  {:<18} {}  owner {:5.1}%  bf={}/{}",
                lang,
                cfg.colorize(
                    &format!("{:<8}", bf.risk_level),
                    devs_risk_color(&bf.risk_level)
                ),
                bf.primary_pct,
                bf.bus_factor,
                bf.total_contributors
            );
        }
        write_more(
            &mut s,
            cfg,
            metrics.busfactor.len().saturating_sub(MAX_BUS_FACTORS),
        );
    }

    if !metrics.churn.is_empty() {
        s.push('\n');
        write_section_title(&mut s, cfg, "Churn Summary");
        let mut total_added = 0i64;
        let mut total_removed = 0i64;
        for c in &metrics.churn {
            total_added += c.added;
            total_removed += c.removed;
        }
        let net = total_added - total_removed;
        let _ = writeln!(
            s,
            "  {:<22} {}",
            "Lines Added",
            format_int_thousands(total_added)
        );
        let _ = writeln!(
            s,
            "  {:<22} {}",
            "Lines Removed",
            format_int_thousands(total_removed)
        );
        let _ = writeln!(s, "  {:<22} {}", "Net Change", format_int_thousands(net));
    }

    s.push('\n');
    Some(s.into_bytes())
}

// ---------------------------------------------------------------------------
// file-history
// ---------------------------------------------------------------------------

/// `int(pct / 100 * max_width)` pipes, clamped to `[0, max_width]`.
fn file_history_build_bar(pct: f64, max_width: i64) -> String {
    let filled = ((pct / 100.0 * max_width as f64) as i64).clamp(0, max_width);
    "|".repeat(filled as usize)
}

/// The `history/file-history` text body: the summary, the file composition
/// by category and the most modified files.
#[must_use]
pub fn file_history_text(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    const MAX_FILES: usize = 10;
    const BAR_WIDTH: i64 = 30;
    const CATEGORY_WIDTH: usize = 16;

    let metrics = super::history::file_history_run_metrics(sub)?;
    let cfg = Config::new();
    let w = cfg.width;
    let mut s = String::new();

    let agg = &metrics.aggregate;
    s.push_str(&draw_header(
        "File History",
        &format!("{} files", agg.total_files),
        w,
    ));
    s.push_str("\n\n");

    write_section_title(&mut s, cfg, "Summary");
    let _ = writeln!(s, "  {:<26} {}", "Total Files", agg.total_files);
    let _ = writeln!(s, "  {:<26} {}", "Total Commits", agg.total_commits);
    let _ = writeln!(
        s,
        "  {:<26} {}",
        "Total Contributors", agg.total_contributors
    );
    let _ = writeln!(
        s,
        "  {:<26} {:.1}",
        "Avg Commits/File", agg.avg_commits_per_file
    );
    let _ = writeln!(s, "  {:<26} {}", "High Churn Files", agg.high_churn_files);

    // Composition, in the canonical category order.
    if !metrics.composition.breakdown.is_empty() {
        s.push('\n');
        write_section_title(&mut s, cfg, "File Composition");
        for cat in cf_file_history::ALL_CATEGORIES {
            let count = metrics
                .composition
                .breakdown
                .get(cat.as_str())
                .copied()
                .unwrap_or(0);
            let pct = metrics
                .composition
                .percentages
                .get(cat.as_str())
                .copied()
                .unwrap_or(0.0);
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

    if !metrics.file_churn.is_empty() {
        s.push('\n');
        write_section_title(&mut s, cfg, "Most Modified Files");
        let limit = metrics.file_churn.len().min(MAX_FILES);
        for f in &metrics.file_churn[..limit] {
            let _ = writeln!(s, "  {:4} commits  {}", f.commit_count, f.path);
        }
        if metrics.file_churn.len() > limit {
            let _ = writeln!(
                s,
                "  ... and {} more files",
                metrics.file_churn.len() - limit
            );
        }
    }

    s.push('\n');
    Some(s.into_bytes())
}

// ---------------------------------------------------------------------------
// Summary rows shared by quality / typos / anomaly / imports
// ---------------------------------------------------------------------------

/// Label column width of the quality / typos / anomaly / imports summaries.
const SUMMARY_LABEL_WIDTH: usize = 26;
/// Rows a top-items section lists before `and N more...`.
const MAX_TOP_ROWS: usize = 10;
/// Characters of an RFC 3339 timestamp that form its date.
const RFC3339_DATE_LEN: usize = 10;
/// Hex digits of a commit hash shown in a text row.
const SHORT_HASH_LEN: usize = 7;
/// Column width of a `wrong → correct` typo pair.
const TYPO_PAIR_WIDTH: usize = 36;
/// Column width of an import path.
const IMPORT_NAME_WIDTH: usize = 40;
/// Column width of a developer name.
const DEVELOPER_NAME_WIDTH: usize = 20;

/// Writes one `label value` summary row.
fn write_summary_row(s: &mut String, label: &str, value: impl std::fmt::Display) {
    let _ = writeln!(s, "  {label:<SUMMARY_LABEL_WIDTH$} {value}");
}

/// The title box and the `Summary` section heading every summary opens with.
fn summary_opening(cfg: Config, title: &str, subtitle: &str) -> String {
    let mut s = draw_header(title, subtitle, cfg.width);
    s.push_str("\n\n");
    write_section_title(&mut s, cfg, "Summary");
    s
}

/// The date part of an RFC 3339 timestamp (the whole string when shorter).
fn rfc3339_date(ts: &str) -> &str {
    ts.get(..RFC3339_DATE_LEN).unwrap_or(ts)
}

// ---------------------------------------------------------------------------
// quality
// ---------------------------------------------------------------------------

/// The `history/quality` text body: the aggregate complexity, Halstead,
/// comment and cohesion figures and the ticks with the highest p95
/// complexity.
#[must_use]
pub fn quality_text(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let metrics = super::history::quality_metrics(sub)?;
    Some(render_quality(&metrics, Config::new()).into_bytes())
}

/// Renders a quality report (see [`quality_text`]).
fn render_quality(metrics: &cf_quality::ComputedMetrics, cfg: Config) -> String {
    let agg = &metrics.aggregate;
    let mut s = summary_opening(cfg, "Code Quality", &format!("{} ticks", agg.total_ticks));
    write_summary_row(&mut s, "Ticks Analyzed", agg.total_ticks);
    write_summary_row(&mut s, "Files Analyzed", agg.total_files_analyzed);
    write_summary_row(
        &mut s,
        "Complexity (median)",
        format!("{:.2}", agg.complexity_median_mean),
    );
    write_summary_row(
        &mut s,
        "Complexity (p95)",
        format!("{:.2}", agg.complexity_p95_mean),
    );
    write_summary_row(
        &mut s,
        "Halstead Volume (median)",
        format!("{:.2}", agg.halstead_vol_median_mean),
    );
    write_summary_row(
        &mut s,
        "Delivered Bugs",
        format!("{:.2}", agg.total_delivered_bugs),
    );
    write_summary_row(
        &mut s,
        "Comment Score",
        format!(
            "{:.2}  (min {:.2})",
            agg.comment_score_mean_mean, agg.min_comment_score
        ),
    );
    write_summary_row(
        &mut s,
        "Cohesion",
        format!(
            "{:.2}  (min {:.2})",
            agg.cohesion_mean_mean, agg.min_cohesion
        ),
    );

    if !metrics.time_series.is_empty() {
        s.push('\n');
        write_section_title(&mut s, cfg, "Most Complex Ticks");
        let mut ranked: Vec<&cf_quality::TimeSeriesEntry> = metrics.time_series.iter().collect();
        ranked.sort_by(|a, b| {
            b.stats
                .complexity_p95
                .total_cmp(&a.stats.complexity_p95)
                .then_with(|| a.tick.cmp(&b.tick))
        });
        for e in ranked.iter().take(MAX_TOP_ROWS) {
            let _ = writeln!(
                s,
                "  tick {:<5} p95 {:>7.2}  max {:>4.0}  {:>4} files  {}",
                e.tick,
                e.stats.complexity_p95,
                e.stats.complexity_max,
                e.stats.files_analyzed,
                rfc3339_date(&e.start_time)
            );
        }
        write_more(&mut s, cfg, ranked.len().saturating_sub(MAX_TOP_ROWS));
    }

    s.push('\n');
    s
}

// ---------------------------------------------------------------------------
// typos
// ---------------------------------------------------------------------------

/// The `history/typos` text body: the fix counts, each `wrong → correct`
/// pair at its first fix, the recurring pairs and the files with the most
/// fixes.
#[must_use]
pub fn typos_text(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let report = super::history::typos_report_data(sub)?;
    Some(render_typos(&report, Config::new()).into_bytes())
}

/// Renders a typos report (see [`typos_text`]).
fn render_typos(report: &cf_typos::ReportData, cfg: Config) -> String {
    use cf_typos::metrics::{
        compute_aggregate, compute_file_typos, compute_typo_list, compute_typo_patterns,
    };

    let agg = compute_aggregate(report);
    let mut s = summary_opening(cfg, "Typos", &format!("{} fixes", agg.total_typos));
    write_summary_row(&mut s, "Typo Fixes", agg.total_typos);
    write_summary_row(&mut s, "Unique Pairs", agg.unique_patterns);
    write_summary_row(&mut s, "Affected Files", agg.affected_files);
    write_summary_row(&mut s, "Affected Commits", agg.affected_commits);

    let list = compute_typo_list(report);
    if !list.is_empty() {
        s.push('\n');
        write_section_title(&mut s, cfg, "Typo Fixes (first occurrence)");
        for t in list.iter().take(MAX_TOP_ROWS) {
            let pair = format!("{} → {}", t.wrong, t.correct);
            let short = t.commit.get(..SHORT_HASH_LEN).unwrap_or(&t.commit);
            let _ = writeln!(
                s,
                "  {pair:<TYPO_PAIR_WIDTH$} {}:{}  {}",
                t.file,
                t.line,
                cfg.colorize(short, Color::Gray)
            );
        }
        write_more(&mut s, cfg, list.len().saturating_sub(MAX_TOP_ROWS));
    }

    let patterns = compute_typo_patterns(report);
    if !patterns.is_empty() {
        s.push('\n');
        write_section_title(&mut s, cfg, "Recurring Pairs");
        for p in patterns.iter().take(MAX_TOP_ROWS) {
            let pair = format!("{} → {}", p.wrong, p.correct);
            let _ = writeln!(s, "  {pair:<TYPO_PAIR_WIDTH$} ×{}", p.frequency);
        }
        write_more(&mut s, cfg, patterns.len().saturating_sub(MAX_TOP_ROWS));
    }

    let files = compute_file_typos(report);
    if !files.is_empty() {
        s.push('\n');
        write_section_title(&mut s, cfg, "Files");
        for f in files.iter().take(MAX_TOP_ROWS) {
            let _ = writeln!(s, "  {:4} fixes  {}", f.typo_count, f.file);
        }
        write_more(&mut s, cfg, files.len().saturating_sub(MAX_TOP_ROWS));
    }

    s.push('\n');
    s
}

// ---------------------------------------------------------------------------
// anomaly
// ---------------------------------------------------------------------------

/// The `history/anomaly` text body: the detection settings, the per-tick
/// baselines and the anomalous ticks, most severe first.
#[must_use]
pub fn anomaly_text(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let metrics = super::history::anomaly_run_metrics(sub)?;
    Some(render_anomaly(&metrics, Config::new()).into_bytes())
}

/// Renders an anomaly report (see [`anomaly_text`]).
fn render_anomaly(metrics: &cf_anomaly::model::ComputedMetrics, cfg: Config) -> String {
    let agg = &metrics.aggregate;
    let mut s = summary_opening(
        cfg,
        "Temporal Anomalies",
        &format!("{} ticks", agg.total_ticks),
    );
    write_summary_row(&mut s, "Ticks Analyzed", agg.total_ticks);
    write_summary_row(
        &mut s,
        "Anomalies",
        format!(
            "{}  ({:.1}% of ticks)",
            agg.total_anomalies, agg.anomaly_rate
        ),
    );
    write_summary_row(
        &mut s,
        "Z-Score Threshold",
        format!("{:.2}", f64::from(agg.threshold)),
    );
    write_summary_row(&mut s, "Window", format!("{} ticks", agg.window_size));
    let baselines = [
        ("Net Churn / Tick", agg.churn_mean, agg.churn_stddev),
        ("Files / Tick", agg.files_mean, agg.files_stddev),
        (
            "Languages / Tick",
            agg.lang_diversity_mean,
            agg.lang_diversity_stddev,
        ),
        (
            "Authors / Tick",
            agg.author_count_mean,
            agg.author_count_stddev,
        ),
    ];
    for (label, mean, stddev) in baselines {
        write_summary_row(&mut s, label, format!("{mean:.2} ± {stddev:.2}"));
    }

    if !metrics.anomalies.is_empty() {
        s.push('\n');
        write_section_title(&mut s, cfg, "Anomalies (most severe first)");
        for a in metrics.anomalies.iter().take(MAX_TOP_ROWS) {
            let m = &a.metrics;
            let _ = writeln!(
                s,
                "  tick {:<5} |z| {}  churn {:+}  (+{} / -{})  {} files  {} authors",
                a.tick,
                cfg.colorize(&format!("{:>6.2}", a.max_abs_z_score), Color::Red),
                m.net_churn,
                m.lines_added,
                m.lines_removed,
                m.files_changed,
                m.author_count
            );
        }
        write_more(
            &mut s,
            cfg,
            metrics.anomalies.len().saturating_sub(MAX_TOP_ROWS),
        );
    }

    s.push('\n');
    s
}

// ---------------------------------------------------------------------------
// imports
// ---------------------------------------------------------------------------

/// The headline figures of a `history/imports` report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ImportsSummary {
    /// Developers who added at least one import.
    pub developers: usize,
    /// Distinct languages the imports were found in.
    pub languages: usize,
    /// Distinct import paths.
    pub distinct_imports: usize,
    /// Import additions summed over developers, languages and ticks.
    pub usages: i64,
}

/// The headline figures of `report` (the text summary and the compact line).
pub(crate) fn imports_summary(report: &cf_imports::HistoryReport) -> ImportsSummary {
    let counts = cf_imports::history::aggregate_import_counts(&report.imports);
    let languages: std::collections::BTreeSet<&str> = report
        .imports
        .values()
        .flat_map(|langs| langs.keys().map(String::as_str))
        .collect();
    ImportsSummary {
        developers: report.imports.len(),
        languages: languages.len(),
        distinct_imports: counts.len(),
        usages: counts.values().sum(),
    }
}

/// The `history/imports` text body: the import counts, the most used
/// imports and the developers who added the most distinct imports.
#[must_use]
pub fn imports_text(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let report = super::history::imports_run_metrics(sub)?;
    Some(render_imports(&report, Config::new()).into_bytes())
}

/// The developers ranked by the number of distinct imports they added
/// (descending, then by name): `(name, distinct imports, languages)`.
fn rank_importers(report: &cf_imports::HistoryReport) -> Vec<(String, usize, String)> {
    let mut ranked: Vec<(String, usize, String)> = report
        .imports
        .iter()
        .map(|(&author, langs)| {
            // `author_index` holds `name|email` signatures; show the name.
            let name = usize::try_from(author)
                .ok()
                .and_then(|i| report.author_index.get(i))
                .and_then(|n| n.split('|').next())
                .filter(|n| !n.is_empty())
                .map_or_else(|| format!("dev#{author}"), str::to_string);
            let distinct = langs.values().map(std::collections::BTreeMap::len).sum();
            let names: Vec<&str> = langs.keys().map(String::as_str).collect();
            (name, distinct, names.join(", "))
        })
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked
}

/// Renders an imports report (see [`imports_text`]).
fn render_imports(report: &cf_imports::HistoryReport, cfg: Config) -> String {
    let summary = imports_summary(report);
    let mut s = summary_opening(
        cfg,
        "Imports",
        &format!("{} developers", summary.developers),
    );
    write_summary_row(&mut s, "Developers", summary.developers);
    write_summary_row(&mut s, "Languages", summary.languages);
    write_summary_row(&mut s, "Distinct Imports", summary.distinct_imports);
    write_summary_row(&mut s, "Import Usages", summary.usages);

    let counts = cf_imports::history::aggregate_import_counts(&report.imports);
    let (names, uses) = cf_imports::history::top_imports(&counts);
    if !names.is_empty() {
        s.push('\n');
        write_section_title(&mut s, cfg, "Most Used Imports");
        for (name, n) in names.iter().zip(&uses).take(MAX_TOP_ROWS) {
            let _ = writeln!(s, "  {name:<IMPORT_NAME_WIDTH$} {n:>6}");
        }
        write_more(&mut s, cfg, counts.len().saturating_sub(MAX_TOP_ROWS));
    }

    let importers = rank_importers(report);
    if !importers.is_empty() {
        s.push('\n');
        write_section_title(&mut s, cfg, "Top Importers");
        for (name, distinct, langs) in importers.iter().take(MAX_TOP_ROWS) {
            let _ = writeln!(
                s,
                "  {name:<DEVELOPER_NAME_WIDTH$} {distinct:>4} imports  {}",
                cfg.colorize(langs, Color::Gray)
            );
        }
        write_more(&mut s, cfg, importers.len().saturating_sub(MAX_TOP_ROWS));
    }

    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A colorless 80-column configuration.
    const PLAIN: Config = Config {
        width: 80,
        no_color: true,
    };

    fn ownership(file: &str, contributors: i32) -> cf_couples::FileOwnershipData {
        cf_couples::FileOwnershipData {
            file: file.to_string(),
            lines: 1,
            contributors,
            top_contributor: String::new(),
        }
    }

    /// Equal contributor counts rank by path, whatever the input order.
    #[test]
    fn ownership_risk_ranks_by_contributors_then_path() {
        let input = [
            ownership("z.go", 1),
            ownership("m.go", 3),
            ownership("a.go", 1),
            ownership("b.go", 2),
        ];
        let mut reversed = input.clone();
        reversed.reverse();
        for list in [&input[..], &reversed[..]] {
            let files: Vec<&str> = rank_ownership_by_risk(list)
                .iter()
                .map(|f| f.file.as_str())
                .collect();
            assert_eq!(files, ["a.go", "z.go", "b.go", "m.go"]);
        }
    }

    #[test]
    fn write_more_is_silent_without_remaining_rows() {
        let mut s = String::new();
        write_more(&mut s, PLAIN, 0);
        assert!(s.is_empty());
        write_more(&mut s, PLAIN, 3);
        assert_eq!(s, "    and 3 more...\n");
    }

    fn typo(wrong: &str, correct: &str, file: &str, line: i64) -> cf_typos::Typo {
        cf_typos::Typo {
            wrong: wrong.to_string(),
            correct: correct.to_string(),
            file: file.to_string(),
            commit: cf_typos::Hash([0xab; 20]),
            line,
        }
    }

    #[test]
    fn typos_text_lists_pairs_recurrences_and_files() {
        let report = cf_typos::ReportData {
            typos: vec![
                typo("recieve", "receive", "a.go", 3),
                typo("recieve", "receive", "b.go", 7),
                typo("lenght", "length", "a.go", 9),
            ],
        };
        let text = render_typos(&report, PLAIN);
        assert!(text.contains("Typo Fixes                 3"), "{text}");
        assert!(text.contains("Unique Pairs               2"), "{text}");
        assert!(text.contains("recieve → receive"), "{text}");
        assert!(text.contains("a.go:3  abababa"), "{text}");
        assert!(text.contains("×2"), "{text}");
        assert!(text.contains("   2 fixes  a.go"), "{text}");
    }

    #[test]
    fn an_empty_typos_report_is_only_the_summary() {
        let text = render_typos(&cf_typos::ReportData::default(), PLAIN);
        assert!(text.contains("Typo Fixes                 0"), "{text}");
        assert!(!text.contains("Recurring Pairs"), "{text}");
        assert!(!text.contains("\n  Files\n"), "{text}");
    }

    #[test]
    fn anomaly_text_lists_the_anomalous_ticks() {
        let metrics = cf_anomaly::model::ComputedMetrics {
            anomalies: vec![cf_anomaly::model::Record {
                tick: 9,
                max_abs_z_score: 3.8,
                metrics: cf_anomaly::model::RawMetrics {
                    files_changed: 2,
                    lines_added: 40,
                    lines_removed: 4,
                    net_churn: 36,
                    language_diversity: 1,
                    author_count: 1,
                },
                ..cf_anomaly::model::Record::default()
            }],
            aggregate: cf_anomaly::model::AggregateData {
                total_ticks: 3,
                total_anomalies: 1,
                anomaly_rate: 100.0 / 3.0,
                threshold: 2.0,
                window_size: 20,
                ..cf_anomaly::model::AggregateData::default()
            },
            ..cf_anomaly::model::ComputedMetrics::default()
        };
        let text = render_anomaly(&metrics, PLAIN);
        assert!(text.contains("1  (33.3% of ticks)"), "{text}");
        assert!(
            text.contains("Window                     20 ticks"),
            "{text}"
        );
        assert!(
            text.contains("tick 9     |z|   3.80  churn +36  (+40 / -4)  2 files  1 authors"),
            "{text}"
        );
    }

    #[test]
    fn imports_text_ranks_imports_and_importers() {
        let mut imports = cf_imports::ImportsMap::new();
        let ticks = |n: i64| std::collections::BTreeMap::from([(0, n)]);
        imports.insert(
            0,
            std::collections::BTreeMap::from([(
                "go".to_string(),
                std::collections::BTreeMap::from([
                    ("fmt".to_string(), ticks(1)),
                    ("os".to_string(), ticks(2)),
                ]),
            )]),
        );
        imports.insert(
            1,
            std::collections::BTreeMap::from([(
                "python".to_string(),
                std::collections::BTreeMap::from([("os".to_string(), ticks(1))]),
            )]),
        );
        let report = cf_imports::HistoryReport {
            tick_size_secs: 86_400,
            author_index: vec!["alice|a@x".to_string(), "bob|b@x".to_string()],
            imports,
        };
        assert_eq!(
            imports_summary(&report),
            ImportsSummary {
                developers: 2,
                languages: 2,
                distinct_imports: 2,
                usages: 4,
            }
        );
        let text = render_imports(&report, PLAIN);
        let os = text.find("\n  os ").expect("os row");
        let fmt = text.find("\n  fmt ").expect("fmt row");
        assert!(os < fmt, "os (3 uses) ranks above fmt (1):\n{text}");
        assert!(
            text.contains("alice                   2 imports  go"),
            "{text}"
        );
        assert!(
            text.contains("bob                     1 imports  python"),
            "{text}"
        );
    }

    #[test]
    fn quality_text_ranks_ticks_by_p95_complexity() {
        let entry = |tick: i64, p95: f64| cf_quality::TimeSeriesEntry {
            tick,
            start_time: "2021-01-05T00:00:00Z".to_string(),
            end_time: String::new(),
            stats: cf_quality::TickStats {
                complexity_p95: p95,
                files_analyzed: 1,
                ..cf_quality::TickStats::default()
            },
        };
        let metrics = cf_quality::ComputedMetrics {
            time_series: vec![entry(0, 1.0), entry(4, 7.5), entry(9, 7.5)],
            aggregate: cf_quality::AggregateData {
                total_ticks: 3,
                total_files_analyzed: 3,
                ..cf_quality::AggregateData::default()
            },
        };
        let text = render_quality(&metrics, PLAIN);
        assert!(text.contains("Files Analyzed             3"), "{text}");
        let rows: Vec<&str> = text
            .lines()
            .filter(|l| l.trim_start().starts_with("tick "))
            .collect();
        assert_eq!(rows.len(), 3, "{text}");
        assert!(
            rows[0].contains("tick 4 ") && rows[0].contains("2021-01-05"),
            "{text}"
        );
        assert!(rows[1].contains("tick 9 "), "{text}");
        assert!(rows[2].contains("tick 0 "), "{text}");
    }
}
