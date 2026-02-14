package typos

import (
	"sort"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/metrics"
)

// --- Input Data Types ---.

// ReportData is the parsed input data for typos metrics computation.
type ReportData struct {
	Typos []Typo
}

// ParseReportData extracts ReportData from an analyzer report.
func ParseReportData(report analyze.Report) (*ReportData, error) {
	data := &ReportData{}

	if v, ok := report["typos"].([]Typo); ok {
		data.Typos = v
	}

	return data, nil
}

// --- Output Data Types ---.

// TypoData contains information about a single typo fix.
type TypoData struct {
	Wrong   string `json:"wrong"   yaml:"wrong"`
	Correct string `json:"correct" yaml:"correct"`
	File    string `json:"file"    yaml:"file"`
	Line    int    `json:"line"    yaml:"line"`
	Commit  string `json:"commit"  yaml:"commit"`
}

// TypoPatternData contains common typo patterns.
type TypoPatternData struct {
	Wrong     string `json:"wrong"     yaml:"wrong"`
	Correct   string `json:"correct"   yaml:"correct"`
	Frequency int    `json:"frequency" yaml:"frequency"`
}

// FileTypoData contains typo statistics per file.
type FileTypoData struct {
	File       string `json:"file"        yaml:"file"`
	TypoCount  int    `json:"typo_count"  yaml:"typo_count"`
	FixedTypos int    `json:"fixed_typos" yaml:"fixed_typos"`
}

// AggregateData contains summary statistics.
type AggregateData struct {
	TotalTypos      int `json:"total_typos"      yaml:"total_typos"`
	UniquePatterns  int `json:"unique_patterns"  yaml:"unique_patterns"`
	AffectedFiles   int `json:"affected_files"   yaml:"affected_files"`
	AffectedCommits int `json:"affected_commits" yaml:"affected_commits"`
}

// --- Metric Implementations ---.

// TypoListMetric computes the list of typo fixes.
type TypoListMetric struct {
	metrics.MetricMeta
}

// NewTypoListMetric creates the typo list metric.
func NewTypoListMetric() *TypoListMetric {
	return &TypoListMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "typo_list",
			MetricDisplayName: "Typo Fixes",
			MetricDescription: "List of identified typo-fix pairs extracted from commit history. " +
				"Shows the incorrect identifier and its correction.",
			MetricType: "list",
		},
	}
}

// Compute calculates typo list data.
func (m *TypoListMetric) Compute(input *ReportData) []TypoData {
	result := make([]TypoData, 0, len(input.Typos))

	for _, t := range input.Typos {
		result = append(result, TypoData{
			Wrong:   t.Wrong,
			Correct: t.Correct,
			File:    t.File,
			Line:    t.Line,
			Commit:  t.Commit.String(),
		})
	}

	return result
}

// TypoPatternMetric computes common typo patterns.
type TypoPatternMetric struct {
	metrics.MetricMeta
}

// NewTypoPatternMetric creates the typo pattern metric.
func NewTypoPatternMetric() *TypoPatternMetric {
	return &TypoPatternMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "typo_patterns",
			MetricDisplayName: "Typo Patterns",
			MetricDescription: "Common typo patterns that occur multiple times across the codebase. " +
				"Can be used to identify systematic naming issues.",
			MetricType: "list",
		},
	}
}

// Compute calculates typo patterns.
func (m *TypoPatternMetric) Compute(input *ReportData) []TypoPatternData {
	patterns := make(map[string]int)

	for _, t := range input.Typos {
		key := t.Wrong + "|" + t.Correct
		patterns[key]++
	}

	var result []TypoPatternData

	for key, freq := range patterns {
		if freq > 1 { // Only include patterns that occur more than once.
			// Split key back into wrong|correct.
			for i := range len(key) {
				if key[i] == '|' {
					result = append(result, TypoPatternData{
						Wrong:     key[:i],
						Correct:   key[i+1:],
						Frequency: freq,
					})

					break
				}
			}
		}
	}

	// Sort by frequency descending.
	sort.Slice(result, func(i, j int) bool {
		return result[i].Frequency > result[j].Frequency
	})

	return result
}

// FileTypoMetric computes typo statistics per file.
type FileTypoMetric struct {
	metrics.MetricMeta
}

// NewFileTypoMetric creates the file typo metric.
func NewFileTypoMetric() *FileTypoMetric {
	return &FileTypoMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "file_typos",
			MetricDisplayName: "File Typo Statistics",
			MetricDescription: "Per-file typo fix counts showing which files had the most typo corrections.",
			MetricType:        "list",
		},
	}
}

// Compute calculates file typo statistics.
func (m *FileTypoMetric) Compute(input *ReportData) []FileTypoData {
	fileCounts := make(map[string]int)

	for _, t := range input.Typos {
		fileCounts[t.File]++
	}

	result := make([]FileTypoData, 0, len(fileCounts))
	for file, count := range fileCounts {
		result = append(result, FileTypoData{
			File:       file,
			TypoCount:  count,
			FixedTypos: count,
		})
	}

	// Sort by typo count descending.
	sort.Slice(result, func(i, j int) bool {
		return result[i].TypoCount > result[j].TypoCount
	})

	return result
}

// AggregateMetric computes summary statistics.
type AggregateMetric struct {
	metrics.MetricMeta
}

// NewAggregateMetric creates the aggregate metric.
func NewAggregateMetric() *AggregateMetric {
	return &AggregateMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "typos_aggregate",
			MetricDisplayName: "Typos Summary",
			MetricDescription: "Aggregate statistics for typo analysis including total count, " +
				"unique patterns, and affected files.",
			MetricType: "aggregate",
		},
	}
}

// Compute calculates aggregate statistics.
func (m *AggregateMetric) Compute(input *ReportData) AggregateData {
	agg := AggregateData{
		TotalTypos: len(input.Typos),
	}

	patterns := make(map[string]bool)
	files := make(map[string]bool)
	commits := make(map[gitlib.Hash]bool)

	for _, t := range input.Typos {
		patterns[t.Wrong+"|"+t.Correct] = true
		files[t.File] = true
		commits[t.Commit] = true
	}

	agg.UniquePatterns = len(patterns)
	agg.AffectedFiles = len(files)
	agg.AffectedCommits = len(commits)

	return agg
}

// --- Computed Metrics ---.

// ComputedMetrics holds all computed metric results for the typos analyzer.
type ComputedMetrics struct {
	TypoList  []TypoData        `json:"typo_list"  yaml:"typo_list"`
	Patterns  []TypoPatternData `json:"patterns"   yaml:"patterns"`
	FileTypos []FileTypoData    `json:"file_typos" yaml:"file_typos"`
	Aggregate AggregateData     `json:"aggregate"  yaml:"aggregate"`
}

// Analyzer name constant for MetricsOutput interface.
const analyzerNameTypos = "typos"

// AnalyzerName returns the name of the analyzer.
func (m *ComputedMetrics) AnalyzerName() string {
	return analyzerNameTypos
}

// ToJSON returns the metrics as a JSON-serializable object.
func (m *ComputedMetrics) ToJSON() any {
	return m
}

// ToYAML returns the metrics as a YAML-serializable object.
func (m *ComputedMetrics) ToYAML() any {
	return m
}

// ComputeAllMetrics runs all typos metrics and returns the results.
func ComputeAllMetrics(report analyze.Report) (*ComputedMetrics, error) {
	input, err := ParseReportData(report)
	if err != nil {
		return nil, err
	}

	listMetric := NewTypoListMetric()
	typoList := listMetric.Compute(input)

	patternMetric := NewTypoPatternMetric()
	patterns := patternMetric.Compute(input)

	fileMetric := NewFileTypoMetric()
	fileTypos := fileMetric.Compute(input)

	aggMetric := NewAggregateMetric()
	aggregate := aggMetric.Compute(input)

	return &ComputedMetrics{
		TypoList:  typoList,
		Patterns:  patterns,
		FileTypos: fileTypos,
		Aggregate: aggregate,
	}, nil
}
