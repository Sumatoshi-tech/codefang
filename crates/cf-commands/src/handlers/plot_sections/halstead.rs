//! `static/halstead` plot sections.
//!
//! Consumes the AGGREGATED RAW halstead report (the `analyze.Report` value
//! `halstead_raw_report_value` builds): the top-effort bar, the
//! volume-vs-difficulty risk scatter, and the volume-distribution pie. Returns
//! `None` when the report lacks the functions table (reference:
//! `ErrInvalidFunctionsData` — the empty-result report).

use cf_gojson::{GoMap, GoValue};
use cf_plotpage::echarts::{
    AxisLabel, AxisLine, BarData, Chart, ChartKind, ItemStyle, LineStyle, ScatterData, SplitLine,
    XAxis, YAxis,
};
use cf_plotpage::{build_pie_chart, get_chart_palette, ChartOpts, Hint, Section, Theme};

use crate::handlers::go_sort;

/// Reference plot-section constants.
const TOP_FUNCTIONS_LIMIT: usize = 12;
const X_AXIS_ROTATE: f64 = 45.0;
const EMPTY_CHART_HEIGHT: &str = "400px";
const PIE_RADIUS: &str = "60%";
const SCATTER_SYMBOL_SIZE: i64 = 12;
const MAX_SYMBOL_SIZE: i64 = 45;
const BUGS_MULTIPLIER: f64 = 10.0;
const VOLUME_LOW: f64 = 100.0;
const VOLUME_MEDIUM: f64 = 1000.0;
const VOLUME_HIGH: f64 = 5000.0;
const EFFORT_LOW: f64 = 1000.0;
const EFFORT_MEDIUM: f64 = 10000.0;
const DIFFICULTY_MEDIUM: f64 = 15.0;
const DIFFICULTY_HIGH: f64 = 30.0;

/// The reference `analyze.ReportFunctionListWithFallback(report, "functions",
/// "function_halstead")`.
fn report_function_list(report: &GoValue) -> Option<Vec<&GoMap>> {
    let top = report.as_map()?;
    let arr = match top.get("functions") {
        Some(GoValue::Array(items)) => items,
        _ => match top.get("function_halstead") {
            Some(GoValue::Array(items)) => items,
            _ => return None,
        },
    };
    Some(arr.iter().filter_map(GoValue::as_map).collect())
}

/// The reference `getEffortValue` / `getVolumeValue` / … : `fn[key].(float64)`, else 0.
fn float_value(f: &GoMap, key: &str) -> f64 {
    match f.get(key) {
        Some(GoValue::Float(v)) => *v,
        _ => 0.0,
    }
}

/// Reference function name lookup: `name` string, else `"unknown"`.
fn function_name(f: &GoMap) -> &str {
    match f.get("name") {
        Some(GoValue::Str(name)) => name,
        _ => "unknown",
    }
}

/// The reference `getEffortColor`.
fn effort_color(effort: f64) -> &'static str {
    if effort <= EFFORT_LOW {
        "#91cc75"
    } else if effort <= EFFORT_MEDIUM {
        "#fac858"
    } else {
        "#ee6666"
    }
}

/// The registered section renderer for `static/halstead` — the reference implementation
/// `halstead.RegisterPlotSections` → `(&Analyzer{}).generateSections`.
pub fn sections(report: &GoValue) -> Option<Vec<Section>> {
    let effort_chart = effort_bar_chart(report)?;
    let scatter_chart = volume_vs_difficulty_chart(report)?;
    let pie_chart = volume_pie_chart(report);

    Some(vec![
        Section::new(
            "Top Functions by Effort",
            "Most expensive functions first; start review from the top.",
            Box::new(effort_chart),
            Hint {
                title: "How to interpret:".to_string(),
                items: vec![
                    "<strong>Effort</strong> = Volume × Difficulty (higher means harder to maintain)".to_string(),
                    "<strong>Green</strong> = monitor, <strong>Yellow</strong> = schedule cleanup, <strong>Red</strong> = refactor now".to_string(),
                    "<strong>Tip:</strong> Start with red bars to reduce risk fastest".to_string(),
                ],
            },
        ),
        Section::new(
            "Volume vs Difficulty",
            "Risk map by size (x), difficulty (y), and bug estimate (bubble size).",
            Box::new(scatter_chart),
            Hint {
                title: "How to interpret:".to_string(),
                items: vec![
                    "<strong>Bottom-left</strong> points are healthiest".to_string(),
                    "<strong>Top-right</strong> points are highest risk".to_string(),
                    "<strong>Bubble size</strong> reflects estimated bugs".to_string(),
                    "<strong>Color</strong> reflects risk zone (green/yellow/red)".to_string(),
                ],
            },
        ),
        Section::new(
            "Volume Distribution",
            "Portfolio split by Halstead volume buckets.",
            Box::new(pie_chart),
            Hint {
                title: "How to interpret:".to_string(),
                items: vec![
                    "<strong>Low (≤100)</strong> = usually easy to maintain".to_string(),
                    "<strong>High / Very High</strong> concentration means decomposition debt".to_string(),
                ],
            },
        ),
    ])
}

/// The reference `generateEffortBarChart` + `createEffortBarChart`.
fn effort_bar_chart(report: &GoValue) -> Option<Chart> {
    let functions = report_function_list(report)?;
    if functions.is_empty() {
        return Some(empty_halstead_chart());
    }

    // mapx.SortAndLimit by effort desc (unstable sort.Slice over the walk-order
    // input — go_sort reproduces the tie permutation).
    let mut sorted = functions.clone();
    go_sort::slice(&mut sorted, |a, b| {
        float_value(a, "effort") > float_value(b, "effort")
    });
    sorted.truncate(TOP_FUNCTIONS_LIMIT);

    let co = ChartOpts::default_dark();
    let mut bar = Chart::new(ChartKind::Bar);
    let (w, h, bg, theme) = co.init("100%", "500px");
    bar.set_init(&w, &h, &bg, &theme);
    bar.tooltip = co.tooltip("axis");
    bar.grid = vec![co.grid()];
    bar.data_zoom = co.data_zoom();
    bar.x_axis = XAxis {
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
    bar.y_axis = co.y_axis("Effort");

    let labels: Vec<String> = sorted.iter().map(|f| function_name(f).to_string()).collect();
    bar.set_x_axis_labels(&labels);

    let bar_data = GoValue::Array(
        sorted
            .iter()
            .map(|f| {
                let effort = float_value(f, "effort");
                BarData {
                    value: Some(GoValue::Float(effort)),
                    item_style: Some(ItemStyle {
                        color: effort_color(effort).to_string(),
                        ..ItemStyle::default()
                    }),
                    ..BarData::default()
                }
                .value()
            })
            .collect(),
    );
    bar.add_series("Effort", bar_data);

    Some(bar)
}

/// Scatter risk zones (reference `classifyScatterRisk`).
fn classify_scatter_risk(volume: f64, difficulty: f64, bugs: f64) -> u8 {
    if volume >= VOLUME_HIGH || difficulty >= DIFFICULTY_HIGH || bugs >= 1.0 {
        2 // riskHigh
    } else if volume >= VOLUME_MEDIUM || difficulty >= DIFFICULTY_MEDIUM || bugs >= 0.3 {
        1 // riskMedium
    } else {
        0 // riskLow
    }
}

/// The reference `generateVolumeVsDifficultyChart` + `createVolumeVsDifficultyChart`
///.
fn volume_vs_difficulty_chart(report: &GoValue) -> Option<Chart> {
    let functions = report_function_list(report)?;
    if functions.is_empty() {
        return Some(empty_halstead_scatter());
    }

    let co = ChartOpts::default_dark();
    let palette = get_chart_palette(Theme::Dark);

    let mut scatter = Chart::new(ChartKind::Scatter);
    let (w, h, bg, theme) = co.init("100%", "500px");
    scatter.set_init(&w, &h, &bg, &theme);
    scatter.tooltip = co.tooltip("item");
    scatter.x_axis = XAxis {
        name: "Volume".to_string(),
        type_: "value".to_string(),
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
    scatter.y_axis = YAxis {
        name: "Difficulty".to_string(),
        type_: "value".to_string(),
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
    scatter.grid = vec![co.grid()];

    let mut low_risk: Vec<GoValue> = Vec::new();
    let mut medium_risk: Vec<GoValue> = Vec::new();
    let mut high_risk: Vec<GoValue> = Vec::new();
    for f in &functions {
        let volume = float_value(f, "volume");
        let difficulty = float_value(f, "difficulty");
        let bugs = float_value(f, "delivered_bugs");
        let name = function_name(f);

        // min(scatterSymbolSize + int(bugs*bugsMultiplier), maxSymbolSize).
        let symbol_size = (SCATTER_SYMBOL_SIZE + (bugs * BUGS_MULTIPLIER) as i64)
            .min(MAX_SYMBOL_SIZE);
        let point = ScatterData {
            value: Some(GoValue::Array(vec![
                GoValue::Float(volume),
                GoValue::Float(difficulty),
                GoValue::Str(name.to_string()),
            ])),
            symbol_size,
            ..ScatterData::default()
        }
        .value();

        match classify_scatter_risk(volume, difficulty, bugs) {
            2 => high_risk.push(point),
            1 => medium_risk.push(point),
            _ => low_risk.push(point),
        }
    }

    let mut add_risk_series = |name: &str, data: Vec<GoValue>, color: &str| {
        if data.is_empty() {
            return;
        }
        let series = scatter.add_series(name, GoValue::Array(data));
        series.item_style = Some(ItemStyle {
            color: color.to_string(),
            ..ItemStyle::default()
        });
    };
    add_risk_series("Low risk", low_risk, palette.semantic.good);
    add_risk_series("Medium risk", medium_risk, palette.semantic.warning);
    add_risk_series("High risk", high_risk, palette.semantic.bad);

    Some(scatter)
}

/// The reference `generateVolumePieChart` + `createVolumeDistributionPie`.
fn volume_pie_chart(report: &GoValue) -> Chart {
    let Some(functions) = report_function_list(report) else {
        return empty_halstead_pie();
    };
    if functions.is_empty() {
        return empty_halstead_pie();
    }

    let (mut low, mut medium, mut high, mut very_high) = (0i64, 0i64, 0i64, 0i64);
    for f in &functions {
        let volume = float_value(f, "volume");
        if volume <= VOLUME_LOW {
            low += 1;
        } else if volume <= VOLUME_MEDIUM {
            medium += 1;
        } else if volume <= VOLUME_HIGH {
            high += 1;
        } else {
            very_high += 1;
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
        item("Low (≤100)", low, palette.semantic.good),
        item("Medium (101-1000)", medium, palette.primary[1]),
        item("High (1001-5000)", high, palette.semantic.warning),
        item("Very High (>5000)", very_high, palette.semantic.bad),
    ];

    build_pie_chart(None, "Volume", pie_data, PIE_RADIUS)
}

/// The reference `createEmptyHalsteadChart`.
fn empty_halstead_chart() -> Chart {
    let co = ChartOpts::default_dark();
    let mut bar = Chart::new(ChartKind::Bar);
    let (w, h, bg, theme) = co.init("100%", EMPTY_CHART_HEIGHT);
    bar.set_init(&w, &h, &bg, &theme);
    bar.title = co.title("Function Effort", "No data");
    bar
}

/// The reference `createEmptyHalsteadScatter`.
fn empty_halstead_scatter() -> Chart {
    let co = ChartOpts::default_dark();
    let mut scatter = Chart::new(ChartKind::Scatter);
    let (w, h, bg, theme) = co.init("100%", EMPTY_CHART_HEIGHT);
    scatter.set_init(&w, &h, &bg, &theme);
    scatter.title = co.title("Volume vs Difficulty", "No data");
    scatter
}

/// The reference `createEmptyHalsteadPie`.
fn empty_halstead_pie() -> Chart {
    let co = ChartOpts::default_dark();
    let mut pie = Chart::new(ChartKind::Pie);
    let (w, h, bg, theme) = co.init("600px", EMPTY_CHART_HEIGHT);
    pie.set_init(&w, &h, &bg, &theme);
    pie.title = co.title("Volume Distribution", "No data");
    pie
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_gojson::MapOrigin;

    fn raw_fn(name: &str, volume: f64, difficulty: f64, effort: f64, bugs: f64) -> GoValue {
        let mut m = GoMap::new(MapOrigin::Map);
        m.push("name", GoValue::Str(name.to_string()));
        m.push("volume", GoValue::Float(volume));
        m.push("difficulty", GoValue::Float(difficulty));
        m.push("effort", GoValue::Float(effort));
        m.push("delivered_bugs", GoValue::Float(bugs));
        GoValue::Map(m)
    }

    fn raw_report() -> GoValue {
        let mut m = GoMap::new(MapOrigin::Map);
        m.push(
            "functions",
            GoValue::Array(vec![
                raw_fn("small", 50.0, 2.0, 100.0, 0.01),
                raw_fn("big", 6000.0, 40.0, 240000.0, 2.0),
            ]),
        );
        GoValue::Map(m)
    }

    #[test]
    fn sections_carry_go_titles() {
        let secs = sections(&raw_report()).expect("sections");
        assert_eq!(secs.len(), 3);
        assert_eq!(secs[0].title, "Top Functions by Effort");
        assert_eq!(secs[1].title, "Volume vs Difficulty");
        assert_eq!(secs[2].title, "Volume Distribution");
    }

    #[test]
    fn missing_functions_key_skips_page() {
        let m = GoMap::new(MapOrigin::Map);
        assert!(sections(&GoValue::Map(m)).is_none());
    }

    #[test]
    fn bar_sorted_by_effort_desc_with_threshold_colors() {
        let json = effort_bar_chart(&raw_report()).expect("bar").option_json();
        assert!(json.contains("\"data\":[\"big\",\"small\"]"));
        assert!(json.contains(
            "{\"name\":\"Effort\",\"type\":\"bar\",\"data\":[{\"value\":240000,\"itemStyle\":{\"color\":\"#ee6666\"}},{\"value\":100,\"itemStyle\":{\"color\":\"#91cc75\"}}]}"
        ));
    }

    #[test]
    fn scatter_splits_risk_series_and_sizes_bubbles() {
        let json = volume_vs_difficulty_chart(&raw_report()).expect("scatter").option_json();
        // small → low risk (symbol 12); big → high risk (12 + 20 = 32).
        assert!(json.contains(
            "{\"name\":\"Low risk\",\"type\":\"scatter\",\"data\":[{\"value\":[50,2,\"small\"],\"symbolSize\":12}],\"itemStyle\":{\"color\":\"#22c55e\"}}"
        ));
        assert!(json.contains(
            "{\"name\":\"High risk\",\"type\":\"scatter\",\"data\":[{\"value\":[6000,40,\"big\"],\"symbolSize\":32}],\"itemStyle\":{\"color\":\"#ef4444\"}}"
        ));
        // No medium-risk series (empty bucket is skipped).
        assert!(!json.contains("Medium risk"));
    }

    #[test]
    fn pie_volume_buckets_match_go() {
        let json = volume_pie_chart(&raw_report()).option_json();
        assert!(json.contains(
            "{\"name\":\"Low (≤100)\",\"value\":1,\"itemStyle\":{\"color\":\"#22c55e\"}},{\"name\":\"Medium (101-1000)\",\"value\":0,\"itemStyle\":{\"color\":\"#38bdf8\"}},{\"name\":\"High (1001-5000)\",\"value\":0,\"itemStyle\":{\"color\":\"#eab308\"}},{\"name\":\"Very High (>5000)\",\"value\":1,\"itemStyle\":{\"color\":\"#ef4444\"}}"
        ));
    }
}
