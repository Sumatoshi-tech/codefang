package filehistory

import (
	"sort"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/metrics"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

// --- Input Data Types ---.

// ReportData is the parsed input data for file history metrics computation.
type ReportData struct {
	Files map[string]FileHistory
}

// ParseReportData extracts ReportData from an analyzer report.
func ParseReportData(report analyze.Report) (*ReportData, error) {
	files, ok := report["Files"].(map[string]FileHistory)
	if !ok {
		files = make(map[string]FileHistory)
	}

	return &ReportData{Files: files}, nil
}

// --- Output Data Types ---.

// FileChurnData contains churn statistics for a single file.
type FileChurnData struct {
	Path             string  `json:"path"                yaml:"path"`
	CommitCount      int     `json:"commit_count"        yaml:"commit_count"`
	ContributorCount int     `json:"contributor_count"   yaml:"contributor_count"`
	TotalAdded       int     `json:"total_lines_added"   yaml:"total_lines_added"`
	TotalRemoved     int     `json:"total_lines_removed" yaml:"total_lines_removed"`
	TotalChanged     int     `json:"total_lines_changed" yaml:"total_lines_changed"`
	ChurnScore       float64 `json:"churn_score"         yaml:"churn_score"`
}

// FileContributorData contains contributor statistics for a file.
type FileContributorData struct {
	Path                string                        `json:"path"                  yaml:"path"`
	Contributors        map[int]pkgplumbing.LineStats `json:"contributors"          yaml:"contributors"`
	TopContributorID    int                           `json:"top_contributor_id"    yaml:"top_contributor_id"`
	TopContributorLines int                           `json:"top_contributor_lines" yaml:"top_contributor_lines"`
}

// HotspotData identifies high-churn files that may need attention.
type HotspotData struct {
	Path        string  `json:"path"         yaml:"path"`
	CommitCount int     `json:"commit_count" yaml:"commit_count"`
	ChurnScore  float64 `json:"churn_score"  yaml:"churn_score"`
	RiskLevel   string  `json:"risk_level"   yaml:"risk_level"`
}

// AggregateData contains summary statistics.
type AggregateData struct {
	TotalFiles             int     `json:"total_files"               yaml:"total_files"`
	TotalCommits           int     `json:"total_commits"             yaml:"total_commits"`
	TotalContributors      int     `json:"total_contributors"        yaml:"total_contributors"`
	AvgCommitsPerFile      float64 `json:"avg_commits_per_file"      yaml:"avg_commits_per_file"`
	AvgContributorsPerFile float64 `json:"avg_contributors_per_file" yaml:"avg_contributors_per_file"`
	HighChurnFiles         int     `json:"high_churn_files"          yaml:"high_churn_files"`
}

// --- Metric Implementations ---.

// FileChurnMetric computes per-file churn statistics.
type FileChurnMetric struct {
	metrics.MetricMeta
}

// NewFileChurnMetric creates the file churn metric.
func NewFileChurnMetric() *FileChurnMetric {
	return &FileChurnMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "file_churn",
			MetricDisplayName: "File Churn Statistics",
			MetricDescription: "Per-file change frequency and line modification statistics. " +
				"High churn files may indicate instability or active development areas.",
			MetricType: "list",
		},
	}
}

// Compute calculates file churn statistics.
func (m *FileChurnMetric) Compute(input *ReportData) []FileChurnData {
	result := make([]FileChurnData, 0, len(input.Files))

	for path, fh := range input.Files {
		var totalAdded, totalRemoved, totalChanged int
		for _, stats := range fh.People {
			totalAdded += stats.Added
			totalRemoved += stats.Removed
			totalChanged += stats.Changed
		}

		commitCount := len(fh.Hashes)
		contributorCount := len(fh.People)

		// Churn score: weighted combination of commits and line changes.
		churnScore := float64(commitCount) + float64(totalAdded+totalRemoved+totalChanged)/churnScoreDivisor

		result = append(result, FileChurnData{
			Path:             path,
			CommitCount:      commitCount,
			ContributorCount: contributorCount,
			TotalAdded:       totalAdded,
			TotalRemoved:     totalRemoved,
			TotalChanged:     totalChanged,
			ChurnScore:       churnScore,
		})
	}

	// Sort by churn score descending.
	sort.Slice(result, func(i, j int) bool {
		return result[i].ChurnScore > result[j].ChurnScore
	})

	return result
}

// FileContributorMetric computes per-file contributor breakdown.
type FileContributorMetric struct {
	metrics.MetricMeta
}

// NewFileContributorMetric creates the file contributor metric.
func NewFileContributorMetric() *FileContributorMetric {
	return &FileContributorMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "file_contributors",
			MetricDisplayName: "File Contributor Breakdown",
			MetricDescription: "Per-file breakdown of which developers contributed and their line statistics. " +
				"Useful for identifying file ownership and knowledge distribution.",
			MetricType: "list",
		},
	}
}

// Compute calculates file contributor statistics.
func (m *FileContributorMetric) Compute(input *ReportData) []FileContributorData {
	result := make([]FileContributorData, 0, len(input.Files))

	for path, fh := range input.Files {
		var topID, topLines int

		for devID, stats := range fh.People {
			totalLines := stats.Added + stats.Changed
			if totalLines > topLines {
				topLines = totalLines
				topID = devID
			}
		}

		result = append(result, FileContributorData{
			Path:                path,
			Contributors:        fh.People,
			TopContributorID:    topID,
			TopContributorLines: topLines,
		})
	}

	return result
}

// HotspotMetric identifies high-churn files.
type HotspotMetric struct {
	metrics.MetricMeta
}

// NewHotspotMetric creates the hotspot metric.
func NewHotspotMetric() *HotspotMetric {
	return &HotspotMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "hotspots",
			MetricDisplayName: "Code Hotspots",
			MetricDescription: "Identifies files with high change frequency that may indicate instability, " +
				"complexity, or areas requiring refactoring attention.",
			MetricType: "risk",
		},
	}
}

// Hotspot risk thresholds.
const (
	HotspotThresholdCritical = 50 // commits.
	HotspotThresholdHigh     = 30
	HotspotThresholdMedium   = 15
)

// Risk level constants.
const (
	RiskCritical = "CRITICAL"
	RiskHigh     = "HIGH"
	RiskMedium   = "MEDIUM"
	RiskLow      = "LOW"
)

// Churn score divisor for normalization.
const churnScoreDivisor = 100.0

// Risk priority values for sorting.
const (
	riskPriorityCritical = 0
	riskPriorityHigh     = 1
	riskPriorityMedium   = 2
	riskPriorityDefault  = 3
)

// Compute calculates hotspot data.
func (m *HotspotMetric) Compute(input *ReportData) []HotspotData {
	result := make([]HotspotData, 0, len(input.Files))

	for path, fh := range input.Files {
		commitCount := len(fh.Hashes)

		var totalAdded, totalRemoved, totalChanged int
		for _, stats := range fh.People {
			totalAdded += stats.Added
			totalRemoved += stats.Removed
			totalChanged += stats.Changed
		}

		churnScore := float64(commitCount) + float64(totalAdded+totalRemoved+totalChanged)/churnScoreDivisor

		var riskLevel string

		switch {
		case commitCount >= HotspotThresholdCritical:
			riskLevel = RiskCritical
		case commitCount >= HotspotThresholdHigh:
			riskLevel = RiskHigh
		case commitCount >= HotspotThresholdMedium:
			riskLevel = RiskMedium
		default:
			continue // Skip low-risk files.
		}

		result = append(result, HotspotData{
			Path:        path,
			CommitCount: commitCount,
			ChurnScore:  churnScore,
			RiskLevel:   riskLevel,
		})
	}

	// Sort by risk (critical first) then by commit count.
	sort.Slice(result, func(i, j int) bool {
		if result[i].RiskLevel != result[j].RiskLevel {
			return riskPriority(result[i].RiskLevel) < riskPriority(result[j].RiskLevel)
		}

		return result[i].CommitCount > result[j].CommitCount
	})

	return result
}

func riskPriority(level string) int {
	switch level {
	case RiskCritical:
		return riskPriorityCritical
	case RiskHigh:
		return riskPriorityHigh
	case RiskMedium:
		return riskPriorityMedium
	default:
		return riskPriorityDefault
	}
}

// AggregateMetric computes summary statistics.
type AggregateMetric struct {
	metrics.MetricMeta
}

// NewAggregateMetric creates the aggregate metric.
func NewAggregateMetric() *AggregateMetric {
	return &AggregateMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "file_history_aggregate",
			MetricDisplayName: "File History Summary",
			MetricDescription: "Aggregate statistics across all tracked files including total commits, " +
				"contributors, and average metrics.",
			MetricType: "aggregate",
		},
	}
}

// Compute calculates aggregate statistics.
func (m *AggregateMetric) Compute(input *ReportData) AggregateData {
	agg := AggregateData{
		TotalFiles: len(input.Files),
	}

	if agg.TotalFiles == 0 {
		return agg
	}

	allContributors := make(map[int]bool)

	var totalCommits, highChurnCount int

	for _, fh := range input.Files {
		totalCommits += len(fh.Hashes)

		for devID := range fh.People {
			allContributors[devID] = true
		}

		if len(fh.Hashes) >= HotspotThresholdMedium {
			highChurnCount++
		}
	}

	agg.TotalCommits = totalCommits
	agg.TotalContributors = len(allContributors)
	agg.HighChurnFiles = highChurnCount
	agg.AvgCommitsPerFile = float64(totalCommits) / float64(agg.TotalFiles)

	var totalContributorCount int

	for _, fh := range input.Files {
		totalContributorCount += len(fh.People)
	}

	agg.AvgContributorsPerFile = float64(totalContributorCount) / float64(agg.TotalFiles)

	return agg
}

// --- Computed Metrics ---.

// ComputedMetrics holds all computed metric results for the file history analyzer.
type ComputedMetrics struct {
	FileChurn        []FileChurnData       `json:"file_churn"        yaml:"file_churn"`
	FileContributors []FileContributorData `json:"file_contributors" yaml:"file_contributors"`
	Hotspots         []HotspotData         `json:"hotspots"          yaml:"hotspots"`
	Aggregate        AggregateData         `json:"aggregate"         yaml:"aggregate"`
}

const analyzerNameFileHistory = "file_history"

// AnalyzerName returns the analyzer identifier.
func (m *ComputedMetrics) AnalyzerName() string {
	return analyzerNameFileHistory
}

// ToJSON returns the metrics in JSON-serializable format.
func (m *ComputedMetrics) ToJSON() any {
	return m
}

// ToYAML returns the metrics in YAML-serializable format.
func (m *ComputedMetrics) ToYAML() any {
	return m
}

// ComputeAllMetrics runs all file history metrics and returns the results.
func ComputeAllMetrics(report analyze.Report) (*ComputedMetrics, error) {
	input, err := ParseReportData(report)
	if err != nil {
		return nil, err
	}

	churnMetric := NewFileChurnMetric()
	fileChurn := churnMetric.Compute(input)

	contribMetric := NewFileContributorMetric()
	fileContributors := contribMetric.Compute(input)

	hotspotMetric := NewHotspotMetric()
	hotspots := hotspotMetric.Compute(input)

	aggMetric := NewAggregateMetric()
	aggregate := aggMetric.Compute(input)

	return &ComputedMetrics{
		FileChurn:        fileChurn,
		FileContributors: fileContributors,
		Hotspots:         hotspots,
		Aggregate:        aggregate,
	}, nil
}
