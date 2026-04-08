// FRD: specs/frds/FRD-20260404-static-composition-analyzer.md.

package composition

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	filehistory "github.com/Sumatoshi-tech/codefang/internal/analyzers/file_history"
)

func newTestCompositionReport() analyze.Report {
	return analyze.Report{
		keyTotalFiles: 10,
		keyBreakdown: map[string]int{
			string(filehistory.CategorySource):        6,
			string(filehistory.CategoryVendor):        2,
			string(filehistory.CategoryDocumentation): 1,
			string(filehistory.CategoryBinary):        1,
		},
		keyPercentage: map[string]float64{
			string(filehistory.CategorySource):        60.0,
			string(filehistory.CategoryVendor):        20.0,
			string(filehistory.CategoryDocumentation): 10.0,
			string(filehistory.CategoryBinary):        10.0,
		},
	}
}

func TestCompositionSection_Title(t *testing.T) {
	t.Parallel()

	s := NewReportSection(newTestCompositionReport())
	assert.Equal(t, sectionTitle, s.SectionTitle())
}

func TestCompositionSection_Score_InfoOnly(t *testing.T) {
	t.Parallel()

	s := NewReportSection(newTestCompositionReport())
	assert.InDelta(t, analyze.ScoreInfoOnly, s.Score(), 0.001)
}

func TestCompositionSection_StatusMessage(t *testing.T) {
	t.Parallel()

	s := NewReportSection(newTestCompositionReport())
	assert.Equal(t, statusDefault, s.StatusMessage())
}

func TestCompositionSection_StatusMessage_Empty(t *testing.T) {
	t.Parallel()

	s := NewReportSection(analyze.Report{})
	assert.Equal(t, statusEmpty, s.StatusMessage())
}

func TestCompositionSection_NilReport(t *testing.T) {
	t.Parallel()

	s := NewReportSection(nil)
	assert.Equal(t, sectionTitle, s.SectionTitle())
	assert.Equal(t, statusEmpty, s.StatusMessage())
}

func TestCompositionSection_KeyMetrics_Count(t *testing.T) {
	t.Parallel()

	s := NewReportSection(newTestCompositionReport())
	metrics := s.KeyMetrics()

	const expectedMetrics = 3
	require.Len(t, metrics, expectedMetrics)
}

func TestCompositionSection_KeyMetrics_Labels(t *testing.T) {
	t.Parallel()

	s := NewReportSection(newTestCompositionReport())
	metrics := s.KeyMetrics()

	assert.Equal(t, metricTotalFiles, metrics[0].Label)
	assert.Equal(t, metricSource, metrics[1].Label)
	assert.Equal(t, metricSourcePct, metrics[2].Label)
}

func TestCompositionSection_KeyMetrics_Values(t *testing.T) {
	t.Parallel()

	s := NewReportSection(newTestCompositionReport())
	metrics := s.KeyMetrics()

	assert.Equal(t, "10", metrics[0].Value)
	assert.Equal(t, "6", metrics[1].Value)
	assert.Contains(t, metrics[2].Value, "60")
}

func TestCompositionSection_Distribution(t *testing.T) {
	t.Parallel()

	s := NewReportSection(newTestCompositionReport())
	dist := s.Distribution()

	require.NotNil(t, dist)
	// 4 categories with non-zero counts.
	require.Len(t, dist, 4)

	// First should be source (order follows AllCategories).
	assert.Equal(t, string(filehistory.CategorySource), dist[0].Label)
	assert.Equal(t, 6, dist[0].Count)
}

func TestCompositionSection_Distribution_Empty(t *testing.T) {
	t.Parallel()

	s := NewReportSection(analyze.Report{})
	assert.Nil(t, s.Distribution())
}

func TestCompositionSection_TopIssues(t *testing.T) {
	t.Parallel()

	s := NewReportSection(newTestCompositionReport())

	issues := s.TopIssues(2)
	require.Len(t, issues, 2)
}

func TestCompositionSection_AllIssues(t *testing.T) {
	t.Parallel()

	s := NewReportSection(newTestCompositionReport())

	issues := s.AllIssues()
	// 3 non-source categories with counts: vendor, docs, binary.
	require.Len(t, issues, 3)
}

func TestCompositionSection_Issues_BinarySeverityPoor(t *testing.T) {
	t.Parallel()

	s := NewReportSection(newTestCompositionReport())

	issues := s.AllIssues()

	var binaryIssue *analyze.Issue

	for idx := range issues {
		if issues[idx].Name == string(filehistory.CategoryBinary) {
			binaryIssue = &issues[idx]

			break
		}
	}

	require.NotNil(t, binaryIssue, "binary category must appear in issues")
	assert.Equal(t, analyze.SeverityPoor, binaryIssue.Severity)
}

func TestCompositionSection_Issues_VendorSeverityInfo(t *testing.T) {
	t.Parallel()

	s := NewReportSection(newTestCompositionReport())

	issues := s.AllIssues()

	var vendorIssue *analyze.Issue

	for idx := range issues {
		if issues[idx].Name == string(filehistory.CategoryVendor) {
			vendorIssue = &issues[idx]

			break
		}
	}

	require.NotNil(t, vendorIssue, "vendor category must appear in issues")
	assert.Equal(t, analyze.SeverityInfo, vendorIssue.Severity)
}

func TestCompositionSection_Issues_Empty(t *testing.T) {
	t.Parallel()

	s := NewReportSection(analyze.Report{})
	assert.Nil(t, s.AllIssues())
}

func TestCompositionSection_ImplementsInterface(t *testing.T) {
	t.Parallel()

	var _ analyze.ReportSection = (*ReportSection)(nil)
}
