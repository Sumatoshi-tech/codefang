package couples

import (
	"sort"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/metrics"
)

// --- Input Data Types ---.

// ReportData is the parsed input data for couples metrics computation.
type ReportData struct {
	PeopleMatrix       []map[int]int64
	PeopleFiles        [][]int
	Files              []string
	FilesLines         []int
	FilesMatrix        []map[int]int64
	ReversedPeopleDict []string
}

// ParseReportData extracts ReportData from an analyzer report.
func ParseReportData(report analyze.Report) (*ReportData, error) {
	data := &ReportData{}

	if v, ok := report["PeopleMatrix"].([]map[int]int64); ok {
		data.PeopleMatrix = v
	}

	if v, ok := report["PeopleFiles"].([][]int); ok {
		data.PeopleFiles = v
	}

	if v, ok := report["Files"].([]string); ok {
		data.Files = v
	}

	if v, ok := report["FilesLines"].([]int); ok {
		data.FilesLines = v
	}

	if v, ok := report["FilesMatrix"].([]map[int]int64); ok {
		data.FilesMatrix = v
	}

	if v, ok := report["ReversedPeopleDict"].([]string); ok {
		data.ReversedPeopleDict = v
	}

	return data, nil
}

// --- Output Data Types ---.

// FileCouplingData contains coupling data for a file pair.
type FileCouplingData struct {
	File1     string  `json:"file1"             yaml:"file1"`
	File2     string  `json:"file2"             yaml:"file2"`
	CoChanges int64   `json:"co_changes"        yaml:"co_changes"`
	Strength  float64 `json:"coupling_strength" yaml:"coupling_strength"`
}

// DeveloperCouplingData contains coupling data for a developer pair.
type DeveloperCouplingData struct {
	Developer1  string  `json:"developer1"          yaml:"developer1"`
	Developer2  string  `json:"developer2"          yaml:"developer2"`
	SharedFiles int64   `json:"shared_file_changes" yaml:"shared_file_changes"`
	Strength    float64 `json:"coupling_strength"   yaml:"coupling_strength"`
}

// FileOwnershipData contains ownership information for a file.
type FileOwnershipData struct {
	File           string `json:"file"                      yaml:"file"`
	Lines          int    `json:"lines"                     yaml:"lines"`
	Contributors   int    `json:"contributors"              yaml:"contributors"`
	TopContributor string `json:"top_contributor,omitempty" yaml:"top_contributor,omitempty"`
}

// AggregateData contains summary statistics.
type AggregateData struct {
	TotalFiles          int     `json:"total_files"           yaml:"total_files"`
	TotalDevelopers     int     `json:"total_developers"      yaml:"total_developers"`
	TotalCoChanges      int64   `json:"total_co_changes"      yaml:"total_co_changes"`
	AvgCouplingStrength float64 `json:"avg_coupling_strength" yaml:"avg_coupling_strength"`
	HighlyCoupledPairs  int     `json:"highly_coupled_pairs"  yaml:"highly_coupled_pairs"`
}

// --- Metric Implementations ---.

// FileCouplingMetric computes file co-change coupling.
type FileCouplingMetric struct {
	metrics.MetricMeta
}

// NewFileCouplingMetric creates the file coupling metric.
func NewFileCouplingMetric() *FileCouplingMetric {
	return &FileCouplingMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "file_coupling",
			MetricDisplayName: "File Coupling",
			MetricDescription: "Measures how often pairs of files change together in commits. " +
				"High coupling may indicate tight dependencies or shared functionality.",
			MetricType: "list",
		},
	}
}

// CouplingThresholdHigh is the coupling strength threshold.
const CouplingThresholdHigh = 10

// Compute calculates file coupling data.
func (m *FileCouplingMetric) Compute(input *ReportData) []FileCouplingData {
	var result []FileCouplingData

	for i, row := range input.FilesMatrix {
		if i >= len(input.Files) {
			continue
		}

		file1 := input.Files[i]

		for j, coChanges := range row {
			if j <= i || j >= len(input.Files) {
				continue // Skip self and lower triangle.
			}

			if coChanges == 0 {
				continue
			}

			file2 := input.Files[j]

			// Calculate coupling strength (normalized by max possible).
			maxChanges := max(coChanges, row[i])

			var strength float64

			if maxChanges > 0 {
				strength = float64(coChanges) / float64(maxChanges)
			}

			result = append(result, FileCouplingData{
				File1:     file1,
				File2:     file2,
				CoChanges: coChanges,
				Strength:  strength,
			})
		}
	}

	// Sort by co-changes descending.
	sort.Slice(result, func(i, j int) bool {
		return result[i].CoChanges > result[j].CoChanges
	})

	return result
}

// DeveloperCouplingMetric computes developer collaboration coupling.
type DeveloperCouplingMetric struct {
	metrics.MetricMeta
}

// NewDeveloperCouplingMetric creates the developer coupling metric.
func NewDeveloperCouplingMetric() *DeveloperCouplingMetric {
	return &DeveloperCouplingMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "developer_coupling",
			MetricDisplayName: "Developer Coupling",
			MetricDescription: "Measures how often pairs of developers work on the same files. " +
				"High coupling indicates collaboration or shared code ownership.",
			MetricType: "list",
		},
	}
}

// Compute calculates developer coupling data.
func (m *DeveloperCouplingMetric) Compute(input *ReportData) []DeveloperCouplingData {
	result := make([]DeveloperCouplingData, 0, len(input.PeopleMatrix))

	for i, row := range input.PeopleMatrix {
		couplings := computeDevCouplings(i, row, input.ReversedPeopleDict)
		result = append(result, couplings...)
	}

	sort.Slice(result, func(i, j int) bool {
		return result[i].SharedFiles > result[j].SharedFiles
	})

	return result
}

func computeDevCouplings(devIdx int, row map[int]int64, names []string) []DeveloperCouplingData {
	dev1 := getDevName(devIdx, names)

	var result []DeveloperCouplingData

	for j, sharedChanges := range row {
		if j <= devIdx || sharedChanges == 0 {
			continue
		}

		coupling := buildCouplingData(dev1, j, sharedChanges, row[devIdx], names)
		result = append(result, coupling)
	}

	return result
}

func buildCouplingData(dev1 string, dev2Idx int, sharedChanges, selfChanges int64, names []string) DeveloperCouplingData {
	dev2 := getDevName(dev2Idx, names)
	maxChanges := max(sharedChanges, selfChanges)

	var strength float64
	if maxChanges > 0 {
		strength = float64(sharedChanges) / float64(maxChanges)
	}

	return DeveloperCouplingData{
		Developer1:  dev1,
		Developer2:  dev2,
		SharedFiles: sharedChanges,
		Strength:    strength,
	}
}

func getDevName(idx int, names []string) string {
	if idx < len(names) {
		return names[idx]
	}

	return ""
}

// FileOwnershipMetric computes file ownership information.
type FileOwnershipMetric struct {
	metrics.MetricMeta
}

// NewFileOwnershipMetric creates the file ownership metric.
func NewFileOwnershipMetric() *FileOwnershipMetric {
	return &FileOwnershipMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "file_ownership",
			MetricDisplayName: "File Ownership",
			MetricDescription: "Shows file size and contributor information for each tracked file.",
			MetricType:        "list",
		},
	}
}

// Compute calculates file ownership data.
func (m *FileOwnershipMetric) Compute(input *ReportData) []FileOwnershipData {
	result := make([]FileOwnershipData, 0, len(input.Files))

	// Build reverse index: file index -> developers who touched it.
	fileContributors := make([]map[int]bool, len(input.Files))
	for i := range fileContributors {
		fileContributors[i] = make(map[int]bool)
	}

	for devID, fileIndices := range input.PeopleFiles {
		for _, fileIdx := range fileIndices {
			if fileIdx < len(fileContributors) {
				fileContributors[fileIdx][devID] = true
			}
		}
	}

	for i, file := range input.Files {
		lines := 0
		if i < len(input.FilesLines) {
			lines = input.FilesLines[i]
		}

		contributors := len(fileContributors[i])

		result = append(result, FileOwnershipData{
			File:         file,
			Lines:        lines,
			Contributors: contributors,
		})
	}

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
			MetricName:        "couples_aggregate",
			MetricDisplayName: "Couples Summary",
			MetricDescription: "Aggregate statistics for file and developer coupling analysis.",
			MetricType:        "aggregate",
		},
	}
}

// Compute calculates aggregate statistics.
func (m *AggregateMetric) Compute(input *ReportData) AggregateData {
	agg := AggregateData{
		TotalFiles:      len(input.Files),
		TotalDevelopers: len(input.ReversedPeopleDict),
	}

	var totalCoChanges int64

	var pairCount int

	var highlyCouplded int

	for i, row := range input.FilesMatrix {
		for j, coChanges := range row {
			if j <= i {
				continue
			}

			if coChanges > 0 {
				totalCoChanges += coChanges
				pairCount++

				if coChanges >= CouplingThresholdHigh {
					highlyCouplded++
				}
			}
		}
	}

	agg.TotalCoChanges = totalCoChanges
	agg.HighlyCoupledPairs = highlyCouplded

	if pairCount > 0 {
		agg.AvgCouplingStrength = float64(totalCoChanges) / float64(pairCount)
	}

	return agg
}

// --- Computed Metrics ---.

// ComputedMetrics holds all computed metric results for the couples analyzer.
type ComputedMetrics struct {
	FileCoupling      []FileCouplingData      `json:"file_coupling"      yaml:"file_coupling"`
	DeveloperCoupling []DeveloperCouplingData `json:"developer_coupling" yaml:"developer_coupling"`
	FileOwnership     []FileOwnershipData     `json:"file_ownership"     yaml:"file_ownership"`
	Aggregate         AggregateData           `json:"aggregate"          yaml:"aggregate"`
}

const analyzerNameCouples = "couples"

// AnalyzerName returns the analyzer identifier.
func (m *ComputedMetrics) AnalyzerName() string {
	return analyzerNameCouples
}

// ToJSON returns the metrics in JSON-serializable format.
func (m *ComputedMetrics) ToJSON() any {
	return m
}

// ToYAML returns the metrics in YAML-serializable format.
func (m *ComputedMetrics) ToYAML() any {
	return m
}

// ComputeAllMetrics runs all couples metrics and returns the results.
func ComputeAllMetrics(report analyze.Report) (*ComputedMetrics, error) {
	input, err := ParseReportData(report)
	if err != nil {
		return nil, err
	}

	fileMetric := NewFileCouplingMetric()
	fileCoupling := fileMetric.Compute(input)

	devMetric := NewDeveloperCouplingMetric()
	devCoupling := devMetric.Compute(input)

	ownerMetric := NewFileOwnershipMetric()
	fileOwnership := ownerMetric.Compute(input)

	aggMetric := NewAggregateMetric()
	aggregate := aggMetric.Compute(input)

	return &ComputedMetrics{
		FileCoupling:      fileCoupling,
		DeveloperCoupling: devCoupling,
		FileOwnership:     fileOwnership,
		Aggregate:         aggregate,
	}, nil
}
