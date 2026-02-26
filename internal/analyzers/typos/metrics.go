package typos

import (
	"sort"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
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

	return &ComputedMetrics{
		TypoList:  computeTypoList(input),
		Patterns:  computeTypoPatterns(input),
		FileTypos: computeFileTypos(input),
		Aggregate: computeAggregate(input),
	}, nil
}

// --- Metric Implementations ---.

func computeTypoList(input *ReportData) []TypoData {
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

func computeTypoPatterns(input *ReportData) []TypoPatternData {
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

func computeFileTypos(input *ReportData) []FileTypoData {
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

func computeAggregate(input *ReportData) AggregateData {
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
