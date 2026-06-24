//! `history/file-history` plot sections.
//! (`GenerateStoreSections` → `buildStoreSections` over the `file_churn` and
//! `composition` store kinds, which the run's `ComputedMetrics` carries as
//! `file_churn` / `composition_ts`).

use cf_file_history::{CompositionTimeSeriesEntry, FileChurnData};
use cf_plotpage::echarts::{AxisLabel, AxisLine, LineStyle, XAxis};
use cf_plotpage::{
    build_bar_chart, build_line_chart, get_chart_palette, BarSeries, Chart, ChartOpts, Hint,
    LineSeries, Section, SeriesValue, Theme,
};

use crate::handlers::go_sort;

/// file_Reference history plot-section constants.
const TOP_FILES_LIMIT: usize = 20;
const X_AXIS_ROTATE: f64 = 60.0;
const COMPOSITION_AREA_ALPHA: f64 = 0.5;

/// The reference `AllCategories` order (file_history `Category` constants) with the
/// per-category chart colors (`categoryColors`).
const CATEGORY_COLORS: &[(&str, &str)] = &[
    ("source", "#4CAF50"),
    ("documentation", "#2196F3"),
    ("configuration", "#FF9800"),
    ("vendor", "#9C27B0"),
    ("generated", "#607D8B"),
    ("dotfile", "#795548"),
    ("image", "#E91E63"),
    ("binary", "#F44336"),
];

/// The reference `GenerateStoreSections` → `buildStoreSections`: zero churn AND zero
/// composition yield zero sections.
pub fn sections(
    file_churn: &[FileChurnData],
    composition_ts: &[CompositionTimeSeriesEntry],
) -> Vec<Section> {
    if file_churn.is_empty() && composition_ts.is_empty() {
        return Vec::new();
    }

    let mut sections = Vec::new();

    if !file_churn.is_empty() {
        sections.push(Section {
            title: "Most Modified Files".to_string(),
            subtitle: "Files ranked by total number of commits touching them.".to_string(),
            chart: Some(Box::new(build_bar_chart_from_churn(file_churn))),
            hint: Hint {
                title: "How to interpret:".to_string(),
                items: vec![
                    "Tall bars = frequently modified files (high churn)".to_string(),
                    "Configuration files = expected to change often".to_string(),
                    "Core business logic = may indicate instability or active development"
                        .to_string(),
                    "Look for: Files changing too frequently that should be stable".to_string(),
                    "Action: High-churn files benefit from better test coverage".to_string(),
                ],
            },
        });
    }

    if let Some(chart) = build_composition_chart_from_ts(composition_ts) {
        sections.push(Section {
            title: "File Composition Over Time".to_string(),
            subtitle: "Distribution of changed files by category across analysis ticks."
                .to_string(),
            chart: Some(Box::new(chart)),
            hint: Hint {
                title: "Categories:".to_string(),
                items: vec![
                    "Source = project code (first-party)".to_string(),
                    "Documentation = docs, README, LICENSE, examples".to_string(),
                    "Configuration = YAML, JSON, TOML, XML, Makefile".to_string(),
                    "Vendor = third-party dependencies (node_modules, vendor/)".to_string(),
                    "Generated = protobuf, code generators, minified bundles".to_string(),
                    "DotFile = .gitignore, .editorconfig, etc.".to_string(),
                    "Image = PNG, JPG, GIF".to_string(),
                    "Binary = files with binary content".to_string(),
                ],
            },
        });
    }

    sections
}

/// The reference `buildBarChartFromChurnData`: re-sort (the reference `sort.Slice`, commit-count
/// descending over the churn-score-sorted input), cut to the top 20, one
/// "Commits" series, rotated X labels.
fn build_bar_chart_from_churn(file_churn: &[FileChurnData]) -> Chart {
    let mut churn: Vec<&FileChurnData> = file_churn.iter().collect();
    go_sort::slice(&mut churn, |a, b| a.commit_count > b.commit_count);
    let limit = churn.len().min(TOP_FILES_LIMIT);
    let top = &churn[..limit];

    let labels: Vec<String> = top.iter().map(|c| c.path.clone()).collect();
    let series = vec![BarSeries {
        name: "Commits".to_string(),
        data: top
            .iter()
            .map(|c| SeriesValue::Int(c.commit_count))
            .collect(),
        color: get_chart_palette(Theme::Dark).semantic.bad.to_string(),
        ..BarSeries::default()
    }];

    let co = ChartOpts::default_dark();
    let mut chart = build_bar_chart(Some(&co), &labels, &series, "Commits");

    // The reference implementation's follow-up WithXAxisOpts replaces the X axis with the rotated-label
    // variant (no name, interval "0").
    chart.x_axis = XAxis {
        axis_label: Some(AxisLabel {
            rotate: X_AXIS_ROTATE,
            interval: "0".to_string(),
            color: co.text_muted_color().to_string(),
            ..AxisLabel::default()
        }),
        axis_line: Some(AxisLine {
            line_style: Some(LineStyle {
                color: co.axis_color().to_string(),
                ..LineStyle::default()
            }),
            ..AxisLine::default()
        }),
        ..XAxis::default()
    };

    chart
}

/// The reference `buildCompositionChartFromTS`: per-category stacked area line over the
/// pre-computed time series; categories with no positive values are skipped;
/// `None` when nothing plots.
fn build_composition_chart_from_ts(ts: &[CompositionTimeSeriesEntry]) -> Option<Chart> {
    if ts.is_empty() {
        return None;
    }

    let labels: Vec<String> = ts.iter().map(|e| format!("Tick {}", e.tick)).collect();

    let mut series: Vec<LineSeries> = Vec::new();
    for (category, color) in CATEGORY_COLORS {
        let mut data: Vec<SeriesValue> = Vec::with_capacity(ts.len());
        let mut has_data = false;
        for entry in ts {
            let v = entry.breakdown.get(*category).copied().unwrap_or(0);
            data.push(SeriesValue::Int(v));
            if v > 0 {
                has_data = true;
            }
        }
        if !has_data {
            continue;
        }
        series.push(LineSeries {
            name: (*category).to_string(),
            data,
            color: (*color).to_string(),
            stack: "total".to_string(),
            area_opacity: COMPOSITION_AREA_ALPHA,
        });
    }

    if series.is_empty() {
        return None;
    }

    let co = ChartOpts::default_dark();
    Some(build_line_chart(Some(&co), &labels, &series, "Files"))
}
