package devs

import (
	"io"
	"sort"
	"strconv"
	"time"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

const (
	topDevsForRadar      = 10
	topDevsForTreemap    = 30
	topLanguagesForRadar = 8
	treemapHeight        = "600px"
	radarHeight          = "500px"
	lineChartHeight      = "500px"
	churnChartHeight     = "450px"
	riskTableMaxRows     = 20

	langOther      = "Other"
	riskCritical   = "CRITICAL"
	riskHigh       = "HIGH"
	riskMedium     = "MEDIUM"
	riskLow        = "LOW"
	recentActivity = 0.7

	defaultTickHours      = 24
	percentMultiplier     = 100
	riskThresholdCritical = 90
	riskThresholdHigh     = 80
	riskThresholdMedium   = 60

	riskPriorityHigh   = 1
	riskPriorityMedium = 2
	riskPriorityLow    = 3

	overviewTableLimit = 10
	statsGridCols      = 4

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

// DeveloperSummary aggregates metrics for a single developer.
type DeveloperSummary struct {
	ID          int
	Name        string
	Commits     int
	Added       int
	Removed     int
	Changed     int
	NetLines    int
	Languages   map[string]pkgplumbing.LineStats
	FirstTick   int
	LastTick    int
	ActiveTicks int
}

// LanguageSummary aggregates metrics for a single language.
type LanguageSummary struct {
	Name       string
	TotalLines int
	Developers map[int]int
}

// BusFactorEntry represents bus factor risk for a language.
type BusFactorEntry struct {
	Language     string
	PrimaryDev   string
	PrimaryPct   float64
	SecondaryDev string
	SecondaryPct float64
	RiskLevel    string
}

// DashboardData holds all computed data for the dashboard.
type DashboardData struct {
	Ticks             map[int]map[int]*DevTick
	Names             []string
	TickSize          time.Duration
	DevSummaries      []DeveloperSummary
	LanguageSummaries []LanguageSummary
	BusFactorEntries  []BusFactorEntry
	TotalCommits      int
	TotalAdded        int
	TotalRemoved      int
	TotalDevelopers   int
	ActiveDevelopers  int
	TopLanguages      []string
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
	data, err := computeDashboardData(report)
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

func computeDashboardData(report analyze.Report) (*DashboardData, error) {
	ticks, ok := report["Ticks"].(map[int]map[int]*DevTick)
	if !ok {
		return nil, ErrInvalidTicks
	}

	names, ok := report["ReversedPeopleDict"].([]string)
	if !ok {
		return nil, ErrInvalidPeopleDict
	}

	tickSize, ok := report["TickSize"].(time.Duration)
	if !ok || tickSize == 0 {
		tickSize = defaultTickHours * time.Hour
	}

	data := &DashboardData{
		Ticks:    ticks,
		Names:    names,
		TickSize: tickSize,
	}

	computeDevSummaries(data)
	computeLanguageSummaries(data)
	computeBusFactorEntries(data)
	computeAggregates(data)

	return data, nil
}

func computeDevSummaries(data *DashboardData) {
	devMap := make(map[int]*DeveloperSummary)

	for tick, devTicks := range data.Ticks {
		for devID, dt := range devTicks {
			ds := getOrCreateDevSummary(devMap, devID, tick, data.Names)
			updateDevSummary(ds, dt, tick)
		}
	}

	summaries := make([]DeveloperSummary, 0, len(devMap))

	for _, ds := range devMap {
		ds.NetLines = ds.Added - ds.Removed
		summaries = append(summaries, *ds)
	}

	sort.Slice(summaries, func(i, j int) bool {
		return summaries[i].Commits > summaries[j].Commits
	})

	data.DevSummaries = summaries
}

func getOrCreateDevSummary(devMap map[int]*DeveloperSummary, devID, tick int, names []string) *DeveloperSummary {
	ds, exists := devMap[devID]
	if !exists {
		ds = &DeveloperSummary{
			ID:        devID,
			Name:      devName(devID, names),
			Languages: make(map[string]pkgplumbing.LineStats),
			FirstTick: tick,
			LastTick:  tick,
		}
		devMap[devID] = ds
	}

	return ds
}

func updateDevSummary(ds *DeveloperSummary, dt *DevTick, tick int) {
	ds.Commits += dt.Commits
	ds.Added += dt.Added
	ds.Removed += dt.Removed
	ds.Changed += dt.Changed
	ds.ActiveTicks++

	if tick < ds.FirstTick {
		ds.FirstTick = tick
	}

	if tick > ds.LastTick {
		ds.LastTick = tick
	}

	accumulateLanguageStats(ds, dt)
}

func accumulateLanguageStats(ds *DeveloperSummary, dt *DevTick) {
	for lang, stats := range dt.Languages {
		ls := ds.Languages[lang]
		ds.Languages[lang] = pkgplumbing.LineStats{
			Added:   ls.Added + stats.Added,
			Removed: ls.Removed + stats.Removed,
			Changed: ls.Changed + stats.Changed,
		}
	}
}

func computeLanguageSummaries(data *DashboardData) {
	langMap := make(map[string]*LanguageSummary)

	for _, ds := range data.DevSummaries {
		for lang, stats := range ds.Languages {
			if lang == "" {
				lang = langOther
			}

			ls, exists := langMap[lang]
			if !exists {
				ls = &LanguageSummary{
					Name:       lang,
					Developers: make(map[int]int),
				}
				langMap[lang] = ls
			}

			ls.TotalLines += stats.Added
			ls.Developers[ds.ID] += stats.Added
		}
	}

	summaries := make([]LanguageSummary, 0, len(langMap))

	for _, ls := range langMap {
		summaries = append(summaries, *ls)
	}

	sort.Slice(summaries, func(i, j int) bool {
		return summaries[i].TotalLines > summaries[j].TotalLines
	})

	data.LanguageSummaries = summaries

	topLangs := make([]string, 0, topLanguagesForRadar)

	for i, ls := range summaries {
		if i >= topLanguagesForRadar {
			break
		}

		topLangs = append(topLangs, ls.Name)
	}

	data.TopLanguages = topLangs
}

func computeBusFactorEntries(data *DashboardData) {
	entries := make([]BusFactorEntry, 0, len(data.LanguageSummaries))

	for _, ls := range data.LanguageSummaries {
		if ls.TotalLines == 0 {
			continue
		}

		type devContrib struct {
			id    int
			lines int
		}

		contribs := make([]devContrib, 0, len(ls.Developers))

		for devID, lines := range ls.Developers {
			contribs = append(contribs, devContrib{devID, lines})
		}

		sort.Slice(contribs, func(i, j int) bool {
			return contribs[i].lines > contribs[j].lines
		})

		entry := BusFactorEntry{
			Language: ls.Name,
		}

		if len(contribs) > 0 {
			entry.PrimaryDev = devName(contribs[0].id, data.Names)
			entry.PrimaryPct = float64(contribs[0].lines) / float64(ls.TotalLines) * percentMultiplier
		}

		if len(contribs) > 1 {
			entry.SecondaryDev = devName(contribs[1].id, data.Names)
			entry.SecondaryPct = float64(contribs[1].lines) / float64(ls.TotalLines) * percentMultiplier
		}

		switch {
		case entry.PrimaryPct >= riskThresholdCritical:
			entry.RiskLevel = riskCritical
		case entry.PrimaryPct >= riskThresholdHigh:
			entry.RiskLevel = riskHigh
		case entry.PrimaryPct >= riskThresholdMedium:
			entry.RiskLevel = riskMedium
		default:
			entry.RiskLevel = riskLow
		}

		entries = append(entries, entry)
	}

	sort.Slice(entries, func(i, j int) bool {
		return riskPriority(entries[i].RiskLevel) < riskPriority(entries[j].RiskLevel)
	})

	if len(entries) > riskTableMaxRows {
		entries = entries[:riskTableMaxRows]
	}

	data.BusFactorEntries = entries
}

func computeAggregates(data *DashboardData) {
	for _, ds := range data.DevSummaries {
		data.TotalCommits += ds.Commits
		data.TotalAdded += ds.Added
		data.TotalRemoved += ds.Removed
	}

	data.TotalDevelopers = len(data.DevSummaries)

	if len(data.Ticks) > 0 {
		tickKeys := sortedKeys(data.Ticks)
		maxTick := tickKeys[len(tickKeys)-1]
		recentThreshold := int(float64(maxTick) * recentActivity)

		activeDevs := make(map[int]bool)

		for tick, devTicks := range data.Ticks {
			if tick >= recentThreshold {
				for devID := range devTicks {
					activeDevs[devID] = true
				}
			}
		}

		data.ActiveDevelopers = len(activeDevs)
	}
}

func riskPriority(level string) int {
	switch level {
	case riskCritical:
		return 0
	case riskHigh:
		return riskPriorityHigh
	case riskMedium:
		return riskPriorityMedium
	default:
		return riskPriorityLow
	}
}

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
	ticks, ok := report["Ticks"].(map[int]map[int]*DevTick)
	if !ok {
		return nil
	}

	names, ok := report["ReversedPeopleDict"].([]string)
	if !ok {
		return nil
	}

	commitCounts := make(map[int]int)

	for _, devTicks := range ticks {
		for devID, dt := range devTicks {
			commitCounts[devID] += dt.Commits
		}
	}

	entries := make([]IdentityAuditEntry, 0, len(names))

	for devID, commits := range commitCounts {
		name := devName(devID, names)
		entries = append(entries, IdentityAuditEntry{
			CanonicalName: name,
			CommitCount:   commits,
		})
	}

	sort.Slice(entries, func(i, j int) bool {
		return entries[i].CommitCount > entries[j].CommitCount
	})

	return entries
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
