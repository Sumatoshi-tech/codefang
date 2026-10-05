//! `history/imports` (`imports-per-dev`) plot sections: the top imports by
//! total usage across every developer, language and tick.
//!
//! The caller passes the aggregated per-import counts
//! (`imports_run_usage_counts`); [`cf_imports::history::top_imports`] ranks
//! them (count descending, name ascending) and cuts the top 20.

use std::collections::BTreeMap;

use cf_imports::history::top_imports;
use cf_plotpage::echarts::{
    AxisLabel, AxisLine, BarData, Chart, ChartKind, ItemStyle, LineStyle, XAxis,
};
use cf_plotpage::{get_chart_palette, ChartOpts, Hint, Section, Theme};

/// Rotation of the x-axis import labels.
const X_AXIS_ROTATE: f64 = 60.0;

/// The plot sections: zero imports yield zero sections.
pub fn sections(usage_counts: &BTreeMap<String, i64>) -> Vec<Section> {
    let (labels, data) = top_imports(usage_counts);
    if labels.is_empty() {
        return Vec::new();
    }

    let chart = build_usage_bar_chart(&labels, &data);

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

/// One "Usage" bar per import label.
fn build_usage_bar_chart(labels: &[String], data: &[i64]) -> Chart {
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
    bar.set_x_axis_labels(labels);

    let values = cf_gojson::GoValue::Array(
        data.iter()
            .map(|count| {
                BarData {
                    value: Some(cf_gojson::GoValue::Int(*count)),
                    ..BarData::default()
                }
                .value()
            })
            .collect(),
    );
    let series = bar.add_series("Usage", values);
    series.item_style = Some(ItemStyle {
        color: palette.primary[1].to_string(),
        ..ItemStyle::default()
    });

    bar
}
