package clones

import (
	"fmt"
	"sort"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/reportutil"
)

// Report section display constants.
const (
	sectionTitle       = "CLONE DETECTION"
	defaultStatusMsg   = "Clone analysis completed"
	metricTotalFuncs   = "Total Functions"
	metricClonePairs   = "Clone Pairs"
	metricCloneRatio   = "Clone Ratio"
	distLabelType1     = "Type-1 (Exact)"
	distLabelType2     = "Type-2 (Renamed)"
	distLabelType3     = "Type-3 (Near-miss)"
	severityThreshHigh = 0.8
)

// ReportSection implements the analyze.ReportSection interface for clone detection.
type ReportSection struct {
	analyze.BaseReportSection

	report analyze.Report
}

// NewReportSection creates a ReportSection from clone detection report data.
func NewReportSection(report analyze.Report) *ReportSection {
	cloneRatio := reportutil.GetFloat64(report, keyCloneRatio)
	msg := reportutil.GetString(report, keyMessage)

	if msg == "" {
		msg = defaultStatusMsg
	}

	score := computeScore(cloneRatio)

	return &ReportSection{
		BaseReportSection: analyze.BaseReportSection{
			Title:      sectionTitle,
			Message:    msg,
			ScoreValue: score,
		},
		report: report,
	}
}

// computeScore converts clone ratio to a 0-1 score (lower ratio = higher score).
func computeScore(cloneRatio float64) float64 {
	score := 1.0 - cloneRatio
	if score < 0 {
		return 0.0
	}

	return score
}

// KeyMetrics returns ordered key metrics for display.
func (s *ReportSection) KeyMetrics() []analyze.Metric {
	return []analyze.Metric{
		{Label: metricTotalFuncs, Value: reportutil.FormatInt(reportutil.GetInt(s.report, keyTotalFunctions))},
		{Label: metricClonePairs, Value: reportutil.FormatInt(reportutil.GetInt(s.report, keyTotalClonePairs))},
		{Label: metricCloneRatio, Value: reportutil.FormatFloat(reportutil.GetFloat64(s.report, keyCloneRatio))},
	}
}

// Distribution returns clone type distribution data.
func (s *ReportSection) Distribution() []analyze.DistributionItem {
	pairs := extractClonePairs(s.report)
	if len(pairs) == 0 {
		return nil
	}

	counts := categorizeClonePairs(pairs)
	total := len(pairs)

	return []analyze.DistributionItem{
		{Label: distLabelType1, Percent: reportutil.Pct(counts.type1, total), Count: counts.type1},
		{Label: distLabelType2, Percent: reportutil.Pct(counts.type2, total), Count: counts.type2},
		{Label: distLabelType3, Percent: reportutil.Pct(counts.type3, total), Count: counts.type3},
	}
}

// cloneTypeCounts holds counts per clone type.
type cloneTypeCounts struct {
	type1 int
	type2 int
	type3 int
}

// categorizeClonePairs counts clone pairs by type.
func categorizeClonePairs(pairs []ClonePair) cloneTypeCounts {
	counts := cloneTypeCounts{}

	for _, p := range pairs {
		switch p.CloneType {
		case CloneType1:
			counts.type1++
		case CloneType2:
			counts.type2++
		case CloneType3:
			counts.type3++
		}
	}

	return counts
}

// TopIssues returns the top N clone pairs as issues.
func (s *ReportSection) TopIssues(n int) []analyze.Issue {
	issues := s.buildSortedIssues()
	if n >= len(issues) {
		return issues
	}

	return issues[:n]
}

// AllIssues returns all clone pairs as issues.
func (s *ReportSection) AllIssues() []analyze.Issue {
	return s.buildSortedIssues()
}

// buildSortedIssues builds issues from clone pairs sorted by severity.
func (s *ReportSection) buildSortedIssues() []analyze.Issue {
	pairs := extractClonePairs(s.report)
	if len(pairs) == 0 {
		return nil
	}

	sort.Slice(pairs, func(i, j int) bool {
		return pairs[i].Similarity > pairs[j].Similarity
	})

	issues := make([]analyze.Issue, 0, len(pairs))

	for _, p := range pairs {
		severity := analyze.SeverityFair
		if p.Similarity >= severityThreshHigh {
			severity = analyze.SeverityPoor
		}

		issues = append(issues, analyze.Issue{
			Name:     fmt.Sprintf("%s <-> %s", p.FuncA, p.FuncB),
			Location: p.CloneType,
			Value:    reportutil.FormatFloat(p.Similarity),
			Severity: severity,
		})
	}

	return issues
}
