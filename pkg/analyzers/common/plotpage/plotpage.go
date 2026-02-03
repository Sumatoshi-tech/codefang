package plotpage

import (
	"bytes"
	"fmt"
	"html/template"
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
		Width:      "100%",
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
	Title           string
	Description     string
	ProjectName     string
	ProjectSubtitle string
	ShowThemeToggle bool
	Style           Style
	Theme           Theme
	Sections        []Section
}

// NewPage creates a new visualization page.
func NewPage(title, description string) *Page {
	return &Page{
		Title:           title,
		Description:     description,
		ProjectName:     "Codefang",
		ProjectSubtitle: "Code Analysis",
		ShowThemeToggle: true,
		Style:           DefaultStyle(),
		Theme:           ThemeDark,
	}
}

// WithTheme sets the theme for the page.
func (p *Page) WithTheme(theme Theme) *Page {
	p.Theme = theme

	return p
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
	themeConfig := GetThemeConfig(page.Theme)

	header, err := renderTemplate("header.html", headerData{
		ProjectName:     page.ProjectName,
		Subtitle:        page.ProjectSubtitle,
		Title:           page.Title,
		Description:     page.Description,
		ShowThemeToggle: page.ShowThemeToggle,
		LogoDataURI:     LogoDataURI(),
	})
	if err != nil {
		return fmt.Errorf("render header: %w", err)
	}

	var sectionsHTML bytes.Buffer

	for _, section := range page.Sections {
		sectionHTML, sectionErr := r.renderSection(section)
		if sectionErr != nil {
			return fmt.Errorf("render section: %w", sectionErr)
		}

		sectionsHTML.WriteString(string(sectionHTML))
	}

	scripts, err := renderTemplate("scripts.html", nil)
	if err != nil {
		return fmt.Errorf("render scripts: %w", err)
	}

	darkClass := ""
	if page.Theme == ThemeDark {
		darkClass = "dark"
	}

	data := pageData{
		Title:       page.Title,
		Description: page.Description,
		ProjectName: page.ProjectName,
		DarkClass:   darkClass,
		Theme:       themeConfig,
		ExtraCSS:    template.CSS(r.ExtraCSS),
		Header:      header,
		Content:     template.HTML(sectionsHTML.String()),
		Scripts:     scripts,
	}

	html, err := renderTemplate("page.html", data)
	if err != nil {
		return fmt.Errorf("render page: %w", err)
	}

	_, err = w.Write([]byte(html))
	if err != nil {
		return fmt.Errorf("writing page: %w", err)
	}

	return nil
}

func (r HTMLRenderer) renderSection(section Section) (template.HTML, error) {
	chartHTML := renderChart(section.Chart)

	var hint *hintData

	if len(section.Hint.Items) > 0 {
		items := make([]template.HTML, len(section.Hint.Items))

		for i, item := range section.Hint.Items {
			items[i] = template.HTML(item)
		}

		hint = &hintData{
			Title: section.Hint.Title,
			Items: items,
		}
	}

	data := sectionData{
		Title:    section.Title,
		Subtitle: section.Subtitle,
		Chart:    template.HTML(chartHTML),
		Hint:     hint,
	}

	return renderTemplate("section.html", data)
}

// BarBuilder provides a fluent API for building bar charts.
type BarBuilder struct {
	style Style
	theme Theme
	bar   *charts.Bar
}

// NewBarChart creates a new bar chart builder.
func NewBarChart(style Style) *BarBuilder {
	return NewBarChartWithTheme(style, ThemeDark)
}

// NewBarChartWithTheme creates a new bar chart builder with theme support.
func NewBarChartWithTheme(style Style, theme Theme) *BarBuilder {
	themeConfig := GetThemeConfig(theme)
	bar := charts.NewBar()

	initOpts := opts.Initialization{
		Width:  style.Width,
		Height: style.Height,
	}

	if themeConfig.EChartsTheme != "" {
		initOpts.Theme = themeConfig.EChartsTheme
	}

	bar.SetGlobalOptions(
		charts.WithTooltipOpts(opts.Tooltip{Show: opts.Bool(true), Trigger: "axis"}),
		charts.WithInitializationOpts(initOpts),
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

	return &BarBuilder{style: style, theme: theme, bar: bar}
}

// XAxis sets the x-axis labels and rotation.
func (b *BarBuilder) XAxis(labels []string, rotate float64) *BarBuilder {
	themeConfig := GetThemeConfig(b.theme)
	b.bar.SetGlobalOptions(charts.WithXAxisOpts(opts.XAxis{
		AxisLabel: &opts.AxisLabel{
			Rotate:   rotate,
			Interval: "0",
			FontSize: labelFontSize,
			Color:    themeConfig.ChartText,
		},
		AxisLine: &opts.AxisLine{
			LineStyle: &opts.LineStyle{Color: themeConfig.ChartAxis},
		},
	}))
	b.bar.SetXAxis(labels)

	return b
}

// YAxis sets the y-axis name.
func (b *BarBuilder) YAxis(name string) *BarBuilder {
	themeConfig := GetThemeConfig(b.theme)
	b.bar.SetGlobalOptions(charts.WithYAxisOpts(opts.YAxis{
		Name:      name,
		AxisLabel: &opts.AxisLabel{Color: themeConfig.ChartText},
		SplitLine: &opts.SplitLine{
			LineStyle: &opts.LineStyle{Color: themeConfig.ChartGrid},
		},
	}))

	return b
}

// Legend enables the chart legend.
func (b *BarBuilder) Legend() *BarBuilder {
	themeConfig := GetThemeConfig(b.theme)
	b.bar.SetGlobalOptions(charts.WithLegendOpts(opts.Legend{
		Show:      opts.Bool(true),
		Top:       "0",
		TextStyle: &opts.TextStyle{Color: themeConfig.ChartText},
	}))

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

// ChartWrapper wraps an echarts chart and renders only the chart content.
type ChartWrapper struct {
	chart Renderable
}

// WrapChart wraps an echarts chart to render only the div and script (no full HTML page).
func WrapChart(chart Renderable) *ChartWrapper {
	return &ChartWrapper{chart: chart}
}

// Render writes the chart element and script without a full HTML page.
func (cw *ChartWrapper) Render(w io.Writer) error {
	if cw.chart == nil {
		return nil
	}

	var buf bytes.Buffer

	err := cw.chart.Render(&buf)
	if err != nil {
		return fmt.Errorf("rendering chart: %w", err)
	}

	content := extractChartContent(buf.String())

	_, err = w.Write([]byte(content))
	if err != nil {
		return fmt.Errorf("writing chart content: %w", err)
	}

	return nil
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
	// Only extract chart content from full HTML pages (echarts output).
	// If the content doesn't start with DOCTYPE, it's already a component fragment.
	if !strings.HasPrefix(strings.TrimSpace(html), "<!DOCTYPE") &&
		!strings.HasPrefix(strings.TrimSpace(html), "<html") {
		return html
	}

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
