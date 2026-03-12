// Package plotpage provides HTML visualization components for analyzer output.
package plotpage

import (
	"bytes"
	"fmt"
	"html/template"
	"io"
)

const (
	maxGridColumns = 4
)

// BadgeVariant defines badge styling variants.
type BadgeVariant string

// Badge variant constants.
const (
	BadgeSolid   BadgeVariant = "solid"
	BadgeSoft    BadgeVariant = "soft"
	BadgeOutline BadgeVariant = "outline"
)

// BadgeColor defines badge colors.
type BadgeColor string

// Badge color constants.
const (
	BadgeDefault BadgeColor = "default"
	BadgeAccent  BadgeColor = "accent"
	BadgeSuccess BadgeColor = "success"
	BadgeWarning BadgeColor = "warning"
	BadgeError   BadgeColor = "error"
	BadgeInfo    BadgeColor = "info"
)

// TabItem represents a single tab in a tab group.
type TabItem struct {
	ID      string
	Label   string
	Content Renderable
}

// Tabs renders a tabbed interface.
type Tabs struct {
	ID    string
	Items []TabItem
}

// NewTabs creates a new tab group.
func NewTabs(id string, items ...TabItem) *Tabs {
	return &Tabs{ID: id, Items: items}
}

// Render writes the tabs HTML.
func (t *Tabs) Render(w io.Writer) error {
	if len(t.Items) == 0 {
		return nil
	}

	items := make([]tabItemData, len(t.Items))

	for i, item := range t.Items {
		var content template.HTML

		if item.Content != nil {
			var buf bytes.Buffer

			err := item.Content.Render(&buf)
			if err != nil {
				return fmt.Errorf("rendering tab %s: %w", item.ID, err)
			}

			content = template.HTML(buf.String())
		}

		items[i] = tabItemData{
			ID:      item.ID,
			Label:   item.Label,
			Content: content,
		}
	}

	html := mustRenderTemplate("tabs.html", tabsData{
		ID:    t.ID,
		Items: items,
	})

	_, err := w.Write([]byte(html))
	if err != nil {
		return fmt.Errorf("writing tabs: %w", err)
	}

	return nil
}

// Card renders a card container.
type Card struct {
	Title    string
	Subtitle string
	Content  Renderable
}

// NewCard creates a new card.
func NewCard(title, subtitle string) *Card {
	return &Card{Title: title, Subtitle: subtitle}
}

// WithContent sets the card content.
func (c *Card) WithContent(content Renderable) *Card {
	c.Content = content

	return c
}

// Render writes the card HTML.
func (c *Card) Render(w io.Writer) error {
	data := cardData{
		Title:    c.Title,
		Subtitle: c.Subtitle,
	}

	if c.Content != nil {
		var buf bytes.Buffer

		err := c.Content.Render(&buf)
		if err != nil {
			return fmt.Errorf("rendering card content: %w", err)
		}

		data.Content = template.HTML(buf.String())
	}

	html := mustRenderTemplate("card.html", data)

	_, err := w.Write([]byte(html))
	if err != nil {
		return fmt.Errorf("writing card: %w", err)
	}

	return nil
}

// Badge renders an inline badge/tag.
type Badge struct {
	Text    string
	Variant BadgeVariant
	Color   BadgeColor
}

// NewBadge creates a new badge.
func NewBadge(text string) *Badge {
	return &Badge{Text: text, Variant: BadgeSoft, Color: BadgeDefault}
}

// WithColor sets the badge color.
func (b *Badge) WithColor(c BadgeColor) *Badge {
	b.Color = c

	return b
}

// Render writes the badge HTML.
func (b *Badge) Render(w io.Writer) error {
	html := mustRenderTemplate("badge.html", badgeData{
		Text:    b.Text,
		Classes: b.getClasses(),
	})

	_, err := w.Write([]byte(html))
	if err != nil {
		return fmt.Errorf("writing badge: %w", err)
	}

	return nil
}

func (b *Badge) getClasses() string {
	switch b.Variant {
	case BadgeSolid:
		return b.getSolidClasses()
	case BadgeOutline:
		return b.getOutlineClasses()
	case BadgeSoft:
		return b.getSoftClasses()
	default:
		return b.getSoftClasses()
	}
}

func (b *Badge) getSolidClasses() string {
	switch b.Color {
	case BadgeAccent:
		return "bg-amber-600 text-white"
	case BadgeSuccess:
		return "bg-green-600 text-white"
	case BadgeWarning:
		return "bg-yellow-500 text-white"
	case BadgeError:
		return "bg-red-600 text-white"
	case BadgeInfo:
		return "bg-blue-600 text-white"
	case BadgeDefault:
		return "bg-stone-600 text-white dark:bg-stone-400 dark:text-stone-900"
	default:
		return "bg-stone-600 text-white dark:bg-stone-400 dark:text-stone-900"
	}
}

func (b *Badge) getOutlineClasses() string {
	switch b.Color {
	case BadgeAccent:
		return "border border-amber-600 text-amber-700 dark:text-amber-400"
	case BadgeSuccess:
		return "border border-green-600 text-green-700 dark:text-green-400"
	case BadgeWarning:
		return "border border-yellow-500 text-yellow-700 dark:text-yellow-400"
	case BadgeError:
		return "border border-red-600 text-red-700 dark:text-red-400"
	case BadgeInfo:
		return "border border-blue-600 text-blue-700 dark:text-blue-400"
	case BadgeDefault:
		return "border border-stone-400 text-stone-600 dark:text-stone-400"
	default:
		return "border border-stone-400 text-stone-600 dark:text-stone-400"
	}
}

func (b *Badge) getSoftClasses() string {
	switch b.Color {
	case BadgeAccent:
		return "bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200"
	case BadgeSuccess:
		return "bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200"
	case BadgeWarning:
		return "bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-200"
	case BadgeError:
		return "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200"
	case BadgeInfo:
		return "bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200"
	case BadgeDefault:
		return "bg-stone-100 text-stone-800 dark:bg-stone-800 dark:text-stone-200"
	default:
		return "bg-stone-100 text-stone-800 dark:bg-stone-800 dark:text-stone-200"
	}
}

// Text renders plain text content.
type Text struct {
	Content string
}

// NewText creates a new text block.
func NewText(content string) *Text {
	return &Text{Content: content}
}

// Render writes the text content.
func (t *Text) Render(w io.Writer) error {
	_, err := w.Write([]byte(template.HTMLEscapeString(t.Content)))
	if err != nil {
		return fmt.Errorf("writing text: %w", err)
	}

	return nil
}

// Grid renders a responsive grid layout.
type Grid struct {
	Columns int
	Gap     string
	Items   []Renderable
}

// NewGrid creates a new grid layout.
func NewGrid(columns int, items ...Renderable) *Grid {
	if columns < 1 {
		columns = 1
	}

	if columns > maxGridColumns {
		columns = maxGridColumns
	}

	return &Grid{Columns: columns, Gap: "gap-4", Items: items}
}

// Render writes the grid HTML.
func (g *Grid) Render(w io.Writer) error {
	colClass := map[int]string{
		1: "grid-cols-1",
		2: "grid-cols-1 md:grid-cols-2",
		3: "grid-cols-1 md:grid-cols-2 lg:grid-cols-3",
		4: "grid-cols-1 md:grid-cols-2 lg:grid-cols-4",
	}[g.Columns]

	items := make([]template.HTML, len(g.Items))

	for i, item := range g.Items {
		if item != nil {
			var buf bytes.Buffer

			err := item.Render(&buf)
			if err != nil {
				return fmt.Errorf("rendering grid item %d: %w", i, err)
			}

			items[i] = template.HTML(buf.String())
		}
	}

	html := mustRenderTemplate("grid.html", gridData{
		ColClass: colClass,
		Gap:      g.Gap,
		Items:    items,
	})

	_, err := w.Write([]byte(html))
	if err != nil {
		return fmt.Errorf("writing grid: %w", err)
	}

	return nil
}

// Stat renders a statistic/metric display.
type Stat struct {
	Label string
	Value string
	Trend string
	Color BadgeColor
}

// NewStat creates a new stat display.
func NewStat(label, value string) *Stat {
	return &Stat{Label: label, Value: value}
}

// WithTrend sets the trend indicator.
func (s *Stat) WithTrend(trend string, color BadgeColor) *Stat {
	s.Trend = trend
	s.Color = color

	return s
}

// Render writes the stat HTML.
func (s *Stat) Render(w io.Writer) error {
	trendClass := "text-stone-500"

	switch s.Color {
	case BadgeSuccess:
		trendClass = "text-green-600 dark:text-green-400"
	case BadgeError:
		trendClass = "text-red-600 dark:text-red-400"
	case BadgeWarning:
		trendClass = "text-yellow-600 dark:text-yellow-400"
	case BadgeDefault, BadgeAccent, BadgeInfo:
		trendClass = "text-stone-500"
	}

	html := mustRenderTemplate("stat.html", statData{
		Label:      s.Label,
		Value:      s.Value,
		Trend:      s.Trend,
		TrendClass: trendClass,
	})

	_, err := w.Write([]byte(html))
	if err != nil {
		return fmt.Errorf("writing stat: %w", err)
	}

	return nil
}

// Alert renders an alert/notification box.
type Alert struct {
	Title   string
	Message string
	Color   BadgeColor
}

// NewAlert creates a new alert.
func NewAlert(title, message string, color BadgeColor) *Alert {
	return &Alert{Title: title, Message: message, Color: color}
}

// Render writes the alert HTML.
func (a *Alert) Render(w io.Writer) error {
	var bgClass, borderClass, textClass, titleClass string

	switch a.Color {
	case BadgeSuccess:
		bgClass = "bg-green-50 dark:bg-green-950"
		borderClass = "border-green-500"
		textClass = "text-green-700 dark:text-green-300"
		titleClass = "text-green-800 dark:text-green-200"
	case BadgeWarning:
		bgClass = "bg-yellow-50 dark:bg-yellow-950"
		borderClass = "border-yellow-500"
		textClass = "text-yellow-700 dark:text-yellow-300"
		titleClass = "text-yellow-800 dark:text-yellow-200"
	case BadgeError:
		bgClass = "bg-red-50 dark:bg-red-950"
		borderClass = "border-red-500"
		textClass = "text-red-700 dark:text-red-300"
		titleClass = "text-red-800 dark:text-red-200"
	case BadgeInfo:
		bgClass = "bg-blue-50 dark:bg-blue-950"
		borderClass = "border-blue-500"
		textClass = "text-blue-700 dark:text-blue-300"
		titleClass = "text-blue-800 dark:text-blue-200"
	case BadgeDefault, BadgeAccent:
		bgClass = "bg-stone-50 dark:bg-stone-900"
		borderClass = "border-stone-500"
		textClass = "text-stone-700 dark:text-stone-300"
		titleClass = "text-stone-800 dark:text-stone-200"
	default:
		bgClass = "bg-stone-50 dark:bg-stone-900"
		borderClass = "border-stone-500"
		textClass = "text-stone-700 dark:text-stone-300"
		titleClass = "text-stone-800 dark:text-stone-200"
	}

	html := mustRenderTemplate("alert.html", alertData{
		Title:       a.Title,
		Message:     a.Message,
		BgClass:     bgClass,
		BorderClass: borderClass,
		TitleClass:  titleClass,
		TextClass:   textClass,
	})

	_, err := w.Write([]byte(html))
	if err != nil {
		return fmt.Errorf("writing alert: %w", err)
	}

	return nil
}

// Table renders an HTML table.
type Table struct {
	Headers []string
	Rows    [][]string
	Striped bool
}

// NewTable creates a new table.
func NewTable(headers []string) *Table {
	return &Table{Headers: headers, Striped: true}
}

// AddRow adds a row to the table.
func (t *Table) AddRow(cells ...string) *Table {
	t.Rows = append(t.Rows, cells)

	return t
}

// WithStriped enables/disables striping.
func (t *Table) WithStriped(striped bool) *Table {
	t.Striped = striped

	return t
}

// Render writes the table HTML.
func (t *Table) Render(w io.Writer) error {
	// Convert string rows to template.HTML to allow raw HTML in cells.
	htmlRows := make([][]template.HTML, len(t.Rows))
	for i, row := range t.Rows {
		htmlRows[i] = make([]template.HTML, len(row))
		for j, cell := range row {
			htmlRows[i][j] = template.HTML(cell)
		}
	}

	html := mustRenderTemplate("table.html", tableData{
		Headers: t.Headers,
		Rows:    htmlRows,
		Striped: t.Striped,
	})

	_, err := w.Write([]byte(html))
	if err != nil {
		return fmt.Errorf("writing table: %w", err)
	}

	return nil
}
