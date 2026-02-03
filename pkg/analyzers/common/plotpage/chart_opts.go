package plotpage

import (
	"github.com/go-echarts/go-echarts/v2/opts"
)

// DataZoom defaults.
const dataZoomEndPercent = 100

// ChartOpts provides themed chart options based on the current theme.
type ChartOpts struct {
	theme ThemeConfig
}

// NewChartOpts creates a new ChartOpts with the given theme.
func NewChartOpts(theme Theme) *ChartOpts {
	return &ChartOpts{theme: GetThemeConfig(theme)}
}

// DefaultChartOpts returns chart options for the default dark theme.
func DefaultChartOpts() *ChartOpts {
	return NewChartOpts(ThemeDark)
}

// Init returns initialization options with themed background.
func (c *ChartOpts) Init(width, height string) opts.Initialization {
	return opts.Initialization{
		Width:           width,
		Height:          height,
		BackgroundColor: c.theme.ChartBackground,
		Theme:           c.theme.EChartsTheme,
	}
}

// Title returns title options with themed text colors.
func (c *ChartOpts) Title(title, subtitle string) opts.Title {
	return opts.Title{
		Title:         title,
		Subtitle:      subtitle,
		Left:          "center",
		TitleStyle:    &opts.TextStyle{Color: c.theme.ChartText},
		SubtitleStyle: &opts.TextStyle{Color: c.theme.ChartTextMuted},
	}
}

// Legend returns legend options with themed text color.
func (c *ChartOpts) Legend() opts.Legend {
	return opts.Legend{
		Show:      opts.Bool(true),
		Type:      "scroll",
		Top:       "10%",
		Left:      "center",
		TextStyle: &opts.TextStyle{Color: c.theme.ChartTextMuted},
	}
}

// XAxis returns x-axis options with themed colors.
func (c *ChartOpts) XAxis(name string) opts.XAxis {
	return opts.XAxis{
		Name:      name,
		AxisLabel: &opts.AxisLabel{Color: c.theme.ChartTextMuted},
		AxisLine:  &opts.AxisLine{LineStyle: &opts.LineStyle{Color: c.theme.ChartAxis}},
	}
}

// YAxis returns y-axis options with themed colors.
func (c *ChartOpts) YAxis(name string) opts.YAxis {
	return opts.YAxis{
		Name:      name,
		AxisLabel: &opts.AxisLabel{Color: c.theme.ChartTextMuted},
		AxisLine:  &opts.AxisLine{LineStyle: &opts.LineStyle{Color: c.theme.ChartAxis}},
		SplitLine: &opts.SplitLine{
			Show:      opts.Bool(true),
			LineStyle: &opts.LineStyle{Color: c.theme.ChartGrid},
		},
	}
}

// Grid returns grid options with standard margins.
func (c *ChartOpts) Grid() opts.Grid {
	return opts.Grid{
		Top:          "25%",
		Bottom:       "15%",
		Left:         "5%",
		Right:        "5%",
		ContainLabel: opts.Bool(true),
	}
}

// GridCompact returns grid options with smaller top margin.
func (c *ChartOpts) GridCompact() opts.Grid {
	return opts.Grid{
		Top:          "20%",
		Bottom:       "15%",
		Left:         "5%",
		Right:        "5%",
		ContainLabel: opts.Bool(true),
	}
}

// DataZoom returns standard data zoom options.
func (c *ChartOpts) DataZoom() []opts.DataZoom {
	return []opts.DataZoom{
		{Type: "slider", Start: 0, End: dataZoomEndPercent},
		{Type: "inside"},
	}
}

// Tooltip returns tooltip options.
func (c *ChartOpts) Tooltip(trigger string) opts.Tooltip {
	return opts.Tooltip{Show: opts.Bool(true), Trigger: trigger}
}

// RadarComponent returns radar component options with themed colors.
func (c *ChartOpts) RadarComponent(indicators []*opts.Indicator, splitNumber int) opts.RadarComponent {
	return opts.RadarComponent{
		Indicator:   indicators,
		Shape:       "polygon",
		SplitNumber: splitNumber,
		SplitLine:   &opts.SplitLine{Show: opts.Bool(true), LineStyle: &opts.LineStyle{Color: c.theme.ChartGrid}},
		SplitArea:   &opts.SplitArea{Show: opts.Bool(true)},
		AxisLine:    &opts.AxisLine{Show: opts.Bool(true), LineStyle: &opts.LineStyle{Color: c.theme.ChartAxis}},
		AxisName:    &opts.AxisName{Color: c.theme.ChartTextMuted},
	}
}

// TextColor returns the primary chart text color.
func (c *ChartOpts) TextColor() string {
	return c.theme.ChartText
}

// TextMutedColor returns the muted chart text color.
func (c *ChartOpts) TextMutedColor() string {
	return c.theme.ChartTextMuted
}

// GridColor returns the chart grid color.
func (c *ChartOpts) GridColor() string {
	return c.theme.ChartGrid
}

// AxisColor returns the chart axis color.
func (c *ChartOpts) AxisColor() string {
	return c.theme.ChartAxis
}
