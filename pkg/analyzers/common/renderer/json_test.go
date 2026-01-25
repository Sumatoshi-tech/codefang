package renderer

import (
	"encoding/json"
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// jsonMockSection implements analyze.ReportSection for JSON tests.
type jsonMockSection struct {
	analyze.BaseReportSection
	metrics      []analyze.Metric
	distribution []analyze.DistributionItem
	issues       []analyze.Issue
}

func (m *jsonMockSection) KeyMetrics() []analyze.Metric             { return m.metrics }
func (m *jsonMockSection) Distribution() []analyze.DistributionItem { return m.distribution }
func (m *jsonMockSection) TopIssues(n int) []analyze.Issue {
	if n >= len(m.issues) {
		return m.issues
	}
	return m.issues[:n]
}
func (m *jsonMockSection) AllIssues() []analyze.Issue { return m.issues }

func newJSONMock(title string, score float64, msg string) *jsonMockSection {
	return &jsonMockSection{
		BaseReportSection: analyze.BaseReportSection{
			Title:      title,
			ScoreValue: score,
			Message:    msg,
		},
	}
}

func TestSectionToJSON_Fields(t *testing.T) {
	mock := newJSONMock("COMPLEXITY", 0.8, "Good - reasonable complexity")
	mock.metrics = []analyze.Metric{
		{Label: "Total Functions", Value: "42"},
		{Label: "Avg Complexity", Value: "3.2"},
	}
	mock.distribution = []analyze.DistributionItem{
		{Label: "Simple (1-5)", Percent: 0.7, Count: 30},
		{Label: "Complex (6-10)", Percent: 0.3, Count: 12},
	}
	mock.issues = []analyze.Issue{
		{Name: "processData", Location: "main.go:10", Value: "15", Severity: analyze.SeverityPoor},
	}

	result := SectionToJSON(mock)

	assert.Equal(t, "COMPLEXITY", result.Title)
	assert.InDelta(t, 0.8, result.Score, 0.001)
	assert.Equal(t, "8/10", result.ScoreLabel)
	assert.Equal(t, "Good - reasonable complexity", result.Status)
	require.Len(t, result.Metrics, 2)
	assert.Equal(t, "Total Functions", result.Metrics[0].Label)
	assert.Equal(t, "42", result.Metrics[0].Value)
	require.Len(t, result.Distribution, 2)
	assert.Equal(t, "Simple (1-5)", result.Distribution[0].Label)
	assert.InDelta(t, 0.7, result.Distribution[0].Percent, 0.001)
	assert.Equal(t, 30, result.Distribution[0].Count)
	require.Len(t, result.Issues, 1)
	assert.Equal(t, "processData", result.Issues[0].Name)
	assert.Equal(t, "main.go:10", result.Issues[0].Location)
	assert.Equal(t, "15", result.Issues[0].Value)
	assert.Equal(t, analyze.SeverityPoor, result.Issues[0].Severity)
}

func TestSectionToJSON_InfoOnly(t *testing.T) {
	mock := newJSONMock("IMPORTS", analyze.ScoreInfoOnly, "5 unique imports found")

	result := SectionToJSON(mock)

	assert.Equal(t, "IMPORTS", result.Title)
	assert.InDelta(t, -1.0, result.Score, 0.001)
	assert.Equal(t, "Info", result.ScoreLabel)
	assert.Equal(t, "5 unique imports found", result.Status)
}

func TestSectionToJSON_EmptyIssues(t *testing.T) {
	mock := newJSONMock("COMMENTS", 0.6, "Fair comment quality")

	result := SectionToJSON(mock)

	assert.NotNil(t, result.Issues, "Issues should be empty array, not nil")
	assert.Empty(t, result.Issues)
}

func TestSectionToJSON_EmptyMetrics(t *testing.T) {
	mock := newJSONMock("TEST", 0.5, "Test section")

	result := SectionToJSON(mock)

	assert.NotNil(t, result.Metrics, "Metrics should be empty array, not nil")
	assert.Empty(t, result.Metrics)
}

func TestSectionsToJSON_MultipleSections(t *testing.T) {
	sections := []analyze.ReportSection{
		newJSONMock("COMPLEXITY", 0.8, "Good"),
		newJSONMock("COMMENTS", 0.6, "Fair"),
	}

	result := SectionsToJSON(sections)

	require.Len(t, result.Sections, 2)
	assert.Equal(t, "COMPLEXITY", result.Sections[0].Title)
	assert.Equal(t, "COMMENTS", result.Sections[1].Title)
}

func TestSectionsToJSON_IncludesOverall(t *testing.T) {
	sections := []analyze.ReportSection{
		newJSONMock("COMPLEXITY", 0.8, "Good"),
		newJSONMock("COMMENTS", 0.6, "Fair"),
	}

	result := SectionsToJSON(sections)

	assert.InDelta(t, 0.7, result.OverallScore, 0.001)
	assert.Equal(t, "7/10", result.OverallScoreLabel)
}

func TestSectionsToJSON_OverallExcludesInfoOnly(t *testing.T) {
	sections := []analyze.ReportSection{
		newJSONMock("COMPLEXITY", 0.8, "Good"),
		newJSONMock("IMPORTS", analyze.ScoreInfoOnly, "Info"),
	}

	result := SectionsToJSON(sections)

	assert.InDelta(t, 0.8, result.OverallScore, 0.001)
}

func TestSectionsToJSON_SingleSection(t *testing.T) {
	sections := []analyze.ReportSection{
		newJSONMock("COMPLEXITY", 0.8, "Good"),
	}

	result := SectionsToJSON(sections)

	require.Len(t, result.Sections, 1)
	assert.InDelta(t, 0.8, result.OverallScore, 0.001)
}

func TestSectionsToJSON_AllInfoOnly(t *testing.T) {
	sections := []analyze.ReportSection{
		newJSONMock("IMPORTS", analyze.ScoreInfoOnly, "Info"),
	}

	result := SectionsToJSON(sections)

	assert.InDelta(t, analyze.ScoreInfoOnly, result.OverallScore, 0.001)
	assert.Equal(t, "Info", result.OverallScoreLabel)
}

func TestSectionsToJSON_Serializable(t *testing.T) {
	sections := []analyze.ReportSection{
		newJSONMock("COMPLEXITY", 0.8, "Good"),
	}

	result := SectionsToJSON(sections)

	data, err := json.Marshal(result)
	require.NoError(t, err)
	assert.Contains(t, string(data), `"title":"COMPLEXITY"`)
	assert.Contains(t, string(data), `"overall_score":0.8`)
}
