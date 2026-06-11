//! `static/complexity` plot sections — port of Go
//! `internal/analyzers/complexity/plot.go`.
//!
//! Consumes the AGGREGATED RAW complexity report (the `analyze.Report` value
//! `complexity_raw_report_value` builds) exactly as Go's registered section
//! renderer does: the bar chart over the top-20 functions by cyclomatic
//! complexity, the cyclomatic-vs-cognitive scatter, and the distribution pie.

use cf_gojson::{GoMap, GoValue};
use cf_plotpage::echarts::{
    AxisLabel, AxisLine, BarData, Chart, ChartKind, Grid, ItemStyle, LineStyle, MarkLineItem,
    ScatterData, SplitLine, XAxis, YAxis,
};
use cf_plotpage::{build_pie_chart, get_chart_palette, ChartOpts, Hint, Section, Theme};

use crate::handlers::static_complexity::go_pdqsort;

/// plot.go constants (plot.go:18).
const TOP_FUNCTIONS_LIMIT: usize = 20;
const X_AXIS_ROTATE: f64 = 45.0;
const EMPTY_CHART_HEIGHT: &str = "400px";
const PIE_RADIUS: &str = "60%";
const SCATTER_SYMBOL_SIZE: i64 = 15;
const NESTING_MULTIPLIER: i64 = 3;
const CYCLOMATIC_YELLOW_LINE: i64 = 5;
const CYCLOMATIC_RED_LINE: i64 = 10;
const COGNITIVE_RED_LINE: i64 = 15;
const UNKNOWN_NAME: &str = "unknown";

/// Plot display labels for the distribution pie (plot.go:31).
const PLOT_LABEL_SIMPLE: &str = "Simple";
const PLOT_LABEL_MODERATE: &str = "Moderate";
const PLOT_LABEL_COMPLEX: &str = "Complex";

/// Go `analyze.ReportFunctionListWithFallback(report, "functions",
/// "function_complexity")`: the function maps under either key.
fn report_function_list(report: &GoValue) -> Option<Vec<&GoMap>> {
    let top = report.as_map()?;
    let arr = match top.get("functions") {
        Some(GoValue::Array(items)) => items,
        _ => match top.get("function_complexity") {
            Some(GoValue::Array(items)) => items,
            _ => return None,
        },
    };
    Some(arr.iter().filter_map(GoValue::as_map).collect())
}

/// Go `reportutil.GetInt` over a function map (safeconv.ToInt semantics:
/// ints pass through, floats truncate toward zero).
fn get_int(m: &GoMap, key: &str) -> i64 {
    match m.get(key) {
        Some(GoValue::Int(i)) => *i,
        Some(GoValue::Uint(u)) => *u as i64,
        Some(GoValue::Float(f)) => *f as i64,
        _ => 0,
    }
}

/// Go `reportutil.MapString`.
fn map_string<'a>(m: &'a GoMap, key: &str) -> &'a str {
    match m.get(key) {
        Some(GoValue::Str(s)) => s.as_str(),
        _ => "",
    }
}

/// Go `filepath.Base`: the last path element (trailing separators trimmed;
/// empty path → `"."`).
fn go_filepath_base(path: &str) -> &str {
    if path.is_empty() {
        return ".";
    }
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return "/";
    }
    match trimmed.rfind('/') {
        Some(pos) => &trimmed[pos + 1..],
        None => trimmed,
    }
}

/// Go `formatPlotLabel` (plot.go:173): `basename:func` when `_source_file`
/// is stamped, otherwise just the name.
fn format_plot_label(f: &GoMap) -> String {
    let mut name = map_string(f, "name");
    if name.is_empty() {
        name = UNKNOWN_NAME;
    }
    let sf = map_string(f, "_source_file");
    if sf.is_empty() {
        return name.to_string();
    }
    format!("{}:{}", go_filepath_base(sf), name)
}

/// Go `getComplexityColor` (plot.go:187).
fn complexity_color(complexity: i64) -> &'static str {
    if complexity <= CYCLOMATIC_YELLOW_LINE {
        "#91cc75"
    } else if complexity <= CYCLOMATIC_RED_LINE {
        "#fac858"
    } else {
        "#ee6666"
    }
}

/// The registered section renderer for `static/complexity` — Go
/// `complexity.RegisterPlotSections` → `(&Analyzer{}).generateSections`
/// (plot.go:61). Returns `None` when the report lacks the functions table
/// (Go `ErrInvalidFunctionsData`).
pub fn sections(report: &GoValue) -> Option<Vec<Section>> {
    let bar = bar_chart(report)?;
    let scatter = scatter_chart(report)?;
    let pie = pie_chart(report);

    Some(vec![
        Section::new(
            "Top Complex Functions",
            "Functions ranked by cyclomatic complexity (higher = more complex).",
            Box::new(bar),
            Hint {
                title: "How to interpret:".to_string(),
                items: vec![
                    "<strong>Green (1-5)</strong> = Simple, easy to understand and test".to_string(),
                    "<strong>Yellow (6-10)</strong> = Moderate complexity, consider simplifying".to_string(),
                    "<strong>Red (>10)</strong> = High complexity, should be refactored".to_string(),
                    "<strong>Action:</strong> Break down complex functions into smaller units".to_string(),
                ],
            },
        ),
        Section::new(
            "Cyclomatic vs Cognitive Complexity",
            "Scatter plot showing relationship between complexity measures.",
            Box::new(scatter),
            Hint {
                title: "How to interpret:".to_string(),
                items: vec![
                    "<strong>Bottom-left</strong> = Simple functions (ideal)".to_string(),
                    "<strong>Top-right</strong> = Complex functions (need attention)".to_string(),
                    "<strong>High cyclomatic, low cognitive</strong> = Many simple branches".to_string(),
                    "<strong>Low cyclomatic, high cognitive</strong> = Deep nesting or recursion".to_string(),
                    "<strong>Bubble size</strong> = Nesting depth".to_string(),
                ],
            },
        ),
        Section::new(
            "Complexity Distribution",
            "Distribution of functions by complexity category.",
            Box::new(pie),
            Hint {
                title: "How to interpret:".to_string(),
                items: vec![
                    "<strong>Simple (1-5)</strong> = Functions that are easy to maintain".to_string(),
                    "<strong>Moderate (6-10)</strong> = Functions that need careful review".to_string(),
                    "<strong>Complex (>10)</strong> = Functions that should be refactored".to_string(),
                    "<strong>Goal:</strong> Maximize Simple functions, minimize Complex ones".to_string(),
                ],
            },
        ),
    ])
}

/// Go `generateComplexityBarChart` + `createComplexityBarChart` (plot.go:121).
fn bar_chart(report: &GoValue) -> Option<Chart> {
    let functions = report_function_list(report)?;
    if functions.is_empty() {
        return Some(empty_bar_chart());
    }

    // mapx.SortAndLimit: copy, sort.Slice (Go's unstable pdqsort — reproduced
    // exactly by go_pdqsort so the tie order in the top 20 matches), truncate.
    let mut sorted = functions.clone();
    go_pdqsort(&mut sorted, &|a: &&GoMap, b: &&GoMap| {
        get_int(a, "cyclomatic_complexity") > get_int(b, "cyclomatic_complexity")
    });
    sorted.truncate(TOP_FUNCTIONS_LIMIT);

    let co = ChartOpts::default_dark();
    let palette = get_chart_palette(Theme::Dark);

    let mut bar = Chart::new(ChartKind::Bar);
    let (w, h, bg, theme) = co.init("100%", "500px");
    bar.set_init(&w, &h, &bg, &theme);
    bar.tooltip = co.tooltip("axis");
    bar.legend = co.legend();
    bar.grid = vec![Grid {
        left: "5%".to_string(),
        right: "5%".to_string(),
        top: "25%".to_string(),
        bottom: "15%".to_string(),
        contain_label: Some(true),
    }];
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
    bar.y_axis = co.y_axis("Complexity");

    let labels: Vec<String> = sorted.iter().map(|f| format_plot_label(f)).collect();
    bar.set_x_axis_labels(&labels);

    let cyclomatic_data = GoValue::Array(
        sorted
            .iter()
            .map(|f| {
                let cc = get_int(f, "cyclomatic_complexity");
                BarData {
                    value: Some(GoValue::Int(cc)),
                    item_style: Some(ItemStyle {
                        color: complexity_color(cc).to_string(),
                        ..ItemStyle::default()
                    }),
                    ..BarData::default()
                }
                .value()
            })
            .collect(),
    );
    let cognitive_data = GoValue::Array(
        sorted
            .iter()
            .map(|f| {
                BarData {
                    value: Some(GoValue::Int(get_int(f, "cognitive_complexity"))),
                    ..BarData::default()
                }
                .value()
            })
            .collect(),
    );

    bar.add_series("Cyclomatic", cyclomatic_data);
    let cognitive = bar.add_series("Cognitive", cognitive_data);
    cognitive.item_style = Some(ItemStyle {
        color: palette.primary[1].to_string(),
        ..ItemStyle::default()
    });

    Some(bar)
}

/// Go `generateComplexityScatterChart` + `createComplexityScatterChart`
/// (plot.go:250).
fn scatter_chart(report: &GoValue) -> Option<Chart> {
    let functions = report_function_list(report)?;
    if functions.is_empty() {
        return Some(empty_scatter_chart());
    }

    let co = ChartOpts::default_dark();
    let palette = get_chart_palette(Theme::Dark);

    let mut scatter = Chart::new(ChartKind::Scatter);
    let (w, h, bg, theme) = co.init("100%", "500px");
    scatter.set_init(&w, &h, &bg, &theme);
    scatter.tooltip = co.tooltip("item");
    scatter.x_axis = XAxis {
        name: "Cyclomatic Complexity".to_string(),
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
        name: "Cognitive Complexity".to_string(),
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

    let scatter_data = GoValue::Array(
        functions
            .iter()
            .map(|f| {
                let cyclomatic = get_int(f, "cyclomatic_complexity");
                let cognitive = get_int(f, "cognitive_complexity");
                let nesting = get_int(f, "nesting_depth");
                ScatterData {
                    value: Some(GoValue::Array(vec![
                        GoValue::Int(cyclomatic),
                        GoValue::Int(cognitive),
                        GoValue::Str(format_plot_label(f)),
                    ])),
                    symbol_size: SCATTER_SYMBOL_SIZE + nesting * NESTING_MULTIPLIER,
                    ..ScatterData::default()
                }
                .value()
            })
            .collect(),
    );

    let series = scatter.add_series("Functions", scatter_data);
    series.item_style = Some(ItemStyle {
        color: palette.primary[1].to_string(),
        ..ItemStyle::default()
    });
    series.mark_lines = vec![
        MarkLineItem::XAxis {
            name: "Cyclomatic warning".to_string(),
            value: GoValue::Int(CYCLOMATIC_RED_LINE),
        },
        MarkLineItem::YAxis {
            name: "Cognitive warning".to_string(),
            value: GoValue::Int(COGNITIVE_RED_LINE),
        },
    ];

    Some(scatter)
}

/// Go `generateComplexityPieChart` + `createComplexityDistributionPie`
/// (plot.go:318).
fn pie_chart(report: &GoValue) -> Chart {
    let Some(functions) = report_function_list(report) else {
        return empty_pie_chart();
    };
    if functions.is_empty() {
        return empty_pie_chart();
    }

    // stats.Distribution over classifyComplexityForPlot.
    let mut simple = 0i64;
    let mut moderate = 0i64;
    let mut complex_count = 0i64;
    for f in &functions {
        let cc = get_int(f, "cyclomatic_complexity");
        if cc <= CYCLOMATIC_YELLOW_LINE {
            simple += 1;
        } else if cc <= CYCLOMATIC_RED_LINE {
            moderate += 1;
        } else {
            complex_count += 1;
        }
    }
    // Distribution keys (kept for parity with the Go label constants).
    let _ = (PLOT_LABEL_SIMPLE, PLOT_LABEL_MODERATE, PLOT_LABEL_COMPLEX);

    let palette = get_chart_palette(Theme::Dark);
    let pie_data = vec![
        cf_plotpage::echarts::PieData {
            name: "Simple (1-5)".to_string(),
            value: Some(GoValue::Int(simple)),
            item_style: Some(ItemStyle {
                color: palette.semantic.good.to_string(),
                ..ItemStyle::default()
            }),
            ..cf_plotpage::echarts::PieData::default()
        },
        cf_plotpage::echarts::PieData {
            name: "Moderate (6-10)".to_string(),
            value: Some(GoValue::Int(moderate)),
            item_style: Some(ItemStyle {
                color: palette.semantic.warning.to_string(),
                ..ItemStyle::default()
            }),
            ..cf_plotpage::echarts::PieData::default()
        },
        cf_plotpage::echarts::PieData {
            name: "Complex (>10)".to_string(),
            value: Some(GoValue::Int(complex_count)),
            item_style: Some(ItemStyle {
                color: palette.semantic.bad.to_string(),
                ..ItemStyle::default()
            }),
            ..cf_plotpage::echarts::PieData::default()
        },
    ];

    build_pie_chart(None, "Complexity", pie_data, PIE_RADIUS)
}

/// Go `createEmptyComplexityChart` (plot.go:355).
fn empty_bar_chart() -> Chart {
    let co = ChartOpts::default_dark();
    let mut bar = Chart::new(ChartKind::Bar);
    let (w, h, bg, theme) = co.init("100%", EMPTY_CHART_HEIGHT);
    bar.set_init(&w, &h, &bg, &theme);
    bar.title = co.title("Function Complexity", "No data");
    bar
}

/// Go `createEmptyScatterChart` (plot.go:367).
fn empty_scatter_chart() -> Chart {
    let co = ChartOpts::default_dark();
    let mut scatter = Chart::new(ChartKind::Scatter);
    let (w, h, bg, theme) = co.init("100%", EMPTY_CHART_HEIGHT);
    scatter.set_init(&w, &h, &bg, &theme);
    scatter.title = co.title("Complexity Scatter", "No data");
    scatter
}

/// Go `createEmptyComplexityPie` (plot.go:379).
fn empty_pie_chart() -> Chart {
    let co = ChartOpts::default_dark();
    let mut pie = Chart::new(ChartKind::Pie);
    let (w, h, bg, theme) = co.init("600px", EMPTY_CHART_HEIGHT);
    pie.set_init(&w, &h, &bg, &theme);
    pie.title = co.title("Complexity Distribution", "No data");
    pie
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_gojson::MapOrigin;

    fn raw_fn(name: &str, sf: &str, cc: i64, cog: i64, nest: i64) -> GoValue {
        let mut m = GoMap::new(MapOrigin::Map);
        m.push("name", GoValue::Str(name.to_string()));
        m.push("_source_file", GoValue::Str(sf.to_string()));
        m.push("cyclomatic_complexity", GoValue::Int(cc));
        m.push("cognitive_complexity", GoValue::Int(cog));
        m.push("nesting_depth", GoValue::Int(nest));
        GoValue::Map(m)
    }

    fn raw_report() -> GoValue {
        let mut m = GoMap::new(MapOrigin::Map);
        m.push(
            "functions",
            GoValue::Array(vec![
                raw_fn("a", "pkg/x.go", 12, 20, 2),
                raw_fn("b", "y.go", 3, 1, 0),
            ]),
        );
        GoValue::Map(m)
    }

    #[test]
    fn sections_carry_go_titles_and_hints() {
        let secs = sections(&raw_report()).expect("sections");
        assert_eq!(secs.len(), 3);
        assert_eq!(secs[0].title, "Top Complex Functions");
        assert_eq!(secs[1].title, "Cyclomatic vs Cognitive Complexity");
        assert_eq!(secs[2].title, "Complexity Distribution");
        assert_eq!(secs[0].hint.items.len(), 4);
        assert_eq!(secs[1].hint.items.len(), 5);
    }

    #[test]
    fn bar_option_json_matches_go_shape() {
        let bar = bar_chart(&raw_report()).expect("bar");
        let json = bar.option_json();
        // Per-item itemStyle on the cyclomatic series, threshold colors.
        assert!(json.contains(
            "{\"name\":\"Cyclomatic\",\"type\":\"bar\",\"data\":[{\"value\":12,\"itemStyle\":{\"color\":\"#ee6666\"}},{\"value\":3,\"itemStyle\":{\"color\":\"#91cc75\"}}]}"
        ));
        // Series-level itemStyle on the cognitive series (dark primary[1]).
        assert!(json.contains(
            "{\"name\":\"Cognitive\",\"type\":\"bar\",\"data\":[{\"value\":20},{\"value\":1}],\"itemStyle\":{\"color\":\"#38bdf8\"}}"
        ));
        // basename:func labels in xAxis data.
        assert!(json.contains("\"data\":[\"x.go:a\",\"y.go:b\"]"));
        assert!(json.contains("\"dataZoom\":[{\"type\":\"slider\",\"end\":100},{\"type\":\"inside\"}]"));
    }

    #[test]
    fn scatter_option_json_matches_go_shape() {
        let scatter = scatter_chart(&raw_report()).expect("scatter");
        let json = scatter.option_json();
        assert!(json.contains("{\"value\":[12,20,\"x.go:a\"],\"symbolSize\":21}"));
        assert!(json.contains("{\"value\":[3,1,\"y.go:b\"],\"symbolSize\":15}"));
        assert!(json.contains(
            "\"markLine\":{\"data\":[{\"name\":\"Cyclomatic warning\",\"xAxis\":10},{\"name\":\"Cognitive warning\",\"yAxis\":15}]}"
        ));
        // XAxis declaration order: type before name; AxisLabel carries the
        // always-emitted showMinLabel/showMaxLabel nulls.
        assert!(json.contains(
            "\"xAxis\":[{\"type\":\"value\",\"name\":\"Cyclomatic Complexity\",\"axisLine\":{\"lineStyle\":{\"color\":\"#57534e\"}},\"axisLabel\":{\"showMinLabel\":null,\"showMaxLabel\":null,\"color\":\"#a8a29e\"}}]"
        ));
        // YAxis declaration order: name, type, splitLine, axisLabel.
        assert!(json.contains(
            "\"yAxis\":[{\"name\":\"Cognitive Complexity\",\"type\":\"value\",\"splitLine\":{\"lineStyle\":{\"color\":\"#44403c\"}},\"axisLabel\":{\"showMinLabel\":null,\"showMaxLabel\":null,\"color\":\"#a8a29e\"}}]"
        ));
    }

    #[test]
    fn pie_option_json_matches_go_shape() {
        let pie = pie_chart(&raw_report());
        let json = pie.option_json();
        assert!(json.contains(
            "\"series\":[{\"name\":\"Complexity\",\"type\":\"pie\",\"radius\":\"60%\",\"data\":[{\"name\":\"Simple (1-5)\",\"value\":1,\"itemStyle\":{\"color\":\"#22c55e\"}},{\"name\":\"Moderate (6-10)\",\"value\":0,\"itemStyle\":{\"color\":\"#eab308\"}},{\"name\":\"Complex (>10)\",\"value\":1,\"itemStyle\":{\"color\":\"#ef4444\"}}],\"label\":{\"show\":true,\"color\":\"#a8a29e\",\"formatter\":\"{b}: {c} ({d}%)\"}}]"
        ));
        assert!(json.contains("\"legend\":{\"show\":true,\"top\":\"bottom\",\"textStyle\":{\"color\":\"#a8a29e\"}}"));
    }

    #[test]
    fn empty_report_yields_no_data_charts() {
        let mut m = GoMap::new(MapOrigin::Map);
        m.push("functions", GoValue::Array(vec![]));
        let report = GoValue::Map(m);
        let bar = bar_chart(&report).expect("empty bar");
        let json = bar.option_json();
        assert!(json.contains("\"series\":null"));
        assert!(json.contains("\"subtext\":\"No data\""));
    }
}
