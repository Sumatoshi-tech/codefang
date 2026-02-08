package devs

import (
	"io"
	"strconv"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
)

// Visualization constants (rendering-specific, not metrics).
const (
	topDevsForRadar      = 10
	topDevsForTreemap    = 30
	topLanguagesForRadar = 8
	treemapHeight        = "600px"
	radarHeight          = "500px"
	lineChartHeight      = "500px"
	churnChartHeight     = "450px"
	riskTableMaxRows     = 20

	overviewTableLimit = 10

	dataZoomEnd       = 100
	areaOpacityNormal = 0.6
	areaOpacityOther  = 0.4
	radarSplitNum     = 5
	radarAreaOpacity  = 0.2
	lineWidth         = 2
	treemapLeafDepth  = 2
	borderWidth       = 2
	gapWidth          = 2
)

// DashboardData wraps ComputedMetrics with rendering-specific data.
type DashboardData struct {
	Metrics      *ComputedMetrics
	TopLanguages []string // Top N language names for radar chart
}

// GenerateDashboard creates the developer analytics dashboard HTML.
func GenerateDashboard(report analyze.Report, writer io.Writer) error {
	sections, err := GenerateSections(report)
	if err != nil {
		return err
	}

	page := plotpage.NewPage(
		"Developer Analytics Dashboard",
		"Comprehensive analysis of developer contributions, expertise, and team health",
	)

	page.Add(sections...)

	return page.Render(writer)
}

// GenerateSections returns the dashboard sections without rendering.
func GenerateSections(report analyze.Report) ([]plotpage.Section, error) {
	data, err := newDashboardData(report)
	if err != nil {
		return nil, err
	}

	tabs := createDashboardTabs(data)

	return []plotpage.Section{
		{
			Title:    "Developer Analytics",
			Subtitle: "Multi-dimensional view of team contributions and codebase ownership",
			Chart:    tabs,
		},
	}, nil
}

func newDashboardData(report analyze.Report) (*DashboardData, error) {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		return nil, err
	}

	// Extract top language names for radar chart.
	topLangs := make([]string, 0, topLanguagesForRadar)

	for i, ld := range metrics.Languages {
		if i >= topLanguagesForRadar {
			break
		}

		topLangs = append(topLangs, ld.Name)
	}

	return &DashboardData{
		Metrics:      metrics,
		TopLanguages: topLangs,
	}, nil
}

func createDashboardTabs(data *DashboardData) *plotpage.Tabs {
	return plotpage.NewTabs("dashboard",
		plotpage.TabItem{ID: "overview", Label: "Overview", Content: createOverviewTab(data)},
		plotpage.TabItem{ID: "activity", Label: "Activity Trends", Content: createActivityTab(data)},
		plotpage.TabItem{ID: "workload", Label: "Workload Distribution", Content: createWorkloadTab(data)},
		plotpage.TabItem{ID: "languages", Label: "Language Expertise", Content: createLanguagesTab(data)},
		plotpage.TabItem{ID: "busfactor", Label: "Bus Factor", Content: createBusFactorTab(data)},
		plotpage.TabItem{ID: "churn", Label: "Code Churn", Content: createChurnTab(data)},
	)
}

// Formatting utilities for dashboard display.

func formatNumber(n int) string {
	if n < 0 {
		return "-" + formatNumber(-n)
	}

	const (
		million  = 1000000
		thousand = 1000
	)

	switch {
	case n >= million:
		return strconv.FormatFloat(float64(n)/float64(million), 'f', 1, 64) + "M"
	case n >= thousand:
		return strconv.FormatFloat(float64(n)/float64(thousand), 'f', 1, 64) + "K"
	default:
		return strconv.Itoa(n)
	}
}

func formatSignedNumber(n int) string {
	if n > 0 {
		return "+" + formatNumber(n)
	}

	return formatNumber(n)
}

func anonymizeNames(names []string) []string {
	result := make([]string, len(names))

	for i := range names {
		result[i] = "Developer-" + anonymousID(i)
	}

	return result
}

func anonymousID(index int) string {
	const letters = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"

	if index < len(letters) {
		return string(letters[index])
	}

	first := index / len(letters)
	second := index % len(letters)

	return string(letters[first-1]) + string(letters[second])
}

// IdentityAuditEntry represents a developer identity for auditing.
type IdentityAuditEntry struct {
	CanonicalName string
	CommitCount   int
}

// GenerateIdentityAudit creates an audit list of developer identities.
func GenerateIdentityAudit(report analyze.Report) []IdentityAuditEntry {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		return nil
	}

	entries := make([]IdentityAuditEntry, len(metrics.Developers))
	for i, d := range metrics.Developers {
		entries[i] = IdentityAuditEntry{
			CanonicalName: d.Name,
			CommitCount:   d.Commits,
		}
	}

	return entries
}
