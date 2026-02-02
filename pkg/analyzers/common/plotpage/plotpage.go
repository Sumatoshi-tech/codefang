// Package plotpage provides plot page rendering for analyzer visualizations.
package plotpage

import (
	"bytes"
	"fmt"
	"io"
	"strings"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/opts"
)

const (
	dataZoomEnd   = 100
	labelFontSize = 10
	styleTagLen   = 8 // len("</style>")
)

// Style defines chart dimensions and grid margins.
type Style struct {
	Width      string
	Height     string
	GridLeft   string
	GridRight  string
	GridTop    string
	GridBottom string
}

// DefaultStyle returns the default chart style.
func DefaultStyle() Style {
	return Style{
		Width:      "1200px",
		Height:     "500px",
		GridLeft:   "5%",
		GridRight:  "5%",
		GridTop:    "40",
		GridBottom: "15%",
	}
}

// Hint contains interpretive guidance for a chart section.
type Hint struct {
	Title string
	Items []string
}

// Section represents a chart section within a page.
type Section struct {
	Title    string
	Subtitle string
	Hint     Hint
	Chart    Renderable
}

// Page represents a complete visualization page.
type Page struct {
	Title       string
	Description string
	Style       Style
	Sections    []Section
}

// NewPage creates a new visualization page.
func NewPage(title, description string) *Page {
	return &Page{
		Title:       title,
		Description: description,
		Style:       DefaultStyle(),
	}
}

// Add appends sections to the page.
func (p *Page) Add(sections ...Section) {
	p.Sections = append(p.Sections, sections...)
}

// Render writes the page as HTML.
func (p *Page) Render(w io.Writer) error {
	return HTMLRenderer{}.Render(w, p)
}

// Renderable is the interface for chart components.
type Renderable interface {
	Render(w io.Writer) error
}

// HTMLRenderer renders pages as HTML.
type HTMLRenderer struct {
	ExtraCSS string
}

// Render writes the page as HTML to the writer.
func (r HTMLRenderer) Render(w io.Writer, page *Page) error {
	err := r.writeHeader(w, page)
	if err != nil {
		return err
	}

	for _, section := range page.Sections {
		err = r.writeSection(w, section)
		if err != nil {
			return err
		}
	}

	return r.writeFooter(w)
}

func (r HTMLRenderer) writeHeader(w io.Writer, page *Page) error {
	const tpl = `<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>%s</title>
    <script src="https://go-echarts.github.io/go-echarts-assets/assets/echarts.min.js"></script>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            margin: 0; padding: 20px; background: #f5f5f5;
        }
        .cf-page { max-width: 1250px; margin: 0 auto; }
        .cf-page h1 { text-align: center; color: #333; margin-bottom: 10px; }
        .cf-intro { text-align: center; color: #666; margin-bottom: 30px; font-size: 14px; }
        .cf-card {
            background: white; border-radius: 8px; padding: 20px;
            margin-bottom: 30px; box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }
        .cf-card h2 { font-size: 20px; font-weight: 600; color: #333; margin: 0 0 5px 0; }
        .cf-card > p { font-size: 13px; color: #888; margin: 0 0 15px 0; }
        .cf-chart { overflow-x: auto; }
        .cf-chart > div { margin: 0 auto; }
        .cf-hint {
            background: #f8f9fa; border-left: 4px solid #4CAF50;
            padding: 12px 15px; margin-top: 15px; font-size: 13px; color: #555;
        }
        .cf-hint strong { color: #333; }
        .cf-hint ul { margin: 8px 0 0 0; padding-left: 20px; }
        .cf-hint li { margin: 4px 0; }
        .echart-box { display: block; }
        .echart-box .item { margin: 0 auto; }
%s
    </style>
</head>
<body>
<div class="cf-page">
    <h1>%s</h1>
    <p class="cf-intro">%s</p>
`

	_, err := fmt.Fprintf(w, tpl, esc(page.Title), r.ExtraCSS, esc(page.Title), esc(page.Description))
	if err != nil {
		return fmt.Errorf("write header: %w", err)
	}

	return nil
}

func (r HTMLRenderer) writeSection(w io.Writer, section Section) error {
	chartHTML := renderChart(section.Chart)

	_, err := fmt.Fprintf(w, `
    <div class="cf-card">
        <h2>%s</h2>
        <p>%s</p>
        <div class="cf-chart">%s</div>`, esc(section.Title), esc(section.Subtitle), chartHTML)
	if err != nil {
		return fmt.Errorf("write section header: %w", err)
	}

	if len(section.Hint.Items) > 0 {
		writeHint(w, section.Hint)
	}

	_, err = fmt.Fprintf(w, `
    </div>
`)
	if err != nil {
		return fmt.Errorf("write section footer: %w", err)
	}

	return nil
}

func writeHint(w io.Writer, hint Hint) {
	fmt.Fprintf(w, `
        <div class="cf-hint">`)

	if hint.Title != "" {
		fmt.Fprintf(w, `<strong>%s</strong>`, esc(hint.Title))
	}

	fmt.Fprintf(w, `
            <ul>`)

	for _, item := range hint.Items {
		fmt.Fprintf(w, `
                <li>%s</li>`, item)
	}

	fmt.Fprintf(w, `
            </ul>
        </div>`)
}

func (r HTMLRenderer) writeFooter(w io.Writer) error {
	_, err := fmt.Fprintf(w, `
</div>
</body>
</html>`)
	if err != nil {
		return fmt.Errorf("write footer: %w", err)
	}

	return nil
}

// BarBuilder provides a fluent API for building bar charts.
type BarBuilder struct {
	style Style
	bar   *charts.Bar
}

// NewBarChart creates a new bar chart builder.
func NewBarChart(style Style) *BarBuilder {
	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithTooltipOpts(opts.Tooltip{Show: opts.Bool(true), Trigger: "axis"}),
		charts.WithInitializationOpts(opts.Initialization{Width: style.Width, Height: style.Height}),
		charts.WithGridOpts(opts.Grid{
			Left: style.GridLeft, Right: style.GridRight,
			Top: style.GridTop, Bottom: style.GridBottom,
			ContainLabel: opts.Bool(true),
		}),
		charts.WithDataZoomOpts(
			opts.DataZoom{Type: "slider", Start: 0, End: dataZoomEnd},
			opts.DataZoom{Type: "inside"},
		),
	)

	return &BarBuilder{style: style, bar: bar}
}

// XAxis sets the x-axis labels and rotation.
func (b *BarBuilder) XAxis(labels []string, rotate float64) *BarBuilder {
	b.bar.SetGlobalOptions(charts.WithXAxisOpts(opts.XAxis{
		AxisLabel: &opts.AxisLabel{Rotate: rotate, Interval: "0", FontSize: labelFontSize},
	}))
	b.bar.SetXAxis(labels)

	return b
}

// YAxis sets the y-axis name.
func (b *BarBuilder) YAxis(name string) *BarBuilder {
	b.bar.SetGlobalOptions(charts.WithYAxisOpts(opts.YAxis{Name: name}))

	return b
}

// Legend enables the chart legend.
func (b *BarBuilder) Legend() *BarBuilder {
	b.bar.SetGlobalOptions(charts.WithLegendOpts(opts.Legend{Show: opts.Bool(true), Top: "0"}))

	return b
}

// Series adds a data series to the chart.
func (b *BarBuilder) Series(name string, data []int, color string) *BarBuilder {
	barData := make([]opts.BarData, len(data))

	for i, v := range data {
		barData[i] = opts.BarData{Value: v}
	}

	b.bar.AddSeries(name, barData, charts.WithItemStyleOpts(opts.ItemStyle{Color: color}))

	return b
}

// Build returns the constructed bar chart.
func (b *BarBuilder) Build() *charts.Bar {
	return b.bar
}

func renderChart(chart Renderable) string {
	if chart == nil {
		return ""
	}

	var buf bytes.Buffer

	err := chart.Render(&buf)
	if err != nil {
		return ""
	}

	return extractChartContent(buf.String())
}

func extractChartContent(html string) string {
	start := strings.Index(html, `<div class="container">`)
	if start == -1 {
		return html
	}

	end := strings.Index(html, `</body>`)
	if end == -1 {
		return html
	}

	content := html[start:end]
	content = strings.ReplaceAll(content, `class="container"`, `class="echart-box"`)
	content = removeStyleTags(content)

	return content
}

func removeStyleTags(content string) string {
	for {
		i := strings.Index(content, `<style>`)
		if i == -1 {
			break
		}

		j := strings.Index(content[i:], `</style>`)
		if j == -1 {
			break
		}

		content = content[:i] + content[i+j+styleTagLen:]
	}

	return content
}

func esc(s string) string {
	s = strings.ReplaceAll(s, "&", "&amp;")
	s = strings.ReplaceAll(s, "<", "&lt;")
	s = strings.ReplaceAll(s, ">", "&gt;")
	s = strings.ReplaceAll(s, `"`, "&quot;")

	return s
}
