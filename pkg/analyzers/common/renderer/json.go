package renderer

import "github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"

// JSONReport is the top-level structured JSON output.
type JSONReport struct {
	OverallScore      float64       `json:"overall_score"`
	OverallScoreLabel string        `json:"overall_score_label"`
	Sections          []JSONSection `json:"sections"`
}

// JSONSection represents one analyzer's output in JSON.
type JSONSection struct {
	Title        string             `json:"title"`
	Score        float64            `json:"score"`
	ScoreLabel   string             `json:"score_label"`
	Status       string             `json:"status"`
	Metrics      []JSONMetric       `json:"metrics"`
	Distribution []JSONDistribution `json:"distribution,omitempty"`
	Issues       []JSONIssue        `json:"issues"`
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
	metrics := make([]JSONMetric, 0)
	for _, m := range section.KeyMetrics() {
		metrics = append(metrics, JSONMetric{Label: m.Label, Value: m.Value})
	}

	var distribution []JSONDistribution
	for _, d := range section.Distribution() {
		distribution = append(distribution, JSONDistribution{
			Label:   d.Label,
			Percent: d.Percent,
			Count:   d.Count,
		})
	}

	issues := make([]JSONIssue, 0)
	for _, i := range section.AllIssues() {
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
