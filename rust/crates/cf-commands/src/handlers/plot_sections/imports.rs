//! `static/imports` plot sections — port of Go
//! `internal/analyzers/imports/static_plot.go` (+ the shared bar helpers in
//! `imports/plot.go`).
//!
//! Consumes the AGGREGATED RAW imports report (the `analyze.Report` value
//! `imports_raw_report_value` builds): the top-imports usage bar, the import
//! categories pie, and the dependency-risk table. The Go renderer never
//! errors, so `sections` always returns `Some`.

use cf_gojson::GoValue;
use cf_plotpage::components::Table;
use cf_plotpage::echarts::{
    AxisLabel, AxisLine, BarData, Chart, ChartKind, ItemStyle, Label, LineStyle, PieData, XAxis,
};
use cf_plotpage::{get_chart_palette, ChartOpts, Hint, Section, Theme};

/// static_plot.go / plot.go constants.
const IMPORTS_PIE_RADIUS: &str = "60%";
const IMPORTS_CATEGORY_HEIGHT: &str = "420px";
const TOP_IMPORTS_LIMIT: usize = 20;
const X_AXIS_ROTATE: f64 = 60.0;
const EMPTY_CHART_HEIGHT: &str = "400px";
const MAX_DEPENDENCY_RISK_ROWS: usize = 30;

/// The registered section renderer for `static/imports` — Go
/// `imports.RegisterPlotSections` → `(&Analyzer{}).generateStaticSections`.
pub fn sections(report: &GoValue) -> Option<Vec<Section>> {
    // ComputeAllMetrics over the raw report (errors fall back to the zero
    // value in Go; the Rust computation is infallible).
    let report_value = imports_report_value(report);
    let metrics = cf_imports::compute_all_metrics(&report_value)
        .unwrap_or_else(|_| cf_imports::ComputedMetrics::default());

    Some(vec![
        Section::new(
            "Top Imports Usage",
            "Most frequently used imports across scanned files.",
            Box::new(static_imports_bar_chart(report, &metrics)),
            Hint {
                title: "How to interpret:".to_string(),
                items: vec![
                    "Tall bars indicate the most reused imports.".to_string(),
                    "High concentration in few imports can signal architectural coupling."
                        .to_string(),
                    "Review rarely used imports for cleanup opportunities.".to_string(),
                ],
            },
        ),
        Section::new(
            "Import Categories",
            "Distribution across stdlib, external, and relative imports.",
            Box::new(import_categories_pie(&metrics)),
            Hint {
                title: "How to interpret:".to_string(),
                items: vec![
                    "Higher external share often implies larger supply-chain surface.".to_string(),
                    "Relative imports can indicate local module coupling.".to_string(),
                    "Use category mix to guide dependency governance decisions.".to_string(),
                ],
            },
        ),
        Section::new(
            "Dependency Risk Overview",
            "Potentially risky import patterns extracted from static metrics.",
            Box::new(dependency_risk_table(&metrics, MAX_DEPENDENCY_RISK_ROWS)),
            Hint {
                title: "How to interpret:".to_string(),
                items: vec![
                    "MEDIUM risk often means deeply nested relative imports.".to_string(),
                    "LOW risk often indicates long package paths.".to_string(),
                    "Treat this table as triage input for refactoring.".to_string(),
                ],
            },
        ),
    ])
}

/// Rebuilds the `cf_imports::ReportValue` view of the raw report (the
/// `imports` list + `count` are all `ComputeAllMetrics` reads).
fn imports_report_value(report: &GoValue) -> cf_imports::ReportValue {
    let mut rv = cf_imports::ReportValue::map();
    if let Some(top) = report.as_map() {
        if let Some(GoValue::Array(items)) = top.get("imports") {
            let imports: Vec<cf_imports::ReportValue> = items
                .iter()
                .filter_map(|v| match v {
                    GoValue::Str(s) => Some(cf_imports::ReportValue::Str(s.clone())),
                    _ => None,
                })
                .collect();
            rv.insert("imports", cf_imports::ReportValue::List(imports));
        }
        if let Some(GoValue::Int(count)) = top.get("count") {
            rv.insert("count", cf_imports::ReportValue::Int(*count));
        }
    }
    rv
}

/// Go `reportutil.GetStringIntMap(report, KeyImportCounts)`: the per-import
/// occurrence counts. The raw report stores them byte-sorted by key, which is
/// the deterministic stand-in for Go's random map order (the harness measures
/// the variance).
fn import_counts(report: &GoValue) -> Vec<(String, i64)> {
    let mut counts: Vec<(String, i64)> = Vec::new();
    if let Some(GoValue::Map(m)) = report.as_map().and_then(|top| top.get("import_counts")) {
        for (k, v) in m.iter() {
            if let GoValue::Int(n) = v {
                counts.push((k.clone(), *n));
            }
        }
    }
    counts
}

/// Go `buildStaticImportsBarChart` + `topImports` + `createImportsBarChart`.
fn static_imports_bar_chart(report: &GoValue, metrics: &cf_imports::ComputedMetrics) -> Chart {
    let mut counts = import_counts(report);
    if counts.is_empty() {
        // Fallback: count the metric ImportList paths.
        let mut order: Vec<String> = Vec::new();
        let mut by_path: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        for imp in &metrics.import_list {
            if !by_path.contains_key(&imp.path) {
                order.push(imp.path.clone());
            }
            *by_path.entry(imp.path.clone()).or_insert(0) += 1;
        }
        counts = order
            .into_iter()
            .map(|p| {
                let c = by_path[&p];
                (p, c)
            })
            .collect();
    }
    if counts.is_empty() {
        return empty_imports_chart();
    }

    // topImports: count descending, top 20. Go iterates a map randomly and
    // sorts unstably, so the equal-count order is Go-nondeterministic; the
    // key-ascending input + stable sort is the deterministic stand-in.
    counts.sort_by(|a, b| b.1.cmp(&a.1));
    counts.truncate(TOP_IMPORTS_LIMIT);

    let co = ChartOpts::default_dark();
    let palette = get_chart_palette(Theme::Dark);

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
    bar.y_axis = co.y_axis("Usage Count");

    let labels: Vec<String> = counts.iter().map(|(k, _)| k.clone()).collect();
    bar.set_x_axis_labels(&labels);

    let bar_data = GoValue::Array(
        counts
            .iter()
            .map(|(_, v)| {
                BarData {
                    value: Some(GoValue::Int(*v)),
                    ..BarData::default()
                }
                .value()
            })
            .collect(),
    );
    let series = bar.add_series("Usage", bar_data);
    series.item_style = Some(ItemStyle {
        color: palette.primary[1].to_string(),
        ..ItemStyle::default()
    });

    bar
}

/// Go `buildImportCategoriesPie` (static_plot.go:116).
fn import_categories_pie(metrics: &cf_imports::ComputedMetrics) -> Chart {
    if metrics.categories.is_empty() {
        return empty_import_categories_pie();
    }

    let co = ChartOpts::default_dark();
    let palette = get_chart_palette(Theme::Dark);

    let mut pie = Chart::new(ChartKind::Pie);
    let (w, h, bg, theme) = co.init("100%", IMPORTS_CATEGORY_HEIGHT);
    pie.set_init(&w, &h, &bg, &theme);
    pie.tooltip = co.tooltip("item");
    pie.legend = co.legend();
    pie.title = co.title("Import Categories", "");

    let color_count = palette.primary.len();
    let data = GoValue::Array(
        metrics
            .categories
            .iter()
            .enumerate()
            .map(|(idx, category)| {
                PieData {
                    name: category.category.clone(),
                    value: Some(GoValue::Int(category.count)),
                    item_style: Some(ItemStyle {
                        color: palette.primary[idx % color_count].to_string(),
                        ..ItemStyle::default()
                    }),
                    ..PieData::default()
                }
                .value()
            })
            .collect(),
    );
    let series = pie.add_series("Categories", data);
    series.radius = Some(GoValue::Str(IMPORTS_PIE_RADIUS.to_string()));
    series.label = Some(Label {
        show: Some(true),
        formatter: "{b}: {c} ({d}%)".to_string(),
        color: co.text_color().to_string(),
        ..Label::default()
    });

    pie
}

/// Go `buildDependencyRiskTableWithLimit` (static_plot.go:177).
fn dependency_risk_table(metrics: &cf_imports::ComputedMetrics, row_limit: usize) -> Table {
    let mut table = Table::new(vec![
        "Import".to_string(),
        "Risk".to_string(),
        "Reason".to_string(),
    ]);

    if metrics.dependencies.is_empty() {
        table.add_row(vec![
            "No dependency risks detected".to_string(),
            "INFO".to_string(),
            "-".to_string(),
        ]);
        return table;
    }

    // sort.Slice by (risk desc, path asc) — the keys fully order the unique
    // paths, so a stable sort yields the Go result.
    let mut deps = metrics.dependencies.clone();
    deps.sort_by(|a, b| {
        if a.risk_level != b.risk_level {
            b.risk_level.cmp(&a.risk_level)
        } else {
            a.path.cmp(&b.path)
        }
    });

    let limit = deps.len().min(row_limit);
    for dep in &deps[..limit] {
        table.add_row(vec![
            dep.path.clone(),
            dep.risk_level.clone(),
            dep.reason.clone(),
        ]);
    }
    if deps.len() > row_limit {
        table.add_row(vec![
            format!("... and {} more", deps.len() - row_limit),
            "INFO".to_string(),
            format!("Showing top {} of {} total risks", row_limit, deps.len()),
        ]);
    }

    table
}

/// Go `createEmptyImportsChart` (plot.go:102).
fn empty_imports_chart() -> Chart {
    let co = ChartOpts::default_dark();
    let mut bar = Chart::new(ChartKind::Bar);
    let (w, h, bg, theme) = co.init("100%", EMPTY_CHART_HEIGHT);
    bar.set_init(&w, &h, &bg, &theme);
    bar.title = co.title("Top Imports", "No data");
    bar
}

/// Go `createEmptyImportCategoriesPie` (static_plot.go:163).
fn empty_import_categories_pie() -> Chart {
    let co = ChartOpts::default_dark();
    let mut pie = Chart::new(ChartKind::Pie);
    let (w, h, bg, theme) = co.init("100%", IMPORTS_CATEGORY_HEIGHT);
    pie.set_init(&w, &h, &bg, &theme);
    pie.title = co.title("Import Categories", "No data");
    pie
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_gojson::{GoMap, MapOrigin};

    fn raw_report() -> GoValue {
        let imports = vec!["fmt", "github.com/x/y", "./../../../deep"];
        let mut counts = GoMap::new(MapOrigin::Map);
        counts.push("./../../../deep", GoValue::Int(1));
        counts.push("fmt", GoValue::Int(3));
        counts.push("github.com/x/y", GoValue::Int(2));
        let mut m = GoMap::new(MapOrigin::Map);
        m.push(
            "imports",
            GoValue::Array(imports.iter().map(|s| GoValue::Str((*s).to_string())).collect()),
        );
        m.push("import_counts", GoValue::Map(counts));
        m.push("count", GoValue::Int(3));
        m.push("total_files", GoValue::Int(4));
        GoValue::Map(m)
    }

    #[test]
    fn sections_carry_go_titles() {
        let secs = sections(&raw_report()).expect("sections");
        assert_eq!(secs.len(), 3);
        assert_eq!(secs[0].title, "Top Imports Usage");
        assert_eq!(secs[1].title, "Import Categories");
        assert_eq!(secs[2].title, "Dependency Risk Overview");
    }

    #[test]
    fn bar_sorted_by_count_desc_with_primary_color() {
        let report = raw_report();
        let metrics = cf_imports::compute_all_metrics(&imports_report_value(&report))
            .expect("infallible");
        let json = static_imports_bar_chart(&report, &metrics).option_json();
        assert!(json.contains("\"data\":[\"fmt\",\"github.com/x/y\",\"./../../../deep\"]"));
        assert!(json.contains(
            "{\"name\":\"Usage\",\"type\":\"bar\",\"data\":[{\"value\":3},{\"value\":2},{\"value\":1}],\"itemStyle\":{\"color\":\"#38bdf8\"}}"
        ));
        assert!(json.contains("\"axisLabel\":{\"interval\":\"0\",\"rotate\":60,"));
    }

    #[test]
    fn categories_pie_title_and_label_colors_match_go() {
        let report = raw_report();
        let metrics = cf_imports::compute_all_metrics(&imports_report_value(&report))
            .expect("infallible");
        let json = import_categories_pie(&metrics).option_json();
        // Title with both text styles, empty subtext omitted.
        assert!(json.contains(
            "\"title\":{\"text\":\"Import Categories\",\"textStyle\":{\"color\":\"#d6d3d1\"},\"subtextStyle\":{\"color\":\"#a8a29e\"},\"left\":\"center\"}"
        ));
        // Label color is the PRIMARY text color (not muted).
        assert!(json.contains("\"label\":{\"show\":true,\"color\":\"#d6d3d1\",\"formatter\":\"{b}: {c} ({d}%)\"}"));
        // Scroll legend (co.Legend), not the bottom pie legend.
        assert!(json.contains("\"legend\":{\"type\":\"scroll\",\"show\":true,\"left\":\"center\",\"top\":\"10%\",\"textStyle\":{\"color\":\"#a8a29e\"}}"));
    }

    #[test]
    fn risk_table_sorts_and_caps_rows() {
        let report = raw_report();
        let metrics = cf_imports::compute_all_metrics(&imports_report_value(&report))
            .expect("infallible");
        let table = dependency_risk_table(&metrics, MAX_DEPENDENCY_RISK_ROWS);
        assert_eq!(table.rows.len(), 1);
        assert_eq!(table.rows[0][0], "./../../../deep");
        assert_eq!(table.rows[0][1], "MEDIUM");
    }
}
