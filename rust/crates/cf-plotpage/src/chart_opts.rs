//! Themed chart-option presets — port of Go `plotpage/chart_opts.go`.

use crate::echarts::{
    AxisLabel, AxisLine, AxisName, DataZoom, Grid, Indicator, Legend, LineStyle, RadarComponent,
    SplitArea, SplitLine, TextStyle, Title, Tooltip, XAxis, YAxis,
};
use crate::theme::{get_theme_config, Theme, ThemeConfig};

/// DataZoom end percentage default (chart_opts.go `dataZoomEndPercent`).
const DATA_ZOOM_END_PERCENT: f64 = 100.0;

/// Themed chart options based on the current theme (Go `plotpage.ChartOpts`).
#[derive(Debug, Clone)]
pub struct ChartOpts {
    theme: ThemeConfig,
}

impl ChartOpts {
    /// Creates chart options for the given theme (Go `NewChartOpts`).
    #[must_use]
    pub fn new(theme: Theme) -> Self {
        ChartOpts {
            theme: get_theme_config(theme),
        }
    }

    /// Chart options for the default dark theme (Go `DefaultChartOpts`).
    #[must_use]
    pub fn default_dark() -> Self {
        ChartOpts::new(Theme::Dark)
    }

    /// Initialization values with the themed background (Go `ChartOpts.Init`).
    /// Returns `(width, height, background_color, theme)` for
    /// [`crate::echarts::Chart::set_init`].
    #[must_use]
    pub fn init(&self, width: &str, height: &str) -> (String, String, String, String) {
        (
            width.to_string(),
            height.to_string(),
            self.theme.chart_background.to_string(),
            self.theme.echarts_theme.to_string(),
        )
    }

    /// Title options with themed text colors (Go `ChartOpts.Title`).
    #[must_use]
    pub fn title(&self, title: &str, subtitle: &str) -> Title {
        Title {
            text: title.to_string(),
            subtext: subtitle.to_string(),
            left: "center".to_string(),
            title_style: Some(TextStyle {
                color: self.theme.chart_text.to_string(),
                ..TextStyle::default()
            }),
            subtitle_style: Some(TextStyle {
                color: self.theme.chart_text_muted.to_string(),
                ..TextStyle::default()
            }),
            ..Title::default()
        }
    }

    /// Legend options with themed text color (Go `ChartOpts.Legend`).
    #[must_use]
    pub fn legend(&self) -> Legend {
        Legend {
            show: Some(true),
            type_: "scroll".to_string(),
            top: "10%".to_string(),
            left: "center".to_string(),
            data: None,
            text_style: Some(TextStyle {
                color: self.theme.chart_text_muted.to_string(),
                ..TextStyle::default()
            }),
        }
    }

    /// X-axis options with themed colors (Go `ChartOpts.XAxis`).
    #[must_use]
    pub fn x_axis(&self, name: &str) -> XAxis {
        XAxis {
            name: name.to_string(),
            axis_label: Some(AxisLabel {
                color: self.theme.chart_text_muted.to_string(),
                ..AxisLabel::default()
            }),
            axis_line: Some(AxisLine {
                line_style: Some(LineStyle {
                    color: self.theme.chart_axis.to_string(),
                    ..LineStyle::default()
                }),
                ..AxisLine::default()
            }),
            ..XAxis::default()
        }
    }

    /// Y-axis options with themed colors (Go `ChartOpts.YAxis`).
    #[must_use]
    pub fn y_axis(&self, name: &str) -> YAxis {
        YAxis {
            name: name.to_string(),
            axis_label: Some(AxisLabel {
                color: self.theme.chart_text_muted.to_string(),
                ..AxisLabel::default()
            }),
            axis_line: Some(AxisLine {
                line_style: Some(LineStyle {
                    color: self.theme.chart_axis.to_string(),
                    ..LineStyle::default()
                }),
                ..AxisLine::default()
            }),
            split_line: Some(SplitLine {
                show: Some(true),
                line_style: Some(LineStyle {
                    color: self.theme.chart_grid.to_string(),
                    ..LineStyle::default()
                }),
            }),
            ..YAxis::default()
        }
    }

    /// Grid options with the standard margins (Go `ChartOpts.Grid`).
    #[must_use]
    pub fn grid(&self) -> Grid {
        Grid {
            top: "25%".to_string(),
            bottom: "15%".to_string(),
            left: "5%".to_string(),
            right: "5%".to_string(),
            contain_label: Some(true),
        }
    }

    /// The standard data-zoom pair (Go `ChartOpts.DataZoom`).
    #[must_use]
    pub fn data_zoom(&self) -> Vec<DataZoom> {
        vec![
            DataZoom {
                type_: "slider".to_string(),
                start: 0.0,
                end: DATA_ZOOM_END_PERCENT,
            },
            DataZoom {
                type_: "inside".to_string(),
                ..DataZoom::default()
            },
        ]
    }

    /// Radar component options with themed colors (Go
    /// `ChartOpts.RadarComponent`).
    #[must_use]
    pub fn radar_component(&self, indicator: Vec<Indicator>, split_number: i64) -> RadarComponent {
        RadarComponent {
            indicator,
            shape: "polygon".to_string(),
            split_number,
            split_line: Some(SplitLine {
                show: Some(true),
                line_style: Some(LineStyle {
                    color: self.theme.chart_grid.to_string(),
                    ..LineStyle::default()
                }),
            }),
            split_area: Some(SplitArea { show: Some(true) }),
            axis_line: Some(AxisLine {
                show: Some(true),
                line_style: Some(LineStyle {
                    color: self.theme.chart_axis.to_string(),
                    ..LineStyle::default()
                }),
            }),
            axis_name: Some(AxisName {
                color: self.theme.chart_text_muted.to_string(),
            }),
        }
    }

    /// Tooltip options (Go `ChartOpts.Tooltip`).
    #[must_use]
    pub fn tooltip(&self, trigger: &str) -> Tooltip {
        Tooltip {
            show: Some(true),
            trigger: trigger.to_string(),
        }
    }

    /// The primary chart text color (Go `ChartOpts.TextColor`).
    #[must_use]
    pub fn text_color(&self) -> &'static str {
        self.theme.chart_text
    }

    /// The muted chart text color (Go `ChartOpts.TextMutedColor`).
    #[must_use]
    pub fn text_muted_color(&self) -> &'static str {
        self.theme.chart_text_muted
    }

    /// The chart grid color (Go `ChartOpts.GridColor`).
    #[must_use]
    pub fn grid_color(&self) -> &'static str {
        self.theme.chart_grid
    }

    /// The chart axis color (Go `ChartOpts.AxisColor`).
    #[must_use]
    pub fn axis_color(&self) -> &'static str {
        self.theme.chart_axis
    }
}
