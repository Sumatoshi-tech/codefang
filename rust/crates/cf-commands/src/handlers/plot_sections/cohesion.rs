//! `static/cohesion` plot sections.
//!
//! Consumes the AGGREGATED RAW cohesion report (the `analyze.Report` value
//! `cohesion_raw_report_value` builds): the score histogram, the distribution
//! pie, and the per-directory box plot. Returns `None` when the report lacks
//! the functions table (the reference `ErrInvalidFunctions` — the empty-result report).

use cf_gojson::{GoMap, GoValue};
use cf_plotpage::echarts::{
    AxisLabel, AxisLine, BarData, BoxPlotData, Chart, ChartKind, ItemStyle, LineStyle, SplitLine,
    XAxis, YAxis,
};
use cf_plotpage::{build_pie_chart, get_chart_palette, ChartOpts, Hint, Section, Theme};

use crate::handlers::go_sort;

/// Reference plot-section constants.
const EMPTY_CHART_HEIGHT: &str = "400px";
const PIE_RADIUS: &str = "60%";
const HISTOGRAM_BINS: usize = 10;
const MIDPOINT_FACTOR: f64 = 0.5;
const MIN_GROUP_SIZE: usize = 3;
const MAX_DIRECTORIES: usize = 15;
const MAX_PATH_COMPONENTS: usize = 3;
const BOX_PLOT_LABEL_ROTATE: f64 = 30.0;

/// Plot display labels for the cohesion distribution pie.
const PLOT_LABEL_EXCELLENT: &str = "Excellent";
const PLOT_LABEL_GOOD: &str = "Good";
const PLOT_LABEL_FAIR: &str = "Fair";
const PLOT_LABEL_POOR: &str = "Poor";

/// Distribution thresholds (exported reference constants).
const DIST_EXCELLENT_MIN: f64 = 0.6;
const DIST_GOOD_MIN: f64 = 0.4;
const DIST_FAIR_MIN: f64 = 0.3;

/// Percentile points.
const P_Q1: f64 = 0.25;
const P_MEDIAN: f64 = 0.50;
const P_Q3: f64 = 0.75;

/// The reference `analyze.ReportFunctionListWithFallback(report, "functions",
/// "function_cohesion")`: the function maps under either key.
fn report_function_list(report: &GoValue) -> Option<Vec<&GoMap>> {
    let top = report.as_map()?;
    let arr = match top.get("functions") {
        Some(GoValue::Array(items)) => items,
        _ => match top.get("function_cohesion") {
            Some(GoValue::Array(items)) => items,
            _ => return None,
        },
    };
    Some(arr.iter().filter_map(GoValue::as_map).collect())
}

/// The reference `getCohesionValue`: `fn["cohesion"].(float64)`, else 0.
fn cohesion_value(f: &GoMap) -> f64 {
    match f.get("cohesion") {
        Some(GoValue::Float(v)) => *v,
        _ => 0.0,
    }
}

/// The reference `getCohesionColor`.
fn cohesion_color(cohesion: f64) -> &'static str {
    if cohesion >= DIST_EXCELLENT_MIN {
        "#91cc75"
    } else if cohesion >= DIST_GOOD_MIN {
        "#fac858"
    } else if cohesion >= DIST_FAIR_MIN {
        "#fd8c73"
    } else {
        "#ee6666"
    }
}

/// The registered section renderer for `static/cohesion` — the reference implementation
/// `cohesion.RegisterPlotSections` → `(&Analyzer{}).generateSections`.
pub fn sections(report: &GoValue) -> Option<Vec<Section>> {
    let histogram = histogram_chart(report)?;
    let pie = pie_chart(report);
    let box_plot = box_plot_chart(report);

    Some(vec![
        Section::new(
            "Cohesion Score Distribution",
            "Number of functions in each cohesion score range.",
            Box::new(histogram),
            Hint {
                title: "How to interpret:".to_string(),
                items: vec![
                    "<strong>Left side (low scores)</strong> = functions with poor cohesion — refactoring candidates".to_string(),
                    "<strong>Right side (high scores)</strong> = functions with good cohesion — well-structured".to_string(),
                    "<strong>Red zone</strong> (< 0.3) = Poor — function is isolated, consider splitting".to_string(),
                    "<strong>Green zone</strong> (≥ 0.6) = Excellent — function shares most variables with the module".to_string(),
                    "<strong>Healthy codebase:</strong> most functions should cluster on the right side".to_string(),
                ],
            },
        ),
        Section::new(
            "Cohesion Distribution",
            "Distribution of functions by cohesion category.",
            Box::new(pie),
            Hint {
                title: "How to interpret:".to_string(),
                items: vec![
                    "<strong>Excellent</strong> = Functions with cohesion ≥ 0.6".to_string(),
                    "<strong>Good</strong> = Functions with cohesion 0.4-0.6".to_string(),
                    "<strong>Fair</strong> = Functions with cohesion 0.3-0.4".to_string(),
                    "<strong>Poor</strong> = Functions with cohesion < 0.3".to_string(),
                    "<strong>Goal:</strong> Maximize the Excellent and Good segments".to_string(),
                ],
            },
        ),
        Section::new(
            "Cohesion by Package",
            "Box plot showing cohesion score distribution per directory.",
            Box::new(box_plot),
            Hint {
                title: "How to interpret:".to_string(),
                items: vec![
                    "<strong>Box</strong> = Middle 50% of scores (interquartile range)".to_string(),
                    "<strong>Line inside box</strong> = Median cohesion score for the package".to_string(),
                    "<strong>Whiskers</strong> = Min and max cohesion scores in the package".to_string(),
                    "<strong>Sorted left-to-right</strong> by median (worst packages first)".to_string(),
                    "<strong>Goal:</strong> All boxes should cluster above 0.5".to_string(),
                ],
            },
        ),
    ])
}

/// The reference `generateHistogram` + `binScores` + `createHistogramChart`.
fn histogram_chart(report: &GoValue) -> Option<Chart> {
    let functions = report_function_list(report)?;
    if functions.is_empty() {
        return Some(empty_cohesion_chart());
    }

    // binScores: equal-width bins over [0,1].
    let bin_width = 1.0 / HISTOGRAM_BINS as f64;
    let mut counts = [0i64; HISTOGRAM_BINS];
    for f in &functions {
        let s = cohesion_value(f);
        let mut idx = (s / bin_width) as i64 as usize;
        if idx >= HISTOGRAM_BINS {
            idx = HISTOGRAM_BINS - 1;
        }
        counts[idx] += 1;
    }

    let co = ChartOpts::default_dark();
    let mut bar = Chart::new(ChartKind::Bar);
    let (w, h, bg, theme) = co.init("100%", "500px");
    bar.set_init(&w, &h, &bg, &theme);
    bar.tooltip = co.tooltip("axis");
    bar.grid = vec![co.grid()];
    bar.x_axis = XAxis {
        name: "Cohesion Score".to_string(),
        axis_label: Some(AxisLabel {
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
    bar.y_axis = YAxis {
        name: "Number of Functions".to_string(),
        axis_label: Some(AxisLabel {
            color: co.text_muted_color().to_string(),
            ..AxisLabel::default()
        }),
        split_line: Some(SplitLine {
            line_style: Some(LineStyle {
                color: co.grid_color().to_string(),
                ..LineStyle::default()
            }),
            ..SplitLine::default()
        }),
        ..YAxis::default()
    };

    let mut labels = Vec::with_capacity(HISTOGRAM_BINS);
    let mut bar_data = Vec::with_capacity(HISTOGRAM_BINS);
    for (i, count) in counts.iter().enumerate() {
        let lo = i as f64 * bin_width;
        let hi = lo + bin_width;
        let mid = lo + bin_width * MIDPOINT_FACTOR;
        labels.push(format!("{lo:.1}–{hi:.1}"));
        bar_data.push(
            BarData {
                value: Some(GoValue::Int(*count)),
                item_style: Some(ItemStyle {
                    color: cohesion_color(mid).to_string(),
                    ..ItemStyle::default()
                }),
                ..BarData::default()
            }
            .value(),
        );
    }
    bar.set_x_axis_labels(&labels);
    bar.add_series("Functions", GoValue::Array(bar_data));

    Some(bar)
}

/// The reference `generatePieChart` + `createCohesionPieChart`.
fn pie_chart(report: &GoValue) -> Chart {
    let Some(functions) = report_function_list(report) else {
        return empty_pie_chart();
    };
    if functions.is_empty() {
        return empty_pie_chart();
    }

    // stats.Distribution over classifyCohesionForPlot.
    let mut excellent = 0i64;
    let mut good = 0i64;
    let mut fair = 0i64;
    let mut poor = 0i64;
    for f in &functions {
        let c = cohesion_value(f);
        if c >= DIST_EXCELLENT_MIN {
            excellent += 1;
        } else if c >= DIST_GOOD_MIN {
            good += 1;
        } else if c >= DIST_FAIR_MIN {
            fair += 1;
        } else {
            poor += 1;
        }
    }

    let palette = get_chart_palette(Theme::Dark);
    let item = |name: &str, value: i64, color: &str| cf_plotpage::echarts::PieData {
        name: name.to_string(),
        value: Some(GoValue::Int(value)),
        item_style: Some(ItemStyle {
            color: color.to_string(),
            ..ItemStyle::default()
        }),
        ..cf_plotpage::echarts::PieData::default()
    };
    let pie_data = vec![
        item(PLOT_LABEL_EXCELLENT, excellent, palette.semantic.good),
        item(PLOT_LABEL_GOOD, good, palette.semantic.warning),
        item(PLOT_LABEL_FAIR, fair, "#fd8c73"),
        item(PLOT_LABEL_POOR, poor, palette.semantic.bad),
    ];

    build_pie_chart(None, "Cohesion", pie_data, PIE_RADIUS)
}

/// One directory group: label + ascending scores.
struct DirectoryGroup {
    label: String,
    scores: Vec<f64>,
}

/// The reference `generateBoxPlot` + `groupByDirectory` + `buildBoxPlotChart`
///.
fn box_plot_chart(report: &GoValue) -> Chart {
    let Some(functions) = report_function_list(report) else {
        return empty_box_plot();
    };
    if functions.is_empty() {
        return empty_box_plot();
    }

    // groupByDirectory: group scores by the shortened source directory. the reference implementation
    // iterates the grouped map in RANDOM order before the median sort; we keep
    // first-seen order (ties between equal medians are nondeterministic in the reference binary and
    // measured by the harness).
    let mut order: Vec<String> = Vec::new();
    let mut grouped: std::collections::HashMap<String, Vec<f64>> =
        std::collections::HashMap::new();
    for f in &functions {
        let file_path = match f.get("_source_file") {
            Some(GoValue::Str(s)) if !s.is_empty() => s.as_str(),
            _ => continue,
        };
        let dir = shorten_directory(&go_filepath_dir(file_path));
        if !grouped.contains_key(&dir) {
            order.push(dir.clone());
        }
        grouped.entry(dir).or_default().push(cohesion_value(f));
    }

    let mut groups: Vec<DirectoryGroup> = Vec::new();
    for dir in order {
        let mut scores = grouped.remove(&dir).expect("group present");
        if scores.len() < MIN_GROUP_SIZE {
            continue;
        }
        scores.sort_by(|a, b| a.partial_cmp(b).expect("finite cohesion scores"));
        groups.push(DirectoryGroup { label: dir, scores });
    }
    if groups.is_empty() {
        return empty_box_plot();
    }

    // sort.Slice by median ascending (reference: pdqsort; equal-median ties measured).
    go_sort::slice(&mut groups, |a, b| {
        percentile(&a.scores, P_MEDIAN) < percentile(&b.scores, P_MEDIAN)
    });
    groups.truncate(MAX_DIRECTORIES);

    let co = ChartOpts::default_dark();
    let mut bp = Chart::new(ChartKind::BoxPlot);
    let (w, h, bg, theme) = co.init("100%", "500px");
    bp.set_init(&w, &h, &bg, &theme);
    bp.tooltip = co.tooltip("item");
    bp.grid = vec![co.grid()];
    bp.x_axis = XAxis {
        name: "Package / Directory".to_string(),
        axis_label: Some(AxisLabel {
            color: co.text_muted_color().to_string(),
            rotate: BOX_PLOT_LABEL_ROTATE,
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
    bp.y_axis = YAxis {
        name: "Cohesion Score".to_string(),
        min: Some(GoValue::Int(0)),
        max: Some(GoValue::Float(1.0)),
        axis_label: Some(AxisLabel {
            color: co.text_muted_color().to_string(),
            ..AxisLabel::default()
        }),
        split_line: Some(SplitLine {
            line_style: Some(LineStyle {
                color: co.grid_color().to_string(),
                ..LineStyle::default()
            }),
            ..SplitLine::default()
        }),
        ..YAxis::default()
    };

    let labels: Vec<String> = groups.iter().map(|g| g.label.clone()).collect();
    bp.set_x_axis_labels(&labels);

    let data = GoValue::Array(
        groups
            .iter()
            .map(|g| {
                let bs = box_stats(&g.scores);
                BoxPlotData {
                    name: g.label.clone(),
                    value: Some(GoValue::Array(bs.iter().map(|v| GoValue::Float(*v)).collect())),
                }
                .value()
            })
            .collect(),
    );
    bp.add_series("Cohesion", data);

    bp
}

/// The reference `boxStats`: `[min, Q1, median, Q3, max]` over a sorted slice.
fn box_stats(sorted: &[f64]) -> [f64; 5] {
    if sorted.is_empty() {
        return [0.0; 5];
    }
    [
        sorted[0],
        percentile(sorted, P_Q1),
        percentile(sorted, P_MEDIAN),
        percentile(sorted, P_Q3),
        sorted[sorted.len() - 1],
    ]
}

/// The reference `stats.Percentile`: linear interpolation
/// over the sorted values (callers pass pre-sorted slices).
fn percentile(sorted: &[f64], p: f64) -> f64 {
    let count = sorted.len();
    if count == 0 {
        return 0.0;
    }
    let idx = p * (count - 1) as f64;
    let lower = idx.floor() as usize;
    let upper = idx.ceil() as usize;
    if lower == upper || upper >= count {
        return sorted[lower];
    }
    let frac = idx - lower as f64;
    sorted[lower] * (1.0 - frac) + sorted[upper] * frac
}

/// The reference `filepath.Dir` over the clean relative paths the report stamps.
fn go_filepath_dir(path: &str) -> String {
    match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(pos) => path[..pos].to_string(),
        None => ".".to_string(),
    }
}

/// The reference `shortenDirectory`: the last `maxPathComponents` non-empty slash
/// components.
fn shorten_directory(dir: &str) -> String {
    let parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    let start = parts.len().saturating_sub(MAX_PATH_COMPONENTS);
    parts[start..].join("/")
}

/// The reference `createEmptyCohesionChart`.
fn empty_cohesion_chart() -> Chart {
    let co = ChartOpts::default_dark();
    let mut bar = Chart::new(ChartKind::Bar);
    let (w, h, bg, theme) = co.init("100%", EMPTY_CHART_HEIGHT);
    bar.set_init(&w, &h, &bg, &theme);
    bar.title = co.title("Function Cohesion", "No data");
    bar
}

/// The reference `createEmptyPieChart`.
fn empty_pie_chart() -> Chart {
    let co = ChartOpts::default_dark();
    let mut pie = Chart::new(ChartKind::Pie);
    let (w, h, bg, theme) = co.init("600px", EMPTY_CHART_HEIGHT);
    pie.set_init(&w, &h, &bg, &theme);
    pie.title = co.title("Cohesion Distribution", "No data");
    pie
}

/// The reference `createEmptyBoxPlot`.
fn empty_box_plot() -> Chart {
    let co = ChartOpts::default_dark();
    let mut bp = Chart::new(ChartKind::BoxPlot);
    let (w, h, bg, theme) = co.init("100%", EMPTY_CHART_HEIGHT);
    bp.set_init(&w, &h, &bg, &theme);
    bp.title = co.title("Cohesion by Package", "No package data available");
    bp
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_gojson::MapOrigin;

    fn raw_fn(name: &str, sf: &str, cohesion: f64) -> GoValue {
        let mut m = GoMap::new(MapOrigin::Map);
        m.push("name", GoValue::Str(name.to_string()));
        m.push("cohesion", GoValue::Float(cohesion));
        m.push("_source_file", GoValue::Str(sf.to_string()));
        GoValue::Map(m)
    }

    fn raw_report() -> GoValue {
        let mut m = GoMap::new(MapOrigin::Map);
        m.push(
            "functions",
            GoValue::Array(vec![
                raw_fn("a", "pkg/x.go", 1.0),
                raw_fn("b", "pkg/y.go", 0.45),
                raw_fn("c", "pkg/z.go", 0.2),
                raw_fn("d", "other.go", 0.35),
            ]),
        );
        GoValue::Map(m)
    }

    #[test]
    fn sections_carry_go_titles() {
        let secs = sections(&raw_report()).expect("sections");
        assert_eq!(secs.len(), 3);
        assert_eq!(secs[0].title, "Cohesion Score Distribution");
        assert_eq!(secs[1].title, "Cohesion Distribution");
        assert_eq!(secs[2].title, "Cohesion by Package");
    }

    #[test]
    fn missing_functions_key_skips_page() {
        let m = GoMap::new(MapOrigin::Map);
        assert!(sections(&GoValue::Map(m)).is_none());
    }

    #[test]
    fn histogram_bins_and_colors_match_go() {
        let json = histogram_chart(&raw_report()).expect("histogram").option_json();
        // En-dash labels in xAxis data.
        assert!(json.contains("\"data\":[\"0.0–0.1\",\"0.1–0.2\",\"0.2–0.3\",\"0.3–0.4\",\"0.4–0.5\",\"0.5–0.6\",\"0.6–0.7\",\"0.7–0.8\",\"0.8–0.9\",\"0.9–1.0\"]"));
        // Bin colors keyed by midpoint: 0.2-0.3 mid 0.25 → poor red; 0.3-0.4 mid
        // 0.35 → fair; 0.4-0.5 mid 0.45 → good; 0.9-1.0 mid 0.95 → excellent.
        assert!(json.contains("{\"value\":1,\"itemStyle\":{\"color\":\"#ee6666\"}},{\"value\":1,\"itemStyle\":{\"color\":\"#fd8c73\"}},{\"value\":1,\"itemStyle\":{\"color\":\"#fac858\"}}"));
        assert!(json.contains("\"xAxis\":[{\"name\":\"Cohesion Score\","));
        // YAxis without splitLine.show (unlike the co.YAxis preset).
        assert!(json.contains("\"yAxis\":[{\"name\":\"Number of Functions\",\"splitLine\":{\"lineStyle\":{\"color\":\"#44403c\"}}"));
    }

    #[test]
    fn box_plot_min_max_and_stats_match_go() {
        let json = box_plot_chart(&raw_report()).option_json();
        // The pkg directory has 3 scores → one group; min 0, max 1 axis bounds.
        assert!(json.contains("\"min\":0,\"max\":1"));
        assert!(json.contains("\"type\":\"boxplot\""));
        // boxStats over sorted [0.2,0.45,1]: Q1 = 0.325, median 0.45, Q3 0.725.
        assert!(json.contains("{\"name\":\"pkg\",\"value\":[0.2,0.325,0.45,0.725,1]}"));
        assert!(json.contains("\"axisLabel\":{\"rotate\":30,"));
    }

    #[test]
    fn pie_distribution_counts_match_go() {
        let json = pie_chart(&raw_report()).option_json();
        assert!(json.contains("{\"name\":\"Excellent\",\"value\":1,\"itemStyle\":{\"color\":\"#22c55e\"}},{\"name\":\"Good\",\"value\":1,\"itemStyle\":{\"color\":\"#eab308\"}},{\"name\":\"Fair\",\"value\":1,\"itemStyle\":{\"color\":\"#fd8c73\"}},{\"name\":\"Poor\",\"value\":1,\"itemStyle\":{\"color\":\"#ef4444\"}}"));
    }
}
