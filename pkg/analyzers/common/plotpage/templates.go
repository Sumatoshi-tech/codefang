package plotpage

import (
	"bytes"
	"embed"
	"encoding/base64"
	"fmt"
	"html/template"
	"sync"
)

//go:embed templates/*.html
var templateFS embed.FS

//go:embed assets/uast_small.png
var logoPNG []byte

var (
	templates     *template.Template
	templatesOnce sync.Once
	errTemplates  error
)

// funcMap provides template function helpers.
var funcMap = template.FuncMap{
	"odd": func(i int) bool {
		return i%2 == 1
	},
}

// getTemplates returns the parsed templates, loading them once.
func getTemplates() (*template.Template, error) {
	templatesOnce.Do(func() {
		var parseErr error

		templates, parseErr = template.New("").
			Funcs(funcMap).
			ParseFS(templateFS, "templates/*.html")
		if parseErr != nil {
			errTemplates = fmt.Errorf("parsing templates: %w", parseErr)
		}
	})

	return templates, errTemplates
}

// renderTemplate renders a named template with the given data.
func renderTemplate(name string, data any) (template.HTML, error) {
	tmpl, err := getTemplates()
	if err != nil {
		return "", fmt.Errorf("loading templates: %w", err)
	}

	var buf bytes.Buffer

	err = tmpl.ExecuteTemplate(&buf, name, data)
	if err != nil {
		return "", fmt.Errorf("executing template %s: %w", name, err)
	}

	return template.HTML(buf.String()), nil
}

// mustRenderTemplate renders a template, panicking on error.
// Use only when errors are not expected (e.g., embedded templates).
func mustRenderTemplate(name string, data any) template.HTML {
	html, err := renderTemplate(name, data)
	if err != nil {
		panic("plotpage: template error: " + err.Error())
	}

	return html
}

// pageData holds data for the page template.
type pageData struct {
	Title       string
	Description string
	ProjectName string
	DarkClass   string
	Theme       ThemeConfig
	ExtraCSS    template.CSS
	Header      template.HTML
	Content     template.HTML
	Scripts     template.HTML
}

// headerData holds data for the header template.
type headerData struct {
	ProjectName     string
	Subtitle        string
	Title           string
	Description     string
	ShowThemeToggle bool
	LogoDataURI     template.URL
}

// LogoDataURI returns the logo as a data URI for embedding in HTML.
func LogoDataURI() template.URL {
	return template.URL("data:image/png;base64," + base64.StdEncoding.EncodeToString(logoPNG))
}

// sectionData holds data for the section template.
type sectionData struct {
	Title    string
	Subtitle string
	Chart    template.HTML
	Hint     *hintData
}

// hintData holds data for hints within sections.
type hintData struct {
	Title string
	Items []template.HTML
}

// cardData holds data for the card template.
type cardData struct {
	Title    string
	Subtitle string
	Content  template.HTML
}

// tabsData holds data for the tabs template.
type tabsData struct {
	ID    string
	Items []tabItemData
}

// tabItemData holds data for individual tab items.
type tabItemData struct {
	ID      string
	Label   string
	Content template.HTML
}

// badgeData holds data for the badge template.
type badgeData struct {
	Text    string
	Classes string
}

// gridData holds data for the grid template.
type gridData struct {
	ColClass string
	Gap      string
	Items    []template.HTML
}

// statData holds data for the stat template.
type statData struct {
	Label      string
	Value      string
	Trend      string
	TrendClass string
}

// alertData holds data for the alert template.
type alertData struct {
	Title       string
	Message     string
	BgClass     string
	BorderClass string
	TitleClass  string
	TextClass   string
}

// tableData holds data for the table template.
type tableData struct {
	Headers []string
	Rows    [][]template.HTML
	Striped bool
}
