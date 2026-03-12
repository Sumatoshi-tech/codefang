package couples

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

func TestNewReportSection_NilReport(t *testing.T) {
	t.Parallel()

	rs := NewReportSection(nil)
	require.NotNil(t, rs)
	assert.Equal(t, ReportSectionTitle, rs.Title)
	assert.InDelta(t, analyze.ScoreInfoOnly, rs.Score(), 0.001)
	assert.Equal(t, DefaultStatusMsg, rs.Message)
}

func TestNewReportSection_EmptyReport(t *testing.T) {
	t.Parallel()

	rs := NewReportSection(analyze.Report{})
	require.NotNil(t, rs)
	assert.InDelta(t, analyze.ScoreInfoOnly, rs.Score(), 0.001)
	assert.Equal(t, DefaultStatusMsg, rs.Message)
}

func TestNewReportSection_WithData(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"Files":      []string{"a.go", "b.go", "c.go"},
		"FilesLines": []int{100, 200, 50},
		"FilesMatrix": []map[int]int64{
			{0: 10, 1: 8, 2: 3},
			{0: 8, 1: 12, 2: 2},
			{0: 3, 1: 2, 2: 5},
		},
		"ReversedPeopleDict": []string{"alice", "bob"},
		"PeopleMatrix": []map[int]int64{
			{0: 20, 1: 10},
			{0: 10, 1: 15},
		},
		"PeopleFiles": [][]int{
			{0, 1, 2},
			{0, 1},
		},
	}

	rs := NewReportSection(report)
	require.NotNil(t, rs)

	// Score should be between 0 and 1 (not info-only).
	score := rs.Score()
	assert.Greater(t, score, 0.0)
	assert.LessOrEqual(t, score, 1.0)

	// Title should match.
	assert.Equal(t, ReportSectionTitle, rs.Title)

	// Message should contain files count.
	assert.NotEmpty(t, rs.Message)
}

func TestReportSection_KeyMetrics(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"Files":      []string{"a.go", "b.go"},
		"FilesLines": []int{10, 20},
		"FilesMatrix": []map[int]int64{
			{0: 5, 1: 3},
			{0: 3, 1: 5},
		},
		"ReversedPeopleDict": []string{"dev1", "dev2"},
		"PeopleMatrix": []map[int]int64{
			{0: 10, 1: 5},
			{0: 5, 1: 8},
		},
		"PeopleFiles": [][]int{
			{0, 1},
			{0},
		},
	}

	rs := NewReportSection(report)
	km := rs.KeyMetrics()
	require.NotEmpty(t, km)

	labels := make(map[string]string)
	for _, m := range km {
		labels[m.Label] = m.Value
	}

	assert.Equal(t, "2", labels[MetricTotalFiles])
	assert.Equal(t, "2", labels[MetricTotalDevelopers])
	assert.Contains(t, labels, MetricTotalCoChanges)
	assert.Contains(t, labels, MetricHighlyCoupled)
	assert.Contains(t, labels, MetricAvgCoupling)
}

func TestReportSection_Distribution(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"Files":      []string{"a.go", "b.go", "c.go"},
		"FilesLines": []int{10, 20, 30},
		"FilesMatrix": []map[int]int64{
			{0: 10, 1: 8, 2: 1},
			{0: 8, 1: 10, 2: 2},
			{0: 1, 1: 2, 2: 10},
		},
		"ReversedPeopleDict": []string{"dev"},
		"PeopleMatrix":       []map[int]int64{{0: 5}},
		"PeopleFiles":        [][]int{{0, 1, 2}},
	}

	rs := NewReportSection(report)
	dist := rs.Distribution()
	require.NotEmpty(t, dist)

	// Verify all distribution labels present.
	labelSet := map[string]bool{}
	for _, d := range dist {
		labelSet[d.Label] = true
	}

	assert.True(t, labelSet[DistLabelStrong])
	assert.True(t, labelSet[DistLabelMod])
	assert.True(t, labelSet[DistLabelWeak])
	assert.True(t, labelSet[DistLabelNone])
}

func TestReportSection_Distribution_Empty(t *testing.T) {
	t.Parallel()

	rs := NewReportSection(analyze.Report{})
	dist := rs.Distribution()
	assert.Nil(t, dist)
}

func TestReportSection_TopIssues(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"Files":      []string{"a.go", "b.go", "c.go"},
		"FilesLines": []int{10, 20, 30},
		"FilesMatrix": []map[int]int64{
			{0: 10, 1: 8, 2: 3},
			{0: 8, 1: 12, 2: 2},
			{0: 3, 1: 2, 2: 5},
		},
		"ReversedPeopleDict": []string{"dev"},
		"PeopleMatrix":       []map[int]int64{{0: 5}},
		"PeopleFiles":        [][]int{{0, 1, 2}},
	}

	rs := NewReportSection(report)

	// Request more than available — should return all.
	all := rs.TopIssues(100)
	require.NotEmpty(t, all)

	// Request top 1.
	top1 := rs.TopIssues(1)
	require.Len(t, top1, 1)

	// Issue should contain file names and severity.
	assert.Contains(t, top1[0].Name, "\u2194") // ↔
	assert.NotEmpty(t, top1[0].Value)
	assert.NotEmpty(t, top1[0].Severity)
}

func TestReportSection_AllIssues(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"Files":      []string{"a.go", "b.go"},
		"FilesLines": []int{10, 20},
		"FilesMatrix": []map[int]int64{
			{0: 5, 1: 3},
			{0: 3, 1: 5},
		},
		"ReversedPeopleDict": []string{"dev"},
		"PeopleMatrix":       []map[int]int64{{0: 5}},
		"PeopleFiles":        [][]int{{0, 1}},
	}

	rs := NewReportSection(report)
	all := rs.AllIssues()
	require.Len(t, all, 1)

	assert.Contains(t, all[0].Name, "a.go")
	assert.Contains(t, all[0].Name, "b.go")
}

func TestReportSection_AllIssues_Empty(t *testing.T) {
	t.Parallel()

	rs := NewReportSection(analyze.Report{})
	assert.Nil(t, rs.AllIssues())
}

func TestComputeScore_GoodCoupling(t *testing.T) {
	t.Parallel()

	// Low avg coupling → high score.
	m := &ComputedMetrics{
		Aggregate: AggregateData{
			TotalFiles:          10,
			AvgCouplingStrength: 0.1,
		},
	}

	score, msg := computeScore(m)
	assert.InDelta(t, 0.9, score, 0.01)
	assert.Contains(t, msg, "Good")
}

func TestComputeScore_FairCoupling(t *testing.T) {
	t.Parallel()

	m := &ComputedMetrics{
		Aggregate: AggregateData{
			TotalFiles:          10,
			AvgCouplingStrength: 0.5,
			HighlyCoupledPairs:  3,
		},
	}

	score, msg := computeScore(m)
	assert.InDelta(t, 0.5, score, 0.01)
	assert.Contains(t, msg, "Fair")
}

func TestComputeScore_PoorCoupling(t *testing.T) {
	t.Parallel()

	m := &ComputedMetrics{
		Aggregate: AggregateData{
			TotalFiles:          10,
			AvgCouplingStrength: 0.8,
			HighlyCoupledPairs:  5,
		},
	}

	score, msg := computeScore(m)
	assert.InDelta(t, 0.2, score, 0.01)
	assert.Contains(t, msg, "Poor")
}

func TestComputeScore_NoFiles(t *testing.T) {
	t.Parallel()

	m := &ComputedMetrics{}
	score, msg := computeScore(m)
	assert.InDelta(t, analyze.ScoreInfoOnly, score, 0.001)
	assert.Equal(t, DefaultStatusMsg, msg)
}

func TestSeverityForStrength(t *testing.T) {
	t.Parallel()

	assert.Equal(t, analyze.SeverityPoor, severityForStrength(0.9))
	assert.Equal(t, analyze.SeverityPoor, severityForStrength(0.7))
	assert.Equal(t, analyze.SeverityFair, severityForStrength(0.5))
	assert.Equal(t, analyze.SeverityFair, severityForStrength(0.4))
	assert.Equal(t, analyze.SeverityGood, severityForStrength(0.3))
	assert.Equal(t, analyze.SeverityGood, severityForStrength(0.0))
}

func TestCategorizeStrength(t *testing.T) {
	t.Parallel()

	couples := []FileCouplingData{
		{Strength: 0.9},  // Strong.
		{Strength: 0.75}, // Strong.
		{Strength: 0.5},  // Moderate.
		{Strength: 0.2},  // Weak.
		{Strength: 0.05}, // Minimal.
	}

	counts := categorizeStrength(couples)
	assert.Equal(t, 2, counts.strong)
	assert.Equal(t, 1, counts.moderate)
	assert.Equal(t, 1, counts.weak)
	assert.Equal(t, 1, counts.minimal)
}

func TestPct(t *testing.T) {
	t.Parallel()

	assert.InDelta(t, 0.5, pct(5, 10), 0.001)
	assert.InDelta(t, 1.0, pct(3, 3), 0.001)
	assert.InDelta(t, 0.0, pct(0, 10), 0.001)
	assert.InDelta(t, 0.0, pct(5, 0), 0.001) // Division by zero → 0.
}

func TestCreateReportSection_ViaHistoryAnalyzer(t *testing.T) {
	t.Parallel()

	c := NewHistoryAnalyzer()
	report := analyze.Report{
		"Files":              []string{"f.go"},
		"FilesLines":         []int{10},
		"FilesMatrix":        []map[int]int64{{0: 5}},
		"ReversedPeopleDict": []string{"dev"},
		"PeopleMatrix":       []map[int]int64{{0: 5}},
		"PeopleFiles":        [][]int{{0}},
	}

	rs := c.CreateReportSection(report)
	require.NotNil(t, rs)
	assert.Equal(t, ReportSectionTitle, rs.SectionTitle())
}
