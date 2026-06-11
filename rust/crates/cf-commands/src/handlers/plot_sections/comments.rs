//! `static/comments` plot sections — port of Go
//! `internal/analyzers/comments/plot.go`.
//!
//! Consumes the AGGREGATED RAW comments report (the `analyze.Report` value
//! `comments_raw_report_value` builds): the overall-score liquid gauge, the
//! per-function documentation bar, and the documentation-coverage pie. Returns
//! `None` when the report lacks the functions table (Go
//! `ErrInvalidFunctionsData` — the empty-result report).

use cf_gojson::{GoMap, GoValue};
use cf_plotpage::echarts::{
    AxisLabel, AxisLine, BarData, Chart, ChartKind, ItemStyle, LineStyle, LiquidData, XAxis,
};
use cf_plotpage::{build_pie_chart, get_chart_palette, ChartOpts, Hint, Section, Theme};

use crate::handlers::go_sort;

/// plot.go constants (plot.go:15).
const TOP_FUNCTIONS_LIMIT: usize = 20;
const X_AXIS_ROTATE: f64 = 45.0;
const EMPTY_CHART_HEIGHT: &str = "400px";
const PIE_RADIUS: &str = "60%";

/// Go `analyze.ReportFunctionListWithFallback(report, "functions",
/// "function_documentation")`.
fn report_function_list(report: &GoValue) -> Option<Vec<&GoMap>> {
    let top = report.as_map()?;
    let arr = match top.get("functions") {
        Some(GoValue::Array(items)) => items,
        _ => match top.get("function_documentation") {
            Some(GoValue::Array(items)) => items,
            _ => return None,
        },
    };
    Some(arr.iter().filter_map(GoValue::as_map).collect())
}

/// Go `reportValue` (plot.go:102): a top-level key, falling back to the
/// `aggregate` sub-map of binary-decoded reports.
fn report_value<'a>(report: &'a GoValue, key: &str) -> Option<&'a GoValue> {
    let top = report.as_map()?;
    if let Some(v) = top.get(key) {
        return Some(v);
    }
    top.get("aggregate")?.as_map()?.get(key)
}

/// Go `getLinesValue`: int passes through, float truncates, else 0.
fn lines_value(f: &GoMap) -> i64 {
    match f.get("lines") {
        Some(GoValue::Int(v)) => *v,
        Some(GoValue::Float(v)) => *v as i64,
        _ => 0,
    }
}

/// Go `isDocumented` (plot.go:147): the in-memory `assessment` string, then
/// the binary-decoded `is_documented` / `status` fallbacks.
fn is_documented(f: &GoMap) -> bool {
    if let Some(GoValue::Str(assessment)) = f.get("assessment") {
        return assessment == "✅ Well Documented";
    }
    if let Some(GoValue::Bool(documented)) = f.get("is_documented") {
        return *documented;
    }
    if let Some(GoValue::Str(status)) = f.get("status") {
        return status == "Well Documented";
    }
    false
}

/// Go `getFunctionName`: `function`, then `name`, then `"unknown"`.
fn function_name(f: &GoMap) -> &str {
    if let Some(GoValue::Str(name)) = f.get("function") {
        return name;
    }
    if let Some(GoValue::Str(name)) = f.get("name") {
        return name;
    }
    "unknown"
}

/// Go `reportValue`-read int (int or float64).
fn report_int(report: &GoValue, key: &str) -> i64 {
    match report_value(report, key) {
        Some(GoValue::Int(v)) => *v,
        Some(GoValue::Float(v)) => *v as i64,
        _ => 0,
    }
}

/// The registered section renderer for `static/comments` — Go
/// `comments.RegisterPlotSections` → `(&Analyzer{}).generateSections`.
pub fn sections(report: &GoValue) -> Option<Vec<Section>> {
    let bar_chart = function_coverage_chart(report)?;
    let pie_chart = documentation_pie_chart(report);
    let gauge_chart = overall_score_gauge(report);

    Some(vec![
        Section::new(
            "Overall Documentation Score",
            "Combined score based on comment quality and placement.",
            Box::new(gauge_chart),
            Hint {
                title: "How to interpret:".to_string(),
                items: vec![
                    "<strong>Green (≥80%)</strong> = Excellent documentation quality".to_string(),
                    "<strong>Yellow (60-80%)</strong> = Good quality with room for improvement".to_string(),
                    "<strong>Orange (40-60%)</strong> = Fair quality - improvements needed".to_string(),
                    "<strong>Red (<40%)</strong> = Poor quality - significant improvements needed".to_string(),
                ],
            },
        ),
        Section::new(
            "Function Documentation Status",
            "Documentation status for each function (sorted by lines of code).",
            Box::new(bar_chart),
            Hint {
                title: "How to interpret:".to_string(),
                items: vec![
                    "<strong>Green bars</strong> = Well-documented functions".to_string(),
                    "<strong>Red bars</strong> = Functions without documentation".to_string(),
                    "<strong>Taller bars</strong> = Larger functions (more lines)".to_string(),
                    "<strong>Action:</strong> Prioritize documenting larger undocumented functions".to_string(),
                ],
            },
        ),
        Section::new(
            "Documentation Coverage",
            "Distribution of documented vs undocumented functions.",
            Box::new(pie_chart),
            Hint {
                title: "How to interpret:".to_string(),
                items: vec![
                    "<strong>Documented</strong> = Functions with properly placed comments".to_string(),
                    "<strong>Undocumented</strong> = Functions missing documentation".to_string(),
                    "<strong>Goal:</strong> Maximize the Documented segment".to_string(),
                ],
            },
        ),
    ])
}

/// Go `generateFunctionCoverageChart` + `createFunctionCoverageBarChart`
/// (plot.go:116).
fn function_coverage_chart(report: &GoValue) -> Option<Chart> {
    let functions = report_function_list(report)?;
    if functions.is_empty() {
        return Some(empty_comments_chart());
    }

    // mapx.SortAndLimit by lines desc (Go's unstable sort.Slice over the
    // walk-order input — go_sort reproduces the tie permutation).
    let mut sorted = functions.clone();
    go_sort::slice(&mut sorted, |a, b| lines_value(a) > lines_value(b));
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
    bar.y_axis = co.y_axis("Lines of Code");

    let labels: Vec<String> = sorted.iter().map(|f| function_name(f).to_string()).collect();
    bar.set_x_axis_labels(&labels);

    let bar_data = GoValue::Array(
        sorted
            .iter()
            .map(|f| {
                let color = if is_documented(f) { "#91cc75" } else { "#ee6666" };
                BarData {
                    value: Some(GoValue::Int(lines_value(f))),
                    item_style: Some(ItemStyle {
                        color: color.to_string(),
                        ..ItemStyle::default()
                    }),
                    ..BarData::default()
                }
                .value()
            })
            .collect(),
    );
    bar.add_series("Lines", bar_data);

    Some(bar)
}

/// Go `generateDocumentationPieChart` + `createDocumentationPieChart`
/// (plot.go:236).
fn documentation_pie_chart(report: &GoValue) -> Chart {
    let documented = report_int(report, "documented_functions");
    // Go computes `undocumented` only when `total_functions` is present (it
    // stays 0 otherwise).
    let undocumented = match report_value(report, "total_functions") {
        Some(GoValue::Int(t)) => *t - documented,
        Some(GoValue::Float(t)) => *t as i64 - documented,
        _ => 0,
    };

    if documented == 0 && undocumented == 0 {
        return empty_comments_pie();
    }

    let palette = get_chart_palette(Theme::Dark);
    let pie_data = vec![
        cf_plotpage::echarts::PieData {
            name: "Documented".to_string(),
            value: Some(GoValue::Int(documented)),
            item_style: Some(ItemStyle {
                color: palette.semantic.good.to_string(),
                ..ItemStyle::default()
            }),
            ..cf_plotpage::echarts::PieData::default()
        },
        cf_plotpage::echarts::PieData {
            name: "Undocumented".to_string(),
            value: Some(GoValue::Int(undocumented)),
            item_style: Some(ItemStyle {
                color: palette.semantic.bad.to_string(),
                ..ItemStyle::default()
            }),
            ..cf_plotpage::echarts::PieData::default()
        },
    ];

    build_pie_chart(None, "Documentation", pie_data, PIE_RADIUS)
}

/// Go `generateOverallScoreGauge` + `createScoreLiquid` (plot.go:276): a
/// default-theme (`white`) liquid-fill chart on a 400x400 canvas.
fn overall_score_gauge(report: &GoValue) -> Chart {
    let score = match report_value(report, "overall_score") {
        Some(GoValue::Float(v)) => *v,
        _ => 0.0,
    };

    let mut liquid = Chart::new(ChartKind::Liquid);
    liquid.set_init("400px", "400px", "", "");
    liquid.add_series(
        "Score",
        GoValue::Array(vec![LiquidData {
            value: Some(GoValue::Float(score)),
            ..LiquidData::default()
        }
        .value()]),
    );
    liquid
}

/// Go `createEmptyCommentsChart` (plot.go:302).
fn empty_comments_chart() -> Chart {
    let co = ChartOpts::default_dark();
    let mut bar = Chart::new(ChartKind::Bar);
    let (w, h, bg, theme) = co.init("100%", EMPTY_CHART_HEIGHT);
    bar.set_init(&w, &h, &bg, &theme);
    bar.title = co.title("Function Documentation", "No data");
    bar
}

/// Go `createEmptyCommentsPie` (plot.go:314).
fn empty_comments_pie() -> Chart {
    let co = ChartOpts::default_dark();
    let mut pie = Chart::new(ChartKind::Pie);
    let (w, h, bg, theme) = co.init("600px", EMPTY_CHART_HEIGHT);
    pie.set_init(&w, &h, &bg, &theme);
    pie.title = co.title("Documentation Coverage", "No data");
    pie
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_gojson::MapOrigin;

    fn raw_fn(name: &str, lines: i64, assessment: &str) -> GoValue {
        let mut m = GoMap::new(MapOrigin::Map);
        m.push("function", GoValue::Str(name.to_string()));
        m.push("lines", GoValue::Int(lines));
        m.push("assessment", GoValue::Str(assessment.to_string()));
        GoValue::Map(m)
    }

    fn raw_report() -> GoValue {
        let mut m = GoMap::new(MapOrigin::Map);
        m.push(
            "functions",
            GoValue::Array(vec![
                raw_fn("a", 10, "✅ Well Documented"),
                raw_fn("b", 30, "❌ No Comment"),
            ]),
        );
        m.push("documented_functions", GoValue::Int(1));
        m.push("total_functions", GoValue::Int(2));
        m.push("overall_score", GoValue::Float(0.25));
        GoValue::Map(m)
    }

    #[test]
    fn sections_carry_go_titles() {
        let secs = sections(&raw_report()).expect("sections");
        assert_eq!(secs.len(), 3);
        assert_eq!(secs[0].title, "Overall Documentation Score");
        assert_eq!(secs[1].title, "Function Documentation Status");
        assert_eq!(secs[2].title, "Documentation Coverage");
    }

    #[test]
    fn missing_functions_key_skips_page() {
        let m = GoMap::new(MapOrigin::Map);
        assert!(sections(&GoValue::Map(m)).is_none());
    }

    #[test]
    fn liquid_gauge_matches_go_shape() {
        let json = overall_score_gauge(&raw_report()).option_json();
        assert!(json.contains(
            "\"series\":[{\"name\":\"Score\",\"type\":\"liquidFill\",\"data\":[{\"value\":0.25}]}]"
        ));
        // No XY axes, default white theme color array.
        assert!(!json.contains("xAxis"));
        assert!(json.starts_with("{\"color\":["));
        let snippet = overall_score_gauge(&raw_report()).render_snippet("AAAAAAAAAAAA");
        assert!(snippet.contains("width:400px;height:400px;"));
    }

    #[test]
    fn bar_sorted_by_lines_desc_with_status_colors() {
        let json = function_coverage_chart(&raw_report()).expect("bar").option_json();
        assert!(json.contains("\"data\":[\"b\",\"a\"]"));
        assert!(json.contains(
            "{\"name\":\"Lines\",\"type\":\"bar\",\"data\":[{\"value\":30,\"itemStyle\":{\"color\":\"#ee6666\"}},{\"value\":10,\"itemStyle\":{\"color\":\"#91cc75\"}}]}"
        ));
    }

    #[test]
    fn pie_counts_match_go() {
        let json = documentation_pie_chart(&raw_report()).option_json();
        assert!(json.contains(
            "{\"name\":\"Documented\",\"value\":1,\"itemStyle\":{\"color\":\"#22c55e\"}},{\"name\":\"Undocumented\",\"value\":1,\"itemStyle\":{\"color\":\"#ef4444\"}}"
        ));
    }
}
