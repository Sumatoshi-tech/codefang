package clones

import (
	"fmt"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/reportutil"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/mapx"
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
// Clone ratio is pairs/functions which can exceed 1.0 (quadratic pair growth),
// so we clamp to [0, 1] before inverting.
func computeScore(cloneRatio float64) float64 {
	if cloneRatio >= 1.0 {
		return 0.0
	}

	if cloneRatio <= 0.0 {
		return 1.0
	}

	return 1.0 - cloneRatio
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
// Uses the full-population distribution when available, falling back to the capped pairs array.
func (s *ReportSection) Distribution() []analyze.DistributionItem {
	counts, total := s.extractDistribution()
	if total == 0 {
		return nil
	}

	return []analyze.DistributionItem{
		{Label: distLabelType1, Percent: reportutil.Pct(counts.type1, total), Count: counts.type1},
		{Label: distLabelType2, Percent: reportutil.Pct(counts.type2, total), Count: counts.type2},
		{Label: distLabelType3, Percent: reportutil.Pct(counts.type3, total), Count: counts.type3},
	}
}

func (s *ReportSection) extractDistribution() (counts cloneTypeCounts, total int) {
	if dist, ok := s.report[keyCloneTypeDistribution].(map[string]int); ok {
		counts = cloneTypeCounts{
			type1: dist[CloneType1],
			type2: dist[CloneType2],
			type3: dist[CloneType3],
		}

		return counts, counts.type1 + counts.type2 + counts.type3
	}

	pairs := extractClonePairs(s.report)

	return categorizeClonePairs(pairs), len(pairs)
}

// cloneTypeCounts holds counts per clone type.
type cloneTypeCounts struct {
	type1 int
	type2 int
	type3 int
}

// increment adds one to the counter for the given clone type.
func (c *cloneTypeCounts) increment(cloneType string) {
	switch cloneType {
	case CloneType1:
		c.type1++
	case CloneType2:
		c.type2++
	case CloneType3:
		c.type3++
	}
}

// cloneTypeDistMap converts counts to a string-keyed map for JSON serialization.
func cloneTypeDistMap(c cloneTypeCounts) map[string]int {
	return map[string]int{
		CloneType1: c.type1,
		CloneType2: c.type2,
		CloneType3: c.type3,
	}
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

// clonePairLess orders clone pairs by Similarity descending (most similar = first).
func clonePairLess(a, b ClonePair) bool { return a.Similarity > b.Similarity }

// TopIssues returns the top N clone pairs as issues.
func (s *ReportSection) TopIssues(n int) []analyze.Issue {
	return s.cloneIssues(n)
}

// AllIssues returns all clone pairs as issues sorted by similarity descending.
func (s *ReportSection) AllIssues() []analyze.Issue {
	return s.cloneIssues(0)
}

// cloneIssues builds issues from clone pairs sorted by similarity descending, limited to limit (0 = all).
func (s *ReportSection) cloneIssues(limit int) []analyze.Issue {
	pairs := extractClonePairs(s.report)
	if len(pairs) == 0 {
		return nil
	}

	sorted := mapx.SortAndLimit(pairs, clonePairLess, limit)

	issues := make([]analyze.Issue, 0, len(sorted))

	for _, p := range sorted {
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
