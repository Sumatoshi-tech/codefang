//! Standard chart constructors — port of Go `plotpage/builders.go`.

use cf_gojson::GoValue;

use crate::chart_opts::ChartOpts;
use crate::echarts::{
    AreaStyle, BarData, Chart, ChartKind, ItemStyle, Label, Legend, LineData, LineStyle, PieData,
    TextStyle,
};

/// A single numeric value in a chart series (Go `plotpage.SeriesData` is `any`
/// holding ints or floats).
#[derive(Debug, Clone, Copy)]
pub enum SeriesValue {
    /// An integer value.
    Int(i64),
    /// A float value.
    Float(f64),
}

impl SeriesValue {
    /// The underlying [`GoValue`].
    #[must_use]
    pub fn go_value(self) -> GoValue {
        match self {
            SeriesValue::Int(i) => GoValue::Int(i),
            SeriesValue::Float(f) => GoValue::Float(f),
        }
    }
}

/// Properties and data for a single bar series (Go `plotpage.BarSeries`).
#[derive(Debug, Clone, Default)]
pub struct BarSeries {
    /// Series name.
    pub name: String,
    /// Data values.
    pub data: Vec<SeriesValue>,
    /// Optional series color (theme default when empty).
    pub color: String,
    /// Optional stack grouping.
    pub stack: String,
}

/// Properties and data for a single line series (Go `plotpage.LineSeries`).
#[derive(Debug, Clone, Default)]
pub struct LineSeries {
    /// Series name.
    pub name: String,
    /// Data values.
    pub data: Vec<SeriesValue>,
    /// Optional series color (theme default when empty).
    pub color: String,
    /// Optional stack grouping.
    pub stack: String,
    /// Optional area opacity for area charts.
    pub area_opacity: f64,
}

/// Pie chart defaults (builders.go:29).
const PIE_DEFAULT_WIDTH: &str = "600px";
const PIE_DEFAULT_HEIGHT: &str = "400px";
const PIE_DEFAULT_RADIUS: &str = "60%";
const PIE_DEFAULT_LABEL: &str = "{b}: {c} ({d}%)";

/// Constructs a fully configured pie chart (Go `plotpage.BuildPieChart`):
/// 600x400 canvas, bottom legend, the given radius (default 60%), and the
/// `{b}: {c} ({d}%)` label formatter. `c_opts == None` uses the dark default.
#[must_use]
pub fn build_pie_chart(
    c_opts: Option<&ChartOpts>,
    series_name: &str,
    data: Vec<PieData>,
    radius: &str,
) -> Chart {
    let default_opts = ChartOpts::default_dark();
    let co = c_opts.unwrap_or(&default_opts);
    let radius = if radius.is_empty() { PIE_DEFAULT_RADIUS } else { radius };

    let mut pie = Chart::new(ChartKind::Pie);
    pie.tooltip = co.tooltip("item");
    let (w, h, bg, theme) = co.init(PIE_DEFAULT_WIDTH, PIE_DEFAULT_HEIGHT);
    pie.set_init(&w, &h, &bg, &theme);
    pie.legend = Legend {
        show: Some(true),
        top: "bottom".to_string(),
        text_style: Some(TextStyle {
            color: co.text_muted_color().to_string(),
            ..TextStyle::default()
        }),
        ..Legend::default()
    };

    let data_value = GoValue::Array(data.iter().map(PieData::value).collect());
    let series = pie.add_series(series_name, data_value);
    series.label = Some(Label {
        show: Some(true),
        formatter: PIE_DEFAULT_LABEL.to_string(),
        color: co.text_muted_color().to_string(),
        ..Label::default()
    });
    series.radius = Some(GoValue::Str(radius.to_string()));

    pie
}

/// Constructs a fully configured bar chart (Go `plotpage.BuildBarChart`).
#[must_use]
pub fn build_bar_chart(
    c_opts: Option<&ChartOpts>,
    labels: &[String],
    series: &[BarSeries],
    y_axis_label: &str,
) -> Chart {
    let default_opts = ChartOpts::default_dark();
    let co = c_opts.unwrap_or(&default_opts);

    let mut bar = Chart::new(ChartKind::Bar);
    let (w, h, bg, theme) = co.init("100%", "500px");
    bar.set_init(&w, &h, &bg, &theme);
    bar.tooltip = co.tooltip("axis");
    bar.data_zoom = co.data_zoom();
    bar.x_axis = co.x_axis("");
    bar.y_axis = co.y_axis(y_axis_label);
    bar.legend = co.legend();

    bar.set_x_axis_labels(labels);

    for s in series {
        let data = GoValue::Array(
            s.data
                .iter()
                .map(|v| {
                    BarData {
                        value: Some(v.go_value()),
                        ..BarData::default()
                    }
                    .value()
                })
                .collect(),
        );
        let added = bar.add_series(&s.name, data);
        if !s.color.is_empty() {
            added.item_style = Some(ItemStyle {
                color: s.color.clone(),
                ..ItemStyle::default()
            });
        }
    }

    bar
}

/// Constructs a fully configured line chart (Go `plotpage.BuildLineChart`).
#[must_use]
pub fn build_line_chart(
    c_opts: Option<&ChartOpts>,
    labels: &[String],
    series: &[LineSeries],
    y_axis_label: &str,
) -> Chart {
    let default_opts = ChartOpts::default_dark();
    let co = c_opts.unwrap_or(&default_opts);

    let mut line = Chart::new(ChartKind::Line);
    let (w, h, bg, theme) = co.init("100%", "500px");
    line.set_init(&w, &h, &bg, &theme);
    line.tooltip = co.tooltip("axis");
    line.data_zoom = co.data_zoom();
    line.x_axis = co.x_axis("");
    line.y_axis = co.y_axis(y_axis_label);
    line.legend = co.legend();

    line.set_x_axis_labels(labels);

    for s in series {
        let data = GoValue::Array(
            s.data
                .iter()
                .map(|v| {
                    LineData {
                        value: Some(v.go_value()),
                        ..LineData::default()
                    }
                    .value()
                })
                .collect(),
        );
        let added = line.add_series(&s.name, data);
        if !s.color.is_empty() {
            added.item_style = Some(ItemStyle {
                color: s.color.clone(),
                ..ItemStyle::default()
            });
            added.line_style = Some(LineStyle {
                color: s.color.clone(),
                ..LineStyle::default()
            });
        }
        if !s.stack.is_empty() {
            added.stack.clone_from(&s.stack);
        }
        if s.area_opacity > 0.0 {
            added.area_style = Some(AreaStyle {
                opacity: Some(s.area_opacity),
                ..AreaStyle::default()
            });
        }
    }

    line
}
