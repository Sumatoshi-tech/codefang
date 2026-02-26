package couples

import (
	"encoding/binary"
	"sort"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/hll"
	"github.com/Sumatoshi-tech/codefang/pkg/metrics"
)

// HLL configuration for per-file contributor cardinality.
const (
	fileContribHLLPrecision = 10 // 1024 registers, ~3% error — sufficient for contributor counts.
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

			// Calculate coupling strength using code-maat formula:
			// degree = co_changes / average(revisions_file1, revisions_file2)
			// where revisions = diagonal element (self-change count).
			selfI := row[i]                       // file1 self-changes.
			selfJ := input.FilesMatrix[j][j]      // file2 self-changes.
			avgRevs := float64(selfI+selfJ) / 2.0 //nolint:mnd // average of two values.

			var strength float64

			if avgRevs > 0 {
				strength = min(float64(coChanges)/avgRevs, 1.0)
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
		couplings := computeDevCouplings(i, row, input.PeopleMatrix, input.ReversedPeopleDict)
		result = append(result, couplings...)
	}

	sort.Slice(result, func(i, j int) bool {
		return result[i].SharedFiles > result[j].SharedFiles
	})

	return result
}

func computeDevCouplings(devIdx int, row map[int]int64, matrix []map[int]int64, names []string) []DeveloperCouplingData {
	dev1 := getDevName(devIdx, names)

	var result []DeveloperCouplingData

	for j, sharedChanges := range row {
		if j <= devIdx || sharedChanges == 0 {
			continue
		}

		selfDev2 := int64(0)
		if j < len(matrix) {
			selfDev2 = matrix[j][j]
		}

		coupling := buildCouplingData(dev1, j, sharedChanges, row[devIdx], selfDev2, names)
		result = append(result, coupling)
	}

	return result
}

func buildCouplingData(dev1 string, dev2Idx int, sharedChanges, selfDev1, selfDev2 int64, names []string) DeveloperCouplingData {
	dev2 := getDevName(dev2Idx, names)

	// Coupling strength using code-maat formula:
	// degree = shared_changes / average(self_dev1, self_dev2), capped at 1.0.
	avgRevs := float64(selfDev1+selfDev2) / 2.0 //nolint:mnd // average of two values.

	var strength float64
	if avgRevs > 0 {
		strength = min(float64(sharedChanges)/avgRevs, 1.0)
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
//
// Uses HyperLogLog sketches per file to estimate contributor cardinality
// instead of maintaining a map[int]bool per file. This reduces memory from
// O(F × D) to O(F × 2^p) where p is the HLL precision.
func (m *FileOwnershipMetric) Compute(input *ReportData) []FileOwnershipData {
	fileSketches := buildFileContributorSketches(len(input.Files), input.PeopleFiles)
	result := make([]FileOwnershipData, 0, len(input.Files))

	for i, file := range input.Files {
		lines := 0
		if i < len(input.FilesLines) {
			lines = input.FilesLines[i]
		}

		contributors := 0
		if i < len(fileSketches) && fileSketches[i] != nil {
			contributors = int(fileSketches[i].Count())
		}

		result = append(result, FileOwnershipData{
			File:         file,
			Lines:        lines,
			Contributors: contributors,
		})
	}

	return result
}

// buildFileContributorSketches creates per-file HLL sketches and populates them
// from the people-files mapping.
func buildFileContributorSketches(numFiles int, peopleFiles [][]int) []*hll.Sketch {
	sketches := make([]*hll.Sketch, numFiles)
	for i := range sketches {
		sketch, err := hll.New(fileContribHLLPrecision)
		if err != nil {
			continue
		}

		sketches[i] = sketch
	}

	var devBuf [8]byte

	for devID, fileIndices := range peopleFiles {
		binary.LittleEndian.PutUint64(devBuf[:], uint64(devID))

		for _, fileIdx := range fileIndices {
			if fileIdx < len(sketches) && sketches[fileIdx] != nil {
				sketches[fileIdx].Add(devBuf[:])
			}
		}
	}

	return sketches
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

// aggregateAccum accumulates statistics while iterating file pairs.
type aggregateAccum struct {
	totalCoChanges int64
	pairCount      int
	highlyCoupled  int
	totalStrength  float64
}

func (a *aggregateAccum) addPair(coChanges, selfI, selfJ int64) {
	a.totalCoChanges += coChanges
	a.pairCount++

	if coChanges >= CouplingThresholdHigh {
		a.highlyCoupled++
	}

	avgRevs := float64(selfI+selfJ) / 2.0 //nolint:mnd // average of two values.
	if avgRevs > 0 {
		a.totalStrength += min(float64(coChanges)/avgRevs, 1.0)
	}
}

// Compute calculates aggregate statistics.
func (m *AggregateMetric) Compute(input *ReportData) AggregateData {
	agg := AggregateData{
		TotalFiles:      len(input.Files),
		TotalDevelopers: len(input.ReversedPeopleDict),
	}

	var acc aggregateAccum

	for i, row := range input.FilesMatrix {
		for j, coChanges := range row {
			if j <= i || coChanges <= 0 {
				continue
			}

			acc.addPair(coChanges, row[i], input.FilesMatrix[j][j])
		}
	}

	agg.TotalCoChanges = acc.totalCoChanges
	agg.HighlyCoupledPairs = acc.highlyCoupled

	if acc.pairCount > 0 {
		agg.AvgCouplingStrength = acc.totalStrength / float64(acc.pairCount)
	}

	return agg
}

// --- Data Reduction / Bucketing (used by both text and plot renderers) ---.

// OwnershipBucket categorizes files by their contributor count.
type OwnershipBucket struct {
	Label string `json:"label" yaml:"label"`
	Count int    `json:"count" yaml:"count"`
}

// Ownership contributor count thresholds.
const (
	ownershipFewThreshold      = 3
	ownershipModerateThreshold = 5
)

// BucketOwnership groups file ownership data into contributor count categories.
func BucketOwnership(ownership []FileOwnershipData) []OwnershipBucket {
	single, few, moderate, many := 0, 0, 0, 0

	for _, fo := range ownership {
		switch {
		case fo.Contributors <= 1:
			single++
		case fo.Contributors <= ownershipFewThreshold:
			few++
		case fo.Contributors <= ownershipModerateThreshold:
			moderate++
		default:
			many++
		}
	}

	return []OwnershipBucket{
		{Label: "Single owner", Count: single},
		{Label: "2-3 owners", Count: few},
		{Label: "4-5 owners", Count: moderate},
		{Label: "6+ owners", Count: many},
	}
}

// SortOwnershipByRisk returns a copy sorted by contributors ascending (highest risk first).
func SortOwnershipByRisk(ownership []FileOwnershipData) []FileOwnershipData {
	sorted := make([]FileOwnershipData, len(ownership))
	copy(sorted, ownership)

	sort.Slice(sorted, func(i, j int) bool {
		return sorted[i].Contributors < sorted[j].Contributors
	})

	return sorted
}

// FilterTopDevs limits a developer coupling matrix to the top N developers
// ranked by diagonal value (activity). Returns the original data if within limit.
func FilterTopDevs(matrix []map[int]int64, names []string, limit int) (filtered []map[int]int64, filteredNames []string) {
	if len(names) <= limit {
		return matrix, names
	}

	// Rank developers by diagonal value (self-activity).
	type devActivity struct {
		idx      int
		activity int64
	}

	devs := make([]devActivity, len(names))
	for i := range names {
		devs[i] = devActivity{idx: i, activity: matrix[i][i]}
	}

	sort.Slice(devs, func(a, b int) bool {
		return devs[a].activity > devs[b].activity
	})

	// Take top N.
	topN := devs[:limit]

	// Build index mapping: old index → new index.
	oldToNew := make(map[int]int, limit)
	newNames := make([]string, limit)

	for newIdx, d := range topN {
		oldToNew[d.idx] = newIdx
		newNames[newIdx] = names[d.idx]
	}

	// Build filtered sub-matrix.
	newMatrix := make([]map[int]int64, limit)
	for i := range newMatrix {
		newMatrix[i] = make(map[int]int64)
	}

	for _, d := range topN {
		oldI := d.idx
		newI := oldToNew[oldI]

		for oldJ, val := range matrix[oldI] {
			newJ, ok := oldToNew[oldJ]
			if ok {
				newMatrix[newI][newJ] = val
			}
		}
	}

	return newMatrix, newNames
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
