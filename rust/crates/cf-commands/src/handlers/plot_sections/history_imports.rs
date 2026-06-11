//! `history/imports` (`imports-per-dev`) plot sections — port of Go
//! `internal/analyzers/imports/store_reader.go` (`GenerateStoreSections` →
//! `buildImportsStoreSections` over the `import_usage` store kind).
//!
//! The store records are Go `topImports(aggregateImportCounts(merged))`: the
//! per-import total counts sorted count-descending and cut to the top 20. The
//! caller passes the name-ascending aggregated counts
//! (`imports_run_usage_counts`); the Go-pdqsort descending sort + cut happen
//! here, mirroring `WriteToStore`.

use cf_plotpage::echarts::{
    AxisLabel, AxisLine, BarData, Chart, ChartKind, ItemStyle, LineStyle, XAxis,
};
use cf_plotpage::{get_chart_palette, ChartOpts, Hint, Section, Theme};

use crate::handlers::go_sort;

/// imports plot.go `topImportsLimit`.
const TOP_IMPORTS_LIMIT: usize = 20;
/// imports plot.go `xAxisRotate`.
const X_AXIS_ROTATE: f64 = 60.0;

/// Go `GenerateStoreSections`: zero records yield zero sections.
pub fn sections(usage_counts: &[(String, i64)]) -> Vec<Section> {
    // Go `topImports`: count-descending `sort.Slice` over the aggregate map's
    // (random-order) entries, then the top-20 cut. The name-ascending input is
    // the deterministic stand-in for the map order; ties are Go-variant.
    let mut records: Vec<(String, i64)> = usage_counts.to_vec();
    go_sort::slice(&mut records, |a, b| a.1 > b.1);
    records.truncate(TOP_IMPORTS_LIMIT);

    if records.is_empty() {
        return Vec::new();
    }

    let chart = build_bar_chart_from_usage_records(&records);

    vec![Section {
        title: "Top Imports Usage".to_string(),
        subtitle: "Most frequently added imports across the codebase.".to_string(),
        chart: Some(Box::new(chart)),
        hint: Hint {
            title: "How to interpret:".to_string(),
            items: vec![
                "Tall bars = frequently used imports (core dependencies)".to_string(),
                "External libraries = check for outdated or redundant dependencies".to_string(),
                "Standard library imports = indicate code patterns".to_string(),
                "Look for: Unexpected dependencies or duplicate functionality".to_string(),
                "Action: Consider consolidating similar imports".to_string(),
            ],
        },
    }]
}

/// Go `buildBarChartFromUsageRecords` → `createImportsBarChart` (+ the
/// follow-up `WithXAxisOpts` override, which re-sets the identical axis).
fn build_bar_chart_from_usage_records(records: &[(String, i64)]) -> Chart {
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

    let labels: Vec<String> = records.iter().map(|(name, _)| name.clone()).collect();
    bar.set_x_axis_labels(&labels);

    let data = cf_gojson::GoValue::Array(
        records
            .iter()
            .map(|(_, count)| {
                BarData {
                    value: Some(cf_gojson::GoValue::Int(*count)),
                    ..BarData::default()
                }
                .value()
            })
            .collect(),
    );
    let series = bar.add_series("Usage", data);
    series.item_style = Some(ItemStyle {
        color: palette.primary[1].to_string(),
        ..ItemStyle::default()
    });

    bar
}
