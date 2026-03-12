package clones

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

func newTestClonesReport() analyze.Report {
	return analyze.Report{
		keyTotalFunctions:  10,
		keyTotalClonePairs: 3,
		keyCloneRatio:      0.3,
		keyMessage:         "Clone analysis completed",
		keyClonePairs: []ClonePair{
			{FuncA: "Foo", FuncB: "Bar", Similarity: 0.6, CloneType: CloneType3},
			{FuncA: "Alpha", FuncB: "Beta", Similarity: 1.0, CloneType: CloneType1},
			{FuncA: "ProcessA", FuncB: "ProcessB", Similarity: 0.85, CloneType: CloneType2},
		},
	}
}

func TestCloneSection_Title(t *testing.T) {
	t.Parallel()

	s := NewReportSection(newTestClonesReport())
	assert.Equal(t, sectionTitle, s.SectionTitle())
}

func TestCloneSection_NilReport(t *testing.T) {
	t.Parallel()

	s := NewReportSection(nil)
	assert.Equal(t, sectionTitle, s.SectionTitle())
}

func TestCloneSection_Score(t *testing.T) {
	t.Parallel()

	// cloneRatio=0.3 → score = 1.0 - 0.3 = 0.7.
	s := NewReportSection(newTestClonesReport())
	assert.InDelta(t, 0.7, s.Score(), 1e-9)
}

func TestCloneSection_StatusMessage(t *testing.T) {
	t.Parallel()

	s := NewReportSection(newTestClonesReport())
	assert.Equal(t, "Clone analysis completed", s.StatusMessage())
}

func TestCloneSection_StatusMessage_Default(t *testing.T) {
	t.Parallel()

	s := NewReportSection(analyze.Report{})
	assert.Equal(t, defaultStatusMsg, s.StatusMessage())
}

func TestCloneSection_KeyMetrics(t *testing.T) {
	t.Parallel()

	s := NewReportSection(newTestClonesReport())
	metrics := s.KeyMetrics()

	require.Len(t, metrics, 3)
	assert.Equal(t, metricTotalFuncs, metrics[0].Label)
	assert.Equal(t, metricClonePairs, metrics[1].Label)
	assert.Equal(t, metricCloneRatio, metrics[2].Label)
}

func TestCloneSection_Distribution(t *testing.T) {
	t.Parallel()

	s := NewReportSection(newTestClonesReport())
	dist := s.Distribution()

	require.Len(t, dist, 3)

	// Type-1: 1 (Alpha/Beta, sim=1.0).
	assert.Equal(t, distLabelType1, dist[0].Label)
	assert.Equal(t, 1, dist[0].Count)

	// Type-2: 1 (ProcessA/ProcessB, sim=0.85).
	assert.Equal(t, distLabelType2, dist[1].Label)
	assert.Equal(t, 1, dist[1].Count)

	// Type-3: 1 (Foo/Bar, sim=0.6).
	assert.Equal(t, distLabelType3, dist[2].Label)
	assert.Equal(t, 1, dist[2].Count)
}

func TestCloneSection_Distribution_Empty(t *testing.T) {
	t.Parallel()

	s := NewReportSection(analyze.Report{})
	assert.Nil(t, s.Distribution())
}

func TestCloneSection_TopIssues_SortedBySimilarityDesc(t *testing.T) {
	t.Parallel()

	s := NewReportSection(newTestClonesReport())

	const topN = 2

	// Highest similarity first: Alpha/Beta (1.0), then ProcessA/ProcessB (0.85).
	issues := s.TopIssues(topN)

	require.Len(t, issues, topN)
	assert.Equal(t, "Alpha <-> Beta", issues[0].Name)
	assert.Equal(t, "ProcessA <-> ProcessB", issues[1].Name)
}

func TestCloneSection_TopIssues_Severity(t *testing.T) {
	t.Parallel()

	s := NewReportSection(newTestClonesReport())

	issues := s.TopIssues(3)

	require.Len(t, issues, 3)

	// sim=1.0 >= severityThreshHigh(0.8) → poor.
	assert.Equal(t, analyze.SeverityPoor, issues[0].Severity)

	// sim=0.85 >= severityThreshHigh(0.8) → poor.
	assert.Equal(t, analyze.SeverityPoor, issues[1].Severity)

	// sim=0.6 < severityThreshHigh(0.8) → fair.
	assert.Equal(t, analyze.SeverityFair, issues[2].Severity)
}

func TestCloneSection_AllIssues_ReturnsAll(t *testing.T) {
	t.Parallel()

	s := NewReportSection(newTestClonesReport())

	// Sorted descending by similarity.
	issues := s.AllIssues()

	require.Len(t, issues, 3)
	assert.Equal(t, "Alpha <-> Beta", issues[0].Name)
	assert.Equal(t, "ProcessA <-> ProcessB", issues[1].Name)
	assert.Equal(t, "Foo <-> Bar", issues[2].Name)
}

func TestCloneSection_TopIssues_Empty(t *testing.T) {
	t.Parallel()

	s := NewReportSection(analyze.Report{})
	assert.Empty(t, s.TopIssues(5))
}

func TestCloneSection_AllIssues_Empty(t *testing.T) {
	t.Parallel()

	s := NewReportSection(analyze.Report{})
	assert.Nil(t, s.AllIssues())
}

func TestCloneSection_TopIssues_NLargerThanTotal(t *testing.T) {
	t.Parallel()

	// n=100 > 3 pairs → returns all 3.
	s := NewReportSection(newTestClonesReport())

	issues := s.TopIssues(100)
	assert.Len(t, issues, 3)
}

func TestCloneSection_ImplementsInterface(t *testing.T) {
	t.Parallel()

	var _ analyze.ReportSection = (*ReportSection)(nil)
}
