//! `history/sentiment` plot sections.
//! (`GenerateStoreSections` → `buildStoreSections` over the `time_series`,
//! `trend`, and `aggregate` store kinds — the run's `ComputedMetrics`).

use cf_gojson::GoValue;
use cf_plotpage::echarts::{
    AreaStyle, AxisLabel, AxisLine, Chart, ChartKind, ItemStyle, Label, LineData, LineStyle,
    PieData, SplitLine, YAxis,
};
use cf_plotpage::{get_chart_palette, ChartOpts, Hint, Section, Theme};

/// Reference sentiment plot-section constants.
const AREA_OPACITY: f64 = 0.3;
const COMMENT_AXIS_INDEX: i64 = 1;
const COMMENT_BAR_OPACITY: f64 = 0.4;
const POSITIVE_ZONE_LABEL: &str = "Positive Zone";
const NEGATIVE_ZONE_LABEL: &str = "Negative Zone";
const SENTIMENT_SERIES_LABEL: &str = "Sentiment";
const COMMENT_COUNT_LABEL: &str = "Comments";
const TREND_LINE_LABEL: &str = "Trend";
const SENTIMENT_AXIS_LABEL: &str = "Sentiment Score";
const COMMENT_COUNT_AXIS_LABEL: &str = "Comment Count";
const CHART_SECTION_TITLE: &str = "Sentiment Analysis Over Time";
const CHART_SECTION_SUBTITLE: &str =
    "Sentiment score and comment volume per time interval. Green zone = positive, red zone = negative.";
const DISTRIBUTION_TITLE: &str = "Sentiment Distribution";
const DISTRIBUTION_SUBTITLE: &str = "Breakdown of positive, neutral, and negative time periods.";
const SENTIMENT_LINE_WIDTH: f64 = 2.0;
const ZONE_LINE_WIDTH: f64 = 1.0;
const ZONE_OPACITY: f64 = 0.08;
const DISTRIBUTION_INNER: &str = "40%";
const DISTRIBUTION_OUTER: &str = "70%";
const PIE_CHART_HEIGHT: &str = "400px";

/// The reference `GenerateStoreSections` → `buildStoreSections`: an empty time series
/// yields zero sections.
pub fn sections(metrics: &cf_sentiment::ComputedMetrics) -> Vec<Section> {
    if metrics.time_series.is_empty() {
        return Vec::new();
    }

    vec![
        Section {
            title: CHART_SECTION_TITLE.to_string(),
            subtitle: CHART_SECTION_SUBTITLE.to_string(),
            chart: Some(Box::new(build_sentiment_chart(metrics))),
            hint: build_main_chart_hint(metrics),
        },
        Section {
            title: DISTRIBUTION_TITLE.to_string(),
            subtitle: DISTRIBUTION_SUBTITLE.to_string(),
            chart: Some(Box::new(build_distribution_chart(metrics))),
            hint: Hint::default(),
        },
    ]
}

/// The reference `buildSentimentChart` (non-empty path; the caller already gated on the
/// time series).
fn build_sentiment_chart(metrics: &cf_sentiment::ComputedMetrics) -> Chart {
    let co = ChartOpts::default_dark();
    let mut line = init_sentiment_line(&co);

    let n = metrics.time_series.len();
    let labels: Vec<String> = metrics
        .time_series
        .iter()
        .map(|ts| ts.tick.to_string())
        .collect();
    line.set_x_axis_labels(&labels);

    // prepareChartData.
    let line_data = |values: Vec<GoValue>| -> GoValue {
        GoValue::Array(
            values
                .into_iter()
                .map(|v| {
                    LineData {
                        value: Some(v),
                        ..LineData::default()
                    }
                    .value()
                })
                .collect(),
        )
    };
    let sentiment = line_data(
        metrics
            .time_series
            .iter()
            .map(|ts| GoValue::Float(f64::from(ts.sentiment)))
            .collect(),
    );
    let comments = line_data(
        metrics
            .time_series
            .iter()
            .map(|ts| GoValue::Int(ts.comment_count))
            .collect(),
    );
    let positive_zone = line_data(
        metrics
            .time_series
            .iter()
            .map(|_| GoValue::Float(cf_sentiment::SENTIMENT_POSITIVE_THRESHOLD))
            .collect(),
    );
    let negative_zone = line_data(
        metrics
            .time_series
            .iter()
            .map(|_| GoValue::Float(cf_sentiment::SENTIMENT_NEGATIVE_THRESHOLD))
            .collect(),
    );
    // Trend interpolation in FLOAT32 (reference: start/end/step are float32; each
    // point is float64(start + step*float32(i))).
    let trend = {
        let start = metrics.trend.start_sentiment;
        let end = metrics.trend.end_sentiment;
        let mut points: Vec<GoValue> = Vec::with_capacity(n);
        if n > 1 {
            let step = (end - start) / (n as f32 - 1.0);
            for i in 0..n {
                points.push(GoValue::Float(f64::from(start + step * i as f32)));
            }
        } else {
            points.push(GoValue::Float(f64::from(start)));
        }
        line_data(points)
    };

    // addChartSeries.
    let palette = get_chart_palette(Theme::Dark);

    let s = line.add_series(POSITIVE_ZONE_LABEL, positive_zone);
    s.line_style = Some(LineStyle {
        color: palette.semantic.good.to_string(),
        type_: "dashed".to_string(),
        width: ZONE_LINE_WIDTH,
        ..LineStyle::default()
    });
    s.item_style = Some(ItemStyle {
        color: palette.semantic.good.to_string(),
        ..ItemStyle::default()
    });
    s.area_style = Some(AreaStyle {
        opacity: Some(ZONE_OPACITY),
        color: palette.semantic.good.to_string(),
    });
    s.stack = "zone".to_string();
    s.smooth = Some(false);
    s.show_symbol = Some(false);

    let s = line.add_series(NEGATIVE_ZONE_LABEL, negative_zone);
    s.line_style = Some(LineStyle {
        color: palette.semantic.bad.to_string(),
        type_: "dashed".to_string(),
        width: ZONE_LINE_WIDTH,
        ..LineStyle::default()
    });
    s.item_style = Some(ItemStyle {
        color: palette.semantic.bad.to_string(),
        ..ItemStyle::default()
    });
    s.smooth = Some(false);
    s.show_symbol = Some(false);

    let s = line.add_series(SENTIMENT_SERIES_LABEL, sentiment);
    s.line_style = Some(LineStyle {
        color: palette.primary[0].to_string(),
        width: SENTIMENT_LINE_WIDTH,
        ..LineStyle::default()
    });
    s.item_style = Some(ItemStyle {
        color: palette.primary[0].to_string(),
        ..ItemStyle::default()
    });
    s.area_style = Some(AreaStyle {
        opacity: Some(AREA_OPACITY),
        ..AreaStyle::default()
    });
    s.smooth = Some(true);

    if n > 1 {
        let s = line.add_series(TREND_LINE_LABEL, trend);
        s.line_style = Some(LineStyle {
            color: palette.secondary[1].to_string(),
            type_: "dashed".to_string(),
            width: ZONE_LINE_WIDTH,
            ..LineStyle::default()
        });
        s.item_style = Some(ItemStyle {
            color: palette.secondary[1].to_string(),
            ..ItemStyle::default()
        });
        s.smooth = Some(false);
        s.show_symbol = Some(false);
    }

    let s = line.add_series(COMMENT_COUNT_LABEL, comments);
    s.line_style = Some(LineStyle {
        color: palette.primary[2].to_string(),
        ..LineStyle::default()
    });
    s.item_style = Some(ItemStyle {
        color: palette.primary[2].to_string(),
        opacity: Some(COMMENT_BAR_OPACITY),
        ..ItemStyle::default()
    });
    s.y_axis_index = COMMENT_AXIS_INDEX;

    line
}

/// The reference `initSentimentLine`: the dual-axis line frame.
fn init_sentiment_line(co: &ChartOpts) -> Chart {
    let mut line = Chart::new(ChartKind::Line);
    let (w, h, bg, theme) = co.init("100%", "500px");
    line.set_init(&w, &h, &bg, &theme);
    line.tooltip = co.tooltip("axis");
    line.data_zoom = co.data_zoom();
    line.legend = co.legend();
    line.x_axis = co.x_axis("");
    line.y_axis = YAxis {
        name: SENTIMENT_AXIS_LABEL.to_string(),
        min: Some(GoValue::Int(0)),
        max: Some(GoValue::Int(1)),
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
        split_line: Some(SplitLine {
            show: Some(true),
            line_style: Some(LineStyle {
                color: co.grid_color().to_string(),
                ..LineStyle::default()
            }),
        }),
        ..YAxis::default()
    };
    line.grid = vec![co.grid()];

    line.extra_y_axes = vec![YAxis {
        name: COMMENT_COUNT_AXIS_LABEL.to_string(),
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
        split_line: Some(SplitLine {
            show: Some(false),
            line_style: None,
        }),
        ..YAxis::default()
    }];

    line
}

/// The reference `buildDistributionChart`.
fn build_distribution_chart(metrics: &cf_sentiment::ComputedMetrics) -> Chart {
    let palette = get_chart_palette(Theme::Dark);
    let co = ChartOpts::default_dark();

    let mut pie = Chart::new(ChartKind::Pie);
    let (w, h, bg, theme) = co.init("100%", PIE_CHART_HEIGHT);
    pie.set_init(&w, &h, &bg, &theme);
    pie.tooltip = co.tooltip("item");
    pie.legend = co.legend();

    let agg = &metrics.aggregate;
    let item = |name: String, value: i64, color: &str| -> GoValue {
        PieData {
            name,
            value: Some(GoValue::Int(value)),
            item_style: Some(ItemStyle {
                color: color.to_string(),
                ..ItemStyle::default()
            }),
            ..PieData::default()
        }
        .value()
    };
    let data = GoValue::Array(vec![
        item(
            format!("Positive ({})", agg.positive_ticks),
            agg.positive_ticks,
            palette.semantic.good,
        ),
        item(
            format!("Neutral ({})", agg.neutral_ticks),
            agg.neutral_ticks,
            palette.semantic.warning,
        ),
        item(
            format!("Negative ({})", agg.negative_ticks),
            agg.negative_ticks,
            palette.semantic.bad,
        ),
    ]);

    let series = pie.add_series(DISTRIBUTION_TITLE, data);
    series.label = Some(Label {
        show: Some(true),
        formatter: "{b}: {d}%".to_string(),
        color: co.text_color().to_string(),
        ..Label::default()
    });
    series.radius = Some(GoValue::Array(vec![
        GoValue::Str(DISTRIBUTION_INNER.to_string()),
        GoValue::Str(DISTRIBUTION_OUTER.to_string()),
    ]));

    pie
}

/// The reference `buildMainChartHint`.
fn build_main_chart_hint(metrics: &cf_sentiment::ComputedMetrics) -> Hint {
    let mut items = vec![
        "Green dashed line = positive threshold (0.6+), Red dashed line = negative threshold (0.4-)"
            .to_string(),
        "Solid line = actual sentiment score per tick, Dashed line = regression trend".to_string(),
        "Secondary axis shows comment count per tick".to_string(),
    ];

    if !metrics.trend.trend_direction.is_empty() {
        items.push(format!(
            "Trend: {} ({:.1}% change)",
            metrics.trend.trend_direction, metrics.trend.change_percent
        ));
    }

    // The reference implementation's STORE path reconstructs ComputedMetrics WITHOUT LowSentimentPeriods
    // (reference only reads time_series/trend/aggregate), so the
    // "low-sentiment period(s)" hint item never renders on plot pages — even
    // though the run metrics carry the periods.

    items.push("Sudden drops may indicate stressful periods or difficult bugs".to_string());
    items.push(
        "SE-domain terms (kill, abort, fatal) are adjusted to avoid false negatives".to_string(),
    );

    Hint {
        title: "How to interpret:".to_string(),
        items,
    }
}
