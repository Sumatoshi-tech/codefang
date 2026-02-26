package burndown

import (
	"time"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// Constants for burndown metrics calculations.
const (
	// Default tick size is 24 hours.
	defaultTickSizeHours = 24

	// Percent multiplier for calculations.
	percentMultiplier = 100

	// Modifier index offset for interaction calculations.
	modifierIndexOffset = 2

	// Self modifier ID.
	selfModifierID = -2
)

// ReportData is the parsed input data for burndown metrics computation.
type ReportData struct {
	GlobalHistory      DenseHistory
	FileHistories      map[string]DenseHistory
	FileOwnership      map[string]map[int]int
	PeopleHistories    []DenseHistory
	PeopleMatrix       DenseHistory
	ReversedPeopleDict []string
	TickSize           time.Duration
	Sampling           int
	Granularity        int
	ProjectName        string
	EndTime            time.Time
}

// ParseReportData extracts ReportData from an analyzer report.
func ParseReportData(report analyze.Report) (*ReportData, error) {
	data := &ReportData{}

	if gh, ok := report["GlobalHistory"].(DenseHistory); ok {
		data.GlobalHistory = gh
	}

	if fh, ok := report["FileHistories"].(map[string]DenseHistory); ok {
		data.FileHistories = fh
	}

	if fo, ok := report["FileOwnership"].(map[string]map[int]int); ok {
		data.FileOwnership = fo
	}

	if ph, ok := report["PeopleHistories"].([]DenseHistory); ok {
		data.PeopleHistories = ph
	}

	if pm, ok := report["PeopleMatrix"].(DenseHistory); ok {
		data.PeopleMatrix = pm
	}

	if rpd, ok := report["ReversedPeopleDict"].([]string); ok {
		data.ReversedPeopleDict = rpd
	}

	if ts, ok := report["TickSize"].(time.Duration); ok {
		data.TickSize = ts
	} else {
		data.TickSize = defaultTickSizeHours * time.Hour
	}

	if s, ok := report["Sampling"].(int); ok {
		data.Sampling = s
	}

	if g, ok := report["Granularity"].(int); ok {
		data.Granularity = g
	}

	if pn, ok := report["ProjectName"].(string); ok {
		data.ProjectName = pn
	}

	if et, ok := report["EndTime"].(time.Time); ok {
		data.EndTime = et
	}

	return data, nil
}

// SurvivalData contains code survival statistics for a time period.
type SurvivalData struct {
	SampleIndex   int     `json:"sample_index"   yaml:"sample_index"`
	TotalLines    int64   `json:"total_lines"    yaml:"total_lines"`
	SurvivalRate  float64 `json:"survival_rate"  yaml:"survival_rate"`
	BandBreakdown []int64 `json:"band_breakdown" yaml:"band_breakdown"`
}

// FileSurvivalData contains survival data for a single file.
type FileSurvivalData struct {
	Path         string      `json:"path"                 yaml:"path"`
	CurrentLines int64       `json:"current_lines"        yaml:"current_lines"`
	Ownership    map[int]int `json:"ownership"            yaml:"ownership"`
	TopOwnerID   int         `json:"top_owner_id"         yaml:"top_owner_id"`
	TopOwnerName string      `json:"top_owner_name"       yaml:"top_owner_name"`
	TopOwnerPct  float64     `json:"top_owner_percentage" yaml:"top_owner_percentage"`
}

// DeveloperSurvivalData contains survival data for a developer's code.
type DeveloperSurvivalData struct {
	ID           int     `json:"id"            yaml:"id"`
	Name         string  `json:"name"          yaml:"name"`
	CurrentLines int64   `json:"current_lines" yaml:"current_lines"`
	PeakLines    int64   `json:"peak_lines"    yaml:"peak_lines"`
	SurvivalRate float64 `json:"survival_rate" yaml:"survival_rate"`
}

// InteractionData contains developer interaction statistics.
type InteractionData struct {
	AuthorID      int    `json:"author_id"      yaml:"author_id"`
	AuthorName    string `json:"author_name"    yaml:"author_name"`
	ModifierID    int    `json:"modifier_id"    yaml:"modifier_id"`
	ModifierName  string `json:"modifier_name"  yaml:"modifier_name"`
	LinesModified int64  `json:"lines_modified" yaml:"lines_modified"`
	IsSelfModify  bool   `json:"is_self_modify" yaml:"is_self_modify"`
}

// AggregateData contains summary statistics.
type AggregateData struct {
	TotalCurrentLines   int64   `json:"total_current_lines"   yaml:"total_current_lines"`
	TotalPeakLines      int64   `json:"total_peak_lines"      yaml:"total_peak_lines"`
	OverallSurvivalRate float64 `json:"overall_survival_rate" yaml:"overall_survival_rate"`
	AnalysisPeriodDays  int     `json:"analysis_period_days"  yaml:"analysis_period_days"`
	NumBands            int     `json:"num_bands"             yaml:"num_bands"`
	NumSamples          int     `json:"num_samples"           yaml:"num_samples"`
	TrackedFiles        int     `json:"tracked_files"         yaml:"tracked_files"`
	TrackedDevelopers   int     `json:"tracked_developers"    yaml:"tracked_developers"`
}

func computeGlobalSurvival(input *ReportData) []SurvivalData {
	if len(input.GlobalHistory) == 0 {
		return nil
	}

	peakLines := findPeakLines(input.GlobalHistory)
	result := make([]SurvivalData, len(input.GlobalHistory))

	for i, sample := range input.GlobalHistory {
		result[i] = computeSurvivalSample(i, sample, peakLines)
	}

	return result
}

// findPeakLines computes the total lines ever written across all cohorts.
// Each band represents a cohort of code written during a time period.
// The peak of each band (its maximum value across all samples) is the number
// of lines originally written in that cohort. Summing these gives the total
// lines ever written, which is the correct denominator for survival rate.
func findPeakLines(history DenseHistory) int64 {
	if len(history) == 0 {
		return 0
	}

	numBands := len(history[0])
	bandPeaks := make([]int64, numBands)

	for _, sample := range history {
		for band := range min(len(sample), numBands) {
			if sample[band] > bandPeaks[band] {
				bandPeaks[band] = sample[band]
			}
		}
	}

	var total int64

	for _, peak := range bandPeaks {
		total += peak
	}

	return total
}

func sumPositiveValues(values []int64) int64 {
	var total int64

	for _, v := range values {
		if v > 0 {
			total += v
		}
	}

	return total
}

func computeSurvivalSample(index int, sample []int64, peakLines int64) SurvivalData {
	breakdown := make([]int64, len(sample))

	var total int64

	for j, v := range sample {
		if v > 0 {
			total += v
			breakdown[j] = v
		}
	}

	var survivalRate float64
	if peakLines > 0 {
		survivalRate = float64(total) / float64(peakLines)
	}

	return SurvivalData{
		SampleIndex:   index,
		TotalLines:    total,
		SurvivalRate:  survivalRate,
		BandBreakdown: breakdown,
	}
}

// FileSurvivalInput holds input for file survival computation.
type FileSurvivalInput struct {
	FileHistories      map[string]DenseHistory
	FileOwnership      map[string]map[int]int
	ReversedPeopleDict []string
}

// DeveloperSurvivalInput holds input for developer survival computation.
type DeveloperSurvivalInput struct {
	PeopleHistories    []DenseHistory
	ReversedPeopleDict []string
}

// InteractionInput holds input for interaction computation.
type InteractionInput struct {
	PeopleMatrix       DenseHistory
	ReversedPeopleDict []string
}

func computeFileSurvival(input FileSurvivalInput) []FileSurvivalData {
	result := make([]FileSurvivalData, 0, len(input.FileOwnership))

	for path, ownership := range input.FileOwnership {
		var (
			currentLines  int64
			topOwnerID    int
			topOwnerLines int
		)

		for devID, lines := range ownership {
			currentLines += int64(lines)
			if lines > topOwnerLines {
				topOwnerLines = lines
				topOwnerID = devID
			}
		}

		var topOwnerPct float64
		if currentLines > 0 {
			topOwnerPct = float64(topOwnerLines) / float64(currentLines) * percentMultiplier
		}

		topOwnerName := ""
		if topOwnerID >= 0 && topOwnerID < len(input.ReversedPeopleDict) {
			topOwnerName = input.ReversedPeopleDict[topOwnerID]
		}

		result = append(result, FileSurvivalData{
			Path:         path,
			CurrentLines: currentLines,
			Ownership:    ownership,
			TopOwnerID:   topOwnerID,
			TopOwnerName: topOwnerName,
			TopOwnerPct:  topOwnerPct,
		})
	}

	return result
}

func computeDeveloperSurvivalList(input DeveloperSurvivalInput) []DeveloperSurvivalData {
	result := make([]DeveloperSurvivalData, 0, len(input.PeopleHistories))

	for devID, history := range input.PeopleHistories {
		if len(history) == 0 {
			continue
		}

		data := computeDeveloperSurvival(devID, history, input.ReversedPeopleDict)
		result = append(result, data)
	}

	return result
}

func computeDeveloperSurvival(devID int, history DenseHistory, names []string) DeveloperSurvivalData {
	peakLines := findPeakLines(history)
	currentLines := sumPositiveValues(history[len(history)-1])

	var survivalRate float64
	if peakLines > 0 {
		survivalRate = float64(currentLines) / float64(peakLines)
	}

	name := ""
	if devID < len(names) {
		name = names[devID]
	}

	return DeveloperSurvivalData{
		ID:           devID,
		Name:         name,
		CurrentLines: currentLines,
		PeakLines:    peakLines,
		SurvivalRate: survivalRate,
	}
}

func computeInteraction(input InteractionInput) []InteractionData {
	if len(input.PeopleMatrix) == 0 {
		return nil
	}

	var result []InteractionData

	for authorID, row := range input.PeopleMatrix {
		if len(row) == 0 {
			continue
		}

		interactions := computeAuthorInteractions(authorID, row, input.ReversedPeopleDict)
		result = append(result, interactions...)
	}

	return result
}

func computeAuthorInteractions(authorID int, row []int64, names []string) []InteractionData {
	authorName := getName(authorID, names)

	var result []InteractionData

	for modifierIdx, lines := range row {
		if lines == 0 {
			continue
		}

		data := buildInteractionData(authorID, authorName, modifierIdx, lines, names)
		result = append(result, data)
	}

	return result
}

func buildInteractionData(authorID int, authorName string, modifierIdx int, lines int64, names []string) InteractionData {
	modifierID := modifierIdx - modifierIndexOffset
	isSelf := modifierID == selfModifierID

	modifierName := resolveModifierName(modifierID, authorName, isSelf, names)
	if isSelf {
		modifierID = authorID
	}

	return InteractionData{
		AuthorID:      authorID,
		AuthorName:    authorName,
		ModifierID:    modifierID,
		ModifierName:  modifierName,
		LinesModified: lines,
		IsSelfModify:  isSelf,
	}
}

func getName(id int, names []string) string {
	if id >= 0 && id < len(names) {
		return names[id]
	}

	return ""
}

func resolveModifierName(modifierID int, authorName string, isSelf bool, names []string) string {
	if isSelf {
		return authorName
	}

	return getName(modifierID, names)
}

func computeAggregate(input *ReportData) AggregateData {
	agg := AggregateData{
		TrackedFiles:      len(input.FileHistories),
		TrackedDevelopers: len(input.PeopleHistories),
	}

	if len(input.GlobalHistory) == 0 {
		return agg
	}

	agg.NumSamples = len(input.GlobalHistory)
	agg.NumBands = len(input.GlobalHistory[0])

	totalTicks := (agg.NumSamples - 1) * input.Sampling
	agg.AnalysisPeriodDays = int(time.Duration(totalTicks) * input.TickSize / (defaultTickSizeHours * time.Hour))

	agg.TotalPeakLines = findPeakLines(input.GlobalHistory)
	agg.TotalCurrentLines = sumPositiveValues(input.GlobalHistory[agg.NumSamples-1])

	if agg.TotalPeakLines > 0 {
		agg.OverallSurvivalRate = float64(agg.TotalCurrentLines) / float64(agg.TotalPeakLines)
	}

	return agg
}

// ComputedMetrics holds all computed metric results for the burndown analyzer.
type ComputedMetrics struct {
	Aggregate         AggregateData           `json:"aggregate"          yaml:"aggregate"`
	GlobalSurvival    []SurvivalData          `json:"global_survival"    yaml:"global_survival"`
	FileSurvival      []FileSurvivalData      `json:"file_survival"      yaml:"file_survival"`
	DeveloperSurvival []DeveloperSurvivalData `json:"developer_survival" yaml:"developer_survival"`
	Interaction       []InteractionData       `json:"interactions"       yaml:"interactions"`
}

// --- MetricsOutput Interface Implementation ---.

const analyzerNameBurndown = "burndown"

// AnalyzerName returns the analyzer identifier.
func (m *ComputedMetrics) AnalyzerName() string {
	return analyzerNameBurndown
}

// ToJSON returns the metrics in JSON-serializable format.
func (m *ComputedMetrics) ToJSON() any {
	return m
}

// ToYAML returns the metrics in YAML-serializable format.
func (m *ComputedMetrics) ToYAML() any {
	return m
}

// ComputeAllMetrics runs all burndown metrics and returns the results.
func ComputeAllMetrics(report analyze.Report) (*ComputedMetrics, error) {
	input, err := ParseReportData(report)
	if err != nil {
		return nil, err
	}

	globalSurvival := computeGlobalSurvival(input)

	fileSurvival := computeFileSurvival(FileSurvivalInput{
		FileHistories:      input.FileHistories,
		FileOwnership:      input.FileOwnership,
		ReversedPeopleDict: input.ReversedPeopleDict,
	})

	devSurvival := computeDeveloperSurvivalList(DeveloperSurvivalInput{
		PeopleHistories:    input.PeopleHistories,
		ReversedPeopleDict: input.ReversedPeopleDict,
	})

	interaction := computeInteraction(InteractionInput{
		PeopleMatrix:       input.PeopleMatrix,
		ReversedPeopleDict: input.ReversedPeopleDict,
	})

	aggregate := computeAggregate(input)

	return &ComputedMetrics{
		GlobalSurvival:    globalSurvival,
		FileSurvival:      fileSurvival,
		DeveloperSurvival: devSurvival,
		Interaction:       interaction,
		Aggregate:         aggregate,
	}, nil
}
