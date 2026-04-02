package renderer

import "github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"

// JSONReport is the top-level structured JSON output.
type JSONReport struct {
	OverallScoreLabel string        `json:"overall_score_label"`
	Sections          []JSONSection `json:"sections"`
	OverallScore      float64       `json:"overall_score"`
}

// JSONSection represents one analyzer's output in JSON.
type JSONSection struct {
	Title        string             `json:"title"`
	ScoreLabel   string             `json:"score_label"`
	Status       string             `json:"status"`
	Metrics      []JSONMetric       `json:"metrics"`
	Distribution []JSONDistribution `json:"distribution,omitempty"`
	Issues       []JSONIssue        `json:"issues"`
	Files        *[]JSONFileEntry   `json:"files,omitempty"`
	Score        float64            `json:"score"`
}

// JSONFileEntry represents one file's analysis results within a section.
// FRD: specs/frds/FRD-20260327-json-perfile-types.md.
type JSONFileEntry struct {
	FilePath     string             `json:"file_path"`
	ScoreLabel   string             `json:"score_label"`
	Status       string             `json:"status"`
	Metrics      []JSONMetric       `json:"metrics"`
	Distribution []JSONDistribution `json:"distribution,omitempty"`
	Issues       []JSONIssue        `json:"issues"`
	Score        float64            `json:"score"`
}

// JSONMetric is a key-value metric in JSON output.
type JSONMetric struct {
	Label string `json:"label"`
	Value string `json:"value"`
}

// JSONDistribution is a distribution category in JSON output.
type JSONDistribution struct {
	Label   string  `json:"label"`
	Percent float64 `json:"percent"`
	Count   int     `json:"count"`
}

// JSONIssue is a single issue in JSON output.
type JSONIssue struct {
	Name     string `json:"name"`
	Location string `json:"location"`
	Value    string `json:"value"`
	Severity string `json:"severity"`
}

// SectionToJSON converts a ReportSection to a JSONSection.
func SectionToJSON(section analyze.ReportSection) JSONSection {
	keyMetrics := section.KeyMetrics()

	metrics := make([]JSONMetric, 0, len(keyMetrics))
	for _, m := range keyMetrics {
		metrics = append(metrics, JSONMetric{Label: m.Label, Value: m.Value})
	}

	dist := section.Distribution()

	distribution := make([]JSONDistribution, 0, len(dist))
	for _, d := range dist {
		distribution = append(distribution, JSONDistribution{
			Label:   d.Label,
			Percent: d.Percent,
			Count:   d.Count,
		})
	}

	allIssues := section.AllIssues()

	issues := make([]JSONIssue, 0, len(allIssues))
	for _, i := range allIssues {
		issues = append(issues, JSONIssue{
			Name:     i.Name,
			Location: i.Location,
			Value:    i.Value,
			Severity: i.Severity,
		})
	}

	return JSONSection{
		Title:        section.SectionTitle(),
		Score:        section.Score(),
		ScoreLabel:   section.ScoreLabel(),
		Status:       section.StatusMessage(),
		Metrics:      metrics,
		Distribution: distribution,
		Issues:       issues,
	}
}

// EnrichWithPerFileData injects per-file data and summary statistics into JSON sections.
// Implements analyze.PerFileEnricher to avoid import cycles.
func (r *JSONReport) EnrichWithPerFileData(
	perFileResults map[string]map[string]analyze.Report,
	rootPath string,
	analyzers []analyze.StaticAnalyzer,
) {
	// Build analyzer name → (section title, provider) mapping.
	type analyzerInfo struct {
		title    string
		provider analyze.ReportSectionProvider
	}

	infoByName := make(map[string]analyzerInfo, len(analyzers))

	for _, analyzer := range analyzers {
		provider, ok := analyzer.(analyze.ReportSectionProvider)
		if !ok {
			continue
		}

		emptySection := provider.CreateReportSection(analyze.Report{})
		infoByName[analyzer.Name()] = analyzerInfo{
			title:    emptySection.SectionTitle(),
			provider: provider,
		}
	}

	// Build section title → index for O(1) lookup.
	titleToIdx := make(map[string]int, len(r.Sections))
	for idx, section := range r.Sections {
		titleToIdx[section.Title] = idx
	}

	// Initialize all sections with empty files array (spec: empty array, not omitted).
	for idx := range r.Sections {
		emptyFiles := make([]JSONFileEntry, 0)
		r.Sections[idx].Files = &emptyFiles
	}

	for analyzerName, fileReports := range perFileResults {
		info, ok := infoByName[analyzerName]
		if !ok {
			continue
		}

		idx, found := titleToIdx[info.title]
		if !found {
			continue
		}

		files := make([]JSONFileEntry, 0, len(fileReports))
		for filePath, report := range fileReports {
			section := info.provider.CreateReportSection(report)
			relPath := analyze.MakeRelativePath(filePath, rootPath)
			files = append(files, SectionToJSONFileEntry(section, relPath))
		}

		r.Sections[idx].Files = &files
	}
}

// SectionToJSONFileEntry converts a ReportSection to a JSONFileEntry for per-file output.
func SectionToJSONFileEntry(section analyze.ReportSection, filePath string) JSONFileEntry {
	base := SectionToJSON(section)

	return JSONFileEntry{
		FilePath:     filePath,
		Score:        base.Score,
		ScoreLabel:   base.ScoreLabel,
		Status:       base.Status,
		Metrics:      base.Metrics,
		Distribution: base.Distribution,
		Issues:       base.Issues,
	}
}

// SectionsToJSON converts multiple ReportSections to a JSONReport with overall score.
func SectionsToJSON(sections []analyze.ReportSection) JSONReport {
	summary := NewExecutiveSummary(sections)

	jsonSections := make([]JSONSection, 0, len(sections))
	for _, s := range sections {
		jsonSections = append(jsonSections, SectionToJSON(s))
	}

	return JSONReport{
		OverallScore:      summary.OverallScore(),
		OverallScoreLabel: summary.OverallScoreLabel(),
		Sections:          jsonSections,
	}
}
