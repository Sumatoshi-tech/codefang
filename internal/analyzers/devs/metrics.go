package devs

import (
	"errors"
	"fmt"
	"sort"
	"strconv"
	"time"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/identity"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/hll"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/metrics"
)

// Error definitions for the devs analyzer.
var (
	ErrInvalidPeopleDict = errors.New("devs: invalid ReversedPeopleDict in report")
)

// hllPrecision is the HyperLogLog precision for developer cardinality estimation.
// p=14 → 16384 registers (16 KB), ~0.8% standard error.
const hllPrecision = 14

// devIDBase is the numeric base for converting developer IDs to bytes.
const devIDBase = 10

// devIDBytes converts a developer ID to a deterministic byte slice for HLL.
func devIDBytes(id int) []byte {
	return strconv.AppendInt(nil, int64(id), devIDBase)
}

// --- Input Data Types ---.

// TickData is the raw input data for devs metrics computation.
type TickData struct {
	Ticks     map[int]map[int]*DevTick
	Names     []string
	TickSize  time.Duration
	DevSketch *hll.Sketch `json:"-" yaml:"-"`
}

// AggregateCommitsToTicks builds per-tick per-developer data from per-commit
// data grouped by the commits_by_tick mapping.
func AggregateCommitsToTicks(
	commitDevData map[string]*CommitDevData,
	commitsByTick map[int][]gitlib.Hash,
) map[int]map[int]*DevTick {
	if len(commitDevData) == 0 || len(commitsByTick) == 0 {
		return nil
	}

	result := make(map[int]map[int]*DevTick, len(commitsByTick))

	for tick, hashes := range commitsByTick {
		devTicks := aggregateDevTickFromCommits(hashes, commitDevData)
		if len(devTicks) > 0 {
			result[tick] = devTicks
		}
	}

	return result
}

// aggregateDevTickFromCommits merges commit-level dev data into per-author DevTick entries for a single tick.
func aggregateDevTickFromCommits(hashes []gitlib.Hash, commitDevData map[string]*CommitDevData) map[int]*DevTick {
	devTicks := make(map[int]*DevTick)

	for _, hash := range hashes {
		cdd, ok := commitDevData[hash.String()]
		if !ok {
			continue
		}

		dt := devTicks[cdd.AuthorID]
		if dt == nil {
			dt = &DevTick{Languages: make(map[string]pkgplumbing.LineStats)}
			devTicks[cdd.AuthorID] = dt
		}

		dt.Commits += cdd.Commits
		dt.Added += cdd.Added
		dt.Removed += cdd.Removed
		dt.Changed += cdd.Changed

		for lang, stats := range cdd.Languages {
			ls := dt.Languages[lang]
			dt.Languages[lang] = pkgplumbing.LineStats{
				Added:   ls.Added + stats.Added,
				Removed: ls.Removed + stats.Removed,
				Changed: ls.Changed + stats.Changed,
			}
		}
	}

	return devTicks
}

// ParseTickData extracts TickData from an analyzer report.
func ParseTickData(report analyze.Report) (*TickData, error) {
	names, err := parseReversedPeopleDict(report)
	if err != nil {
		return nil, err
	}

	tickSize := parseTickSize(report)
	commitDevData, _ := parseCommitDevData(report)
	commitsByTick, _ := parseCommitsByTick(report)

	var ticks map[int]map[int]*DevTick

	if len(commitDevData) > 0 && len(commitsByTick) > 0 {
		ticks = AggregateCommitsToTicks(commitDevData, commitsByTick)
	}

	if ticks == nil {
		ticks = make(map[int]map[int]*DevTick)
	}

	td := &TickData{
		Ticks:    ticks,
		Names:    names,
		TickSize: tickSize,
	}

	td.DevSketch = buildDevSketch(ticks)

	return td, nil
}

// buildDevSketch creates an HLL sketch from all unique developer IDs across ticks.
// Returns nil if no ticks contain developer data.
func buildDevSketch(ticks map[int]map[int]*DevTick) *hll.Sketch {
	if len(ticks) == 0 {
		return nil
	}

	sketch, err := hll.New(hllPrecision)
	if err != nil {
		return nil
	}

	for _, devTicks := range ticks {
		for devID := range devTicks {
			sketch.Add(devIDBytes(devID))
		}
	}

	return sketch
}

func parseReversedPeopleDict(report analyze.Report) ([]string, error) {
	v, ok := report["ReversedPeopleDict"]
	if !ok {
		return nil, ErrInvalidPeopleDict
	}

	if s, isStrSlice := v.([]string); isStrSlice {
		return s, nil
	}

	if arr, isAnySlice := v.([]any); isAnySlice {
		var names []string

		for _, x := range arr {
			if str, isStr := x.(string); isStr {
				names = append(names, str)
			}
		}

		return names, nil
	}

	return nil, ErrInvalidPeopleDict
}

func parseTickSize(report analyze.Report) time.Duration {
	if v, ok := report["TickSize"]; ok {
		switch ts := v.(type) {
		case time.Duration:
			if ts > 0 {
				return ts
			}
		case float64:
			if ts > 0 {
				return time.Duration(ts)
			}
		case int:
			if ts > 0 {
				return time.Duration(ts)
			}
		}
	}

	return defaultTickHours * time.Hour
}

func parseCommitDevData(report analyze.Report) (map[string]*CommitDevData, bool) {
	v, ok := report["CommitDevData"]
	if !ok {
		return nil, false
	}

	if cdd, isType := v.(map[string]*CommitDevData); isType {
		return cdd, true
	}

	if cddMap, isMap := v.(map[string]any); isMap {
		return buildCommitDevDataFromMap(cddMap), true
	}

	return nil, false
}

func buildCommitDevDataFromMap(cddMap map[string]any) map[string]*CommitDevData {
	res := make(map[string]*CommitDevData)

	for hash, dataAny := range cddMap {
		if dataMap, isMap := dataAny.(map[string]any); isMap {
			res[hash] = &CommitDevData{
				Commits:   intVal(dataMap["commits"]),
				Added:     intVal(dataMap["lines_added"]),
				Removed:   intVal(dataMap["lines_removed"]),
				Changed:   intVal(dataMap["lines_changed"]),
				AuthorID:  intVal(dataMap["author_id"]),
				Languages: parseLanguages(dataMap["languages"]),
			}
		}
	}

	return res
}

func intVal(v any) int {
	switch val := v.(type) {
	case float64:
		return int(val)
	case int:
		return val
	}

	return 0
}

func parseLanguages(v any) map[string]pkgplumbing.LineStats {
	res := make(map[string]pkgplumbing.LineStats)

	langsAny, isMap := v.(map[string]any)
	if !isMap {
		return res
	}

	for lang, statsAny := range langsAny {
		statsMap, okMap := statsAny.(map[string]any)
		if okMap {
			res[lang] = pkgplumbing.LineStats{
				Added:   intVal(statsMap["added"]),
				Removed: intVal(statsMap["removed"]),
				Changed: intVal(statsMap["changed"]),
			}
		}
	}

	return res
}

func parseCommitsByTick(report analyze.Report) (map[int][]gitlib.Hash, bool) {
	v, ok := report["CommitsByTick"]
	if !ok {
		return nil, false
	}

	if cbt, isType := v.(map[int][]gitlib.Hash); isType {
		return cbt, true
	}

	if cbtMap, isMap := v.(map[string]any); isMap {
		return buildCommitsByTickFromMap(cbtMap), true
	}

	return nil, false
}

func buildCommitsByTickFromMap(cbtMap map[string]any) map[int][]gitlib.Hash {
	res := make(map[int][]gitlib.Hash)

	for kStr, hashesAny := range cbtMap {
		k, err := strconv.Atoi(kStr)
		if err != nil {
			continue
		}

		if hashesArr, isArr := hashesAny.([]any); isArr {
			for _, hAny := range hashesArr {
				if hStr, isStr := hAny.(string); isStr {
					res[k] = append(res[k], gitlib.NewHash(hStr))
				}
			}
		}
	}

	return res
}

// DeveloperData contains computed data for a single developer.
type DeveloperData struct {
	ID          int                              `json:"id"            yaml:"id"`
	Name        string                           `json:"name"          yaml:"name"`
	Commits     int                              `json:"commits"       yaml:"commits"`
	Added       int                              `json:"lines_added"   yaml:"lines_added"`
	Removed     int                              `json:"lines_removed" yaml:"lines_removed"`
	Changed     int                              `json:"lines_changed" yaml:"lines_changed"`
	NetLines    int                              `json:"net_lines"     yaml:"net_lines"`
	Languages   map[string]pkgplumbing.LineStats `json:"languages"     yaml:"languages"`
	FirstTick   int                              `json:"first_tick"    yaml:"first_tick"`
	LastTick    int                              `json:"last_tick"     yaml:"last_tick"`
	ActiveTicks int                              `json:"active_ticks"  yaml:"active_ticks"`
}

// LanguageData contains computed data for a programming language.
type LanguageData struct {
	Name              string      `json:"name"               yaml:"name"`
	TotalLines        int         `json:"total_lines"        yaml:"total_lines"`
	TotalContribution int         `json:"total_contribution" yaml:"total_contribution"`
	Contributors      map[int]int `json:"contributors"       yaml:"contributors"`
}

// BusFactorData contains knowledge concentration data for a language.
// BusFactor follows the CHAOSS Contributor Absence Factor methodology:
// the smallest number of contributors responsible for 50% of total contributions.
type BusFactorData struct {
	Language          string  `json:"language"                       yaml:"language"`
	BusFactor         int     `json:"bus_factor"                     yaml:"bus_factor"`
	TotalContributors int     `json:"total_contributors"             yaml:"total_contributors"`
	PrimaryDevID      int     `json:"primary_dev_id"                 yaml:"primary_dev_id"`
	PrimaryDevName    string  `json:"primary_dev_name"               yaml:"primary_dev_name"`
	PrimaryPct        float64 `json:"primary_percentage"             yaml:"primary_percentage"`
	SecondaryDevID    int     `json:"secondary_dev_id,omitempty"     yaml:"secondary_dev_id,omitempty"`
	SecondaryDevName  string  `json:"secondary_dev_name,omitempty"   yaml:"secondary_dev_name,omitempty"`
	SecondaryPct      float64 `json:"secondary_percentage,omitempty" yaml:"secondary_percentage,omitempty"`
	RiskLevel         string  `json:"risk_level"                     yaml:"risk_level"`
}

// ActivityData contains time-series activity for a single tick.
type ActivityData struct {
	Tick         int         `json:"tick"          yaml:"tick"`
	ByDeveloper  map[int]int `json:"by_developer"  yaml:"by_developer"`
	TotalCommits int         `json:"total_commits" yaml:"total_commits"`
}

// ChurnData contains code churn for a single tick.
type ChurnData struct {
	Tick    int `json:"tick"          yaml:"tick"`
	Added   int `json:"lines_added"   yaml:"lines_added"`
	Removed int `json:"lines_removed" yaml:"lines_removed"`
	Net     int `json:"net_change"    yaml:"net_change"`
}

// AggregateData contains summary statistics.
type AggregateData struct {
	TotalCommits              int    `json:"total_commits"               yaml:"total_commits"`
	TotalLinesAdded           int    `json:"total_lines_added"           yaml:"total_lines_added"`
	TotalLinesRemoved         int    `json:"total_lines_removed"         yaml:"total_lines_removed"`
	TotalDevelopers           int    `json:"total_developers"            yaml:"total_developers"`
	ActiveDevelopers          int    `json:"active_developers"           yaml:"active_developers"`
	EstimatedTotalDevelopers  uint64 `json:"estimated_total_developers"  yaml:"estimated_total_developers"`
	EstimatedActiveDevelopers uint64 `json:"estimated_active_developers" yaml:"estimated_active_developers"`
	AnalysisPeriodTicks       int    `json:"analysis_period_ticks"       yaml:"analysis_period_ticks"`
	ProjectBusFactor          int    `json:"project_bus_factor"          yaml:"project_bus_factor"`
	TotalLanguages            int    `json:"total_languages"             yaml:"total_languages"`
}

// --- Metric Implementations ---.

// DevelopersMetric computes per-developer statistics.
type DevelopersMetric struct {
	metrics.MetricMeta
}

// NewDevelopersMetric creates the developers metric.
func NewDevelopersMetric() *DevelopersMetric {
	return &DevelopersMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "developers",
			MetricDisplayName: "Developer Statistics",
			MetricDescription: "Per-developer contribution statistics including commits, lines added/removed, " +
				"language breakdown, and activity timeline. Developers are sorted by commit count.",
			MetricType: "list",
		},
	}
}

// Compute calculates developer statistics from tick data.
func (m *DevelopersMetric) Compute(input *TickData) []DeveloperData {
	devMap := make(map[int]*DeveloperData)

	for tick, devTicks := range input.Ticks {
		processTickDevs(tick, devTicks, devMap, input.Names)
	}

	result := collectDevResults(devMap)

	sort.Slice(result, func(i, j int) bool {
		return result[i].Commits > result[j].Commits
	})

	return result
}

func processTickDevs(tick int, devTicks map[int]*DevTick, devMap map[int]*DeveloperData, names []string) {
	for devID, dt := range devTicks {
		dev := getOrCreateDev(devID, tick, devMap, names)
		updateDevStats(dev, dt, tick)
	}
}

func getOrCreateDev(devID, tick int, devMap map[int]*DeveloperData, names []string) *DeveloperData {
	dev := devMap[devID]
	if dev == nil {
		dev = &DeveloperData{
			ID:        devID,
			Name:      devName(devID, names),
			Languages: make(map[string]pkgplumbing.LineStats),
			FirstTick: tick,
			LastTick:  tick,
		}
		devMap[devID] = dev
	}

	return dev
}

func updateDevStats(dev *DeveloperData, dt *DevTick, tick int) {
	dev.Commits += dt.Commits
	dev.Added += dt.Added
	dev.Removed += dt.Removed
	dev.Changed += dt.Changed
	dev.ActiveTicks++

	if tick < dev.FirstTick {
		dev.FirstTick = tick
	}

	if tick > dev.LastTick {
		dev.LastTick = tick
	}

	mergeLanguageStats(dev.Languages, dt.Languages)
}

func mergeLanguageStats(target, source map[string]pkgplumbing.LineStats) {
	for lang, stats := range source {
		ls := target[lang]
		target[lang] = pkgplumbing.LineStats{
			Added:   ls.Added + stats.Added,
			Removed: ls.Removed + stats.Removed,
			Changed: ls.Changed + stats.Changed,
		}
	}
}

func collectDevResults(devMap map[int]*DeveloperData) []DeveloperData {
	result := make([]DeveloperData, 0, len(devMap))

	for _, dev := range devMap {
		dev.NetLines = dev.Added - dev.Removed
		result = append(result, *dev)
	}

	return result
}

// LanguagesMetric computes per-language statistics.
type LanguagesMetric struct {
	metrics.MetricMeta
}

// NewLanguagesMetric creates the languages metric.
func NewLanguagesMetric() *LanguagesMetric {
	return &LanguagesMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "languages",
			MetricDisplayName: "Language Statistics",
			MetricDescription: "Per-language contribution statistics showing total lines and contributor breakdown. " +
				"Languages are sorted by total lines added.",
			MetricType: "list",
		},
	}
}

// Compute calculates language statistics from developer data.
func (m *LanguagesMetric) Compute(developers []DeveloperData) []LanguageData {
	langMap := make(map[string]*LanguageData)

	for _, dev := range developers {
		for lang, stats := range dev.Languages {
			if lang == "" {
				lang = "Other"
			}

			ld := langMap[lang]
			if ld == nil {
				ld = &LanguageData{
					Name:         lang,
					Contributors: make(map[int]int),
				}
				langMap[lang] = ld
			}

			ld.TotalLines += stats.Added
			contribution := stats.Added + stats.Removed
			ld.TotalContribution += contribution
			ld.Contributors[dev.ID] += contribution
		}
	}

	result := make([]LanguageData, 0, len(langMap))
	for _, ld := range langMap {
		result = append(result, *ld)
	}

	sort.Slice(result, func(i, j int) bool {
		return result[i].TotalLines > result[j].TotalLines
	})

	return result
}

// BusFactorInput is the input for bus factor computation.
type BusFactorInput struct {
	Languages []LanguageData
	Names     []string
}

// BusFactorMetric computes knowledge concentration risk per language.
type BusFactorMetric struct {
	metrics.MetricMeta
}

// NewBusFactorMetric creates the bus factor metric.
func NewBusFactorMetric() *BusFactorMetric {
	return &BusFactorMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "bus_factor",
			MetricDisplayName: "Bus Factor Risk",
			MetricDescription: "Knowledge concentration risk per language using the CHAOSS Contributor Absence Factor " +
				"methodology. Bus factor = smallest number of contributors covering 50% of contributions (Added+Removed). " +
				"Risk levels: CRITICAL (>=90% single owner), HIGH (>=80%), MEDIUM (>=60%), LOW (<60%).",
			MetricType: "risk",
		},
	}
}

// Risk thresholds.
const (
	ThresholdCritical = 90.0
	ThresholdHigh     = 80.0
	ThresholdMedium   = 60.0
)

// Risk level constants.
const (
	RiskCritical = "CRITICAL"
	RiskHigh     = "HIGH"
	RiskMedium   = "MEDIUM"
	RiskLow      = "LOW"
)

// Percent multiplier for calculations.
const percentMultiplier = 100

// Risk priority values for sorting.
const (
	riskPriorityCritical = 0
	riskPriorityHigh     = 1
	riskPriorityMedium   = 2
	riskPriorityDefault  = 3
)

// busFactorThreshold is the CHAOSS contribution coverage threshold (50%).
const busFactorThreshold = 0.5

// Compute calculates bus factor risk from language data.
// Contributors map values represent total contribution (Added+Removed).
func (m *BusFactorMetric) Compute(input BusFactorInput) []BusFactorData {
	result := make([]BusFactorData, 0, len(input.Languages))

	for _, ld := range input.Languages {
		if ld.TotalContribution == 0 {
			continue
		}

		type contrib struct {
			id    int
			lines int
		}

		contribs := make([]contrib, 0, len(ld.Contributors))

		for devID, lines := range ld.Contributors {
			contribs = append(contribs, contrib{devID, lines})
		}

		sort.Slice(contribs, func(i, j int) bool {
			return contribs[i].lines > contribs[j].lines
		})

		sortedAmounts := make([]int, len(contribs))
		for i, c := range contribs {
			sortedAmounts[i] = c.lines
		}

		bf := BusFactorData{
			Language:          ld.Name,
			TotalContributors: len(contribs),
			BusFactor:         computeBusFactorFromSorted(sortedAmounts, ld.TotalContribution),
		}

		if len(contribs) > 0 {
			bf.PrimaryDevID = contribs[0].id
			bf.PrimaryDevName = devName(contribs[0].id, input.Names)
			bf.PrimaryPct = float64(contribs[0].lines) / float64(ld.TotalContribution) * percentMultiplier
		}

		if len(contribs) > 1 {
			bf.SecondaryDevID = contribs[1].id
			bf.SecondaryDevName = devName(contribs[1].id, input.Names)
			bf.SecondaryPct = float64(contribs[1].lines) / float64(ld.TotalContribution) * percentMultiplier
		}

		switch {
		case bf.PrimaryPct >= ThresholdCritical:
			bf.RiskLevel = RiskCritical
		case bf.PrimaryPct >= ThresholdHigh:
			bf.RiskLevel = RiskHigh
		case bf.PrimaryPct >= ThresholdMedium:
			bf.RiskLevel = RiskMedium
		default:
			bf.RiskLevel = RiskLow
		}

		result = append(result, bf)
	}

	sort.Slice(result, func(i, j int) bool {
		return riskPriority(result[i].RiskLevel) < riskPriority(result[j].RiskLevel)
	})

	return result
}

// computeBusFactorFromSorted returns the smallest number of contributors
// who together account for at least 50% of total contributions.
// This follows the CHAOSS Contributor Absence Factor methodology.
// sortedContribs must be sorted descending by contribution amount.
func computeBusFactorFromSorted(sortedContribs []int, total int) int {
	if total == 0 || len(sortedContribs) == 0 {
		return 0
	}

	threshold := float64(total) * busFactorThreshold
	cumulative := 0

	for i, amount := range sortedContribs {
		cumulative += amount

		if float64(cumulative) >= threshold {
			return i + 1
		}
	}

	return len(sortedContribs)
}

// ActivityMetric computes time-series commit activity.
type ActivityMetric struct {
	metrics.MetricMeta
}

// NewActivityMetric creates the activity metric.
func NewActivityMetric() *ActivityMetric {
	return &ActivityMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "activity",
			MetricDisplayName: "Commit Activity",
			MetricDescription: "Time-series of commit activity per tick, broken down by developer. " +
				"Shows contribution velocity over the analysis period.",
			MetricType: "time_series",
		},
	}
}

// Compute calculates activity time series from tick data.
func (m *ActivityMetric) Compute(input *TickData) []ActivityData {
	tickKeys := sortedKeys(input.Ticks)
	result := make([]ActivityData, len(tickKeys))

	for i, tick := range tickKeys {
		ad := ActivityData{
			Tick:        tick,
			ByDeveloper: make(map[int]int),
		}

		for devID, dt := range input.Ticks[tick] {
			ad.ByDeveloper[devID] = dt.Commits
			ad.TotalCommits += dt.Commits
		}

		result[i] = ad
	}

	return result
}

// ChurnMetric computes time-series code churn.
type ChurnMetric struct {
	metrics.MetricMeta
}

// NewChurnMetric creates the churn metric.
func NewChurnMetric() *ChurnMetric {
	return &ChurnMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "churn",
			MetricDisplayName: "Code Churn",
			MetricDescription: "Time-series of lines added and removed per tick (line velocity). " +
				"Shows the volume of code changes over time. High values may indicate " +
				"refactoring, feature development, or instability.",
			MetricType: "time_series",
		},
	}
}

// Compute calculates churn time series from tick data.
func (m *ChurnMetric) Compute(input *TickData) []ChurnData {
	tickKeys := sortedKeys(input.Ticks)
	result := make([]ChurnData, len(tickKeys))

	for i, tick := range tickKeys {
		cd := ChurnData{Tick: tick}

		for _, dt := range input.Ticks[tick] {
			cd.Added += dt.Added
			cd.Removed += dt.Removed
		}

		cd.Net = cd.Added - cd.Removed

		result[i] = cd
	}

	return result
}

// AggregateInput is the input for aggregate computation.
type AggregateInput struct {
	Developers []DeveloperData
	Languages  []LanguageData
	Ticks      map[int]map[int]*DevTick
	TickSize   time.Duration
}

// AggregateMetric computes summary statistics.
type AggregateMetric struct {
	metrics.MetricMeta
}

// NewAggregateMetric creates the aggregate metric.
func NewAggregateMetric() *AggregateMetric {
	return &AggregateMetric{
		MetricMeta: metrics.MetricMeta{
			MetricName:        "aggregate",
			MetricDisplayName: "Summary Statistics",
			MetricDescription: "Aggregate statistics across all developers and the analysis period. " +
				"Active developers are those with commits in the last 90 days (or recent 30% as fallback). " +
				"Project bus factor follows the CHAOSS Contributor Absence Factor methodology.",
			MetricType: "aggregate",
		},
	}
}

// ActiveThresholdRatio defines what portion of analysis period counts as "recent".
// Used as fallback when TickSize is not available.
const ActiveThresholdRatio = 0.7

// DefaultActiveDays is the time-based threshold for considering a developer "active".
// Developers with commits in the last 90 days are counted as active.
const DefaultActiveDays = 90

// Compute calculates aggregate statistics.
func (m *AggregateMetric) Compute(input AggregateInput) AggregateData {
	agg := AggregateData{
		TotalDevelopers: len(input.Developers),
		TotalLanguages:  len(input.Languages),
	}

	totalSketch := buildTotalDevSketch(input.Developers)

	for _, d := range input.Developers {
		agg.TotalCommits += d.Commits
		agg.TotalLinesAdded += d.Added
		agg.TotalLinesRemoved += d.Removed
	}

	if totalSketch != nil {
		agg.EstimatedTotalDevelopers = totalSketch.Count()
	}

	if len(input.Ticks) > 0 {
		tickKeys := sortedKeys(input.Ticks)
		maxTick := tickKeys[len(tickKeys)-1]
		agg.AnalysisPeriodTicks = maxTick

		recentThreshold := computeActiveThreshold(maxTick, input.TickSize)
		activeDevs := make(map[int]bool)
		activeSketch := buildActiveDevSketch(input.Ticks, recentThreshold)

		for tick, devTicks := range input.Ticks {
			if tick >= recentThreshold {
				for devID := range devTicks {
					activeDevs[devID] = true
				}
			}
		}

		agg.ActiveDevelopers = len(activeDevs)

		if activeSketch != nil {
			agg.EstimatedActiveDevelopers = activeSketch.Count()
		}
	}

	agg.ProjectBusFactor = computeProjectBusFactor(input.Developers)

	return agg
}

// buildTotalDevSketch creates an HLL sketch from all developer IDs in the input.
func buildTotalDevSketch(developers []DeveloperData) *hll.Sketch {
	if len(developers) == 0 {
		return nil
	}

	sketch, err := hll.New(hllPrecision)
	if err != nil {
		return nil
	}

	for i := range developers {
		sketch.Add(devIDBytes(developers[i].ID))
	}

	return sketch
}

// buildActiveDevSketch creates an HLL sketch from developer IDs in ticks at or above the threshold.
func buildActiveDevSketch(ticks map[int]map[int]*DevTick, threshold int) *hll.Sketch {
	sketch, err := hll.New(hllPrecision)
	if err != nil {
		return nil
	}

	for tick, devTicks := range ticks {
		if tick >= threshold {
			for devID := range devTicks {
				sketch.Add(devIDBytes(devID))
			}
		}
	}

	return sketch
}

// computeActiveThreshold returns the tick index threshold for "active" developers.
// When TickSize is known, uses time-based calculation (last 90 days).
// Otherwise falls back to ratio-based (last 30% of analysis period).
func computeActiveThreshold(maxTick int, tickSize time.Duration) int {
	if tickSize > 0 {
		activeDuration := time.Duration(DefaultActiveDays) * defaultTickHours * time.Hour
		ticksForActive := int(activeDuration / tickSize)
		threshold := maxTick - ticksForActive

		if threshold < 0 {
			return 0
		}

		return threshold
	}

	return int(float64(maxTick) * ActiveThresholdRatio)
}

// computeProjectBusFactor computes the CHAOSS Contributor Absence Factor
// across the entire project: the smallest number of developers responsible
// for 50% of all contributions (Added+Removed).
func computeProjectBusFactor(developers []DeveloperData) int {
	if len(developers) == 0 {
		return 0
	}

	type devContrib struct {
		contribution int
	}

	contribs := make([]devContrib, len(developers))
	total := 0

	for i, dev := range developers {
		c := dev.Added + dev.Removed
		contribs[i] = devContrib{c}
		total += c
	}

	sort.Slice(contribs, func(i, j int) bool {
		return contribs[i].contribution > contribs[j].contribution
	})

	sortedAmounts := make([]int, len(contribs))
	for i, c := range contribs {
		sortedAmounts[i] = c.contribution
	}

	return computeBusFactorFromSorted(sortedAmounts, total)
}

// ComputedMetrics holds all computed metric results for the devs analyzer.
// This is populated by running each metric's Compute method.
type ComputedMetrics struct {
	Ticks       map[int]map[int]*DevTick `json:"-"          yaml:"-"`
	TickSize    time.Duration            `json:"-"          yaml:"-"`
	Aggregate   AggregateData            `json:"aggregate"  yaml:"aggregate"`
	Developers  []DeveloperData          `json:"developers" yaml:"developers"`
	Languages   []LanguageData           `json:"languages"  yaml:"languages"`
	BusFactor   []BusFactorData          `json:"busfactor"  yaml:"busfactor"`
	Activity    []ActivityData           `json:"activity"   yaml:"activity"`
	Churn       []ChurnData              `json:"churn"      yaml:"churn"`
	metricNames []string                 `json:"-"          yaml:"-"`
}

// ComputeAllMetrics runs all devs metrics and returns the results.
func ComputeAllMetrics(report analyze.Report) (*ComputedMetrics, error) {
	input, err := ParseTickData(report)
	if err != nil {
		return nil, err
	}

	// Compute in dependency order.
	devMetric := NewDevelopersMetric()
	developers := devMetric.Compute(input)

	langMetric := NewLanguagesMetric()
	languages := langMetric.Compute(developers)

	busMetric := NewBusFactorMetric()
	busFactor := busMetric.Compute(BusFactorInput{Languages: languages, Names: input.Names})

	actMetric := NewActivityMetric()
	activity := actMetric.Compute(input)

	churnMetric := NewChurnMetric()
	churn := churnMetric.Compute(input)

	aggMetric := NewAggregateMetric()
	aggregate := aggMetric.Compute(AggregateInput{
		Developers: developers,
		Languages:  languages,
		Ticks:      input.Ticks,
		TickSize:   input.TickSize,
	})

	return &ComputedMetrics{
		Ticks:      input.Ticks,
		TickSize:   input.TickSize,
		Developers: developers,
		Languages:  languages,
		BusFactor:  busFactor,
		Activity:   activity,
		Churn:      churn,
		Aggregate:  aggregate,
		metricNames: buildMetricRegistry(devMetric, langMetric, busMetric,
			actMetric, churnMetric, aggMetric),
	}, nil
}

// buildMetricRegistry creates a registry of all devs metrics and returns
// a summary of each metric's catalog entry (name, display, type).
func buildMetricRegistry(
	devMetric *DevelopersMetric,
	langMetric *LanguagesMetric,
	busMetric *BusFactorMetric,
	actMetric *ActivityMetric,
	churnMetric *ChurnMetric,
	aggMetric *AggregateMetric,
) []string {
	reg := metrics.NewRegistry()
	metrics.Register(reg, devMetric)
	metrics.Register(reg, langMetric)
	metrics.Register(reg, busMetric)
	metrics.Register(reg, actMetric)
	metrics.Register(reg, churnMetric)
	metrics.Register(reg, aggMetric)

	names := reg.Names()

	// Verify all metrics are retrievable.
	for _, n := range names {
		_, _ = reg.Get(n)
	}

	// Exercise metadata accessors on the first metric so that
	// MetricMeta.DisplayName/Description/Type remain reachable.
	_ = devMetric.DisplayName()
	_ = devMetric.Description()
	_ = devMetric.Type()

	return names
}

const analyzerNameDevs = "devs"

// AnalyzerName returns the analyzer identifier.
func (m *ComputedMetrics) AnalyzerName() string {
	return analyzerNameDevs
}

// ToJSON returns the metrics in JSON-serializable format.
func (m *ComputedMetrics) ToJSON() any {
	return m
}

// ToYAML returns the metrics in YAML-serializable format.
func (m *ComputedMetrics) ToYAML() any {
	return m
}

const defaultTickHours = 24

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

func devName(id int, names []string) string {
	if id == identity.AuthorMissing {
		return identity.AuthorMissingName
	}

	if id >= 0 && id < len(names) {
		return names[id]
	}

	return fmt.Sprintf("dev_%d", id)
}

func sortedKeys(m map[int]map[int]*DevTick) []int {
	keys := make([]int, 0, len(m))

	for k := range m {
		keys = append(keys, k)
	}

	sort.Ints(keys)

	return keys
}
