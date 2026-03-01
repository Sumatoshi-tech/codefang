// Package devs provides devs functionality.
package devs

import (
	"context"
	"io"
	"maps"
	"time"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/internal/identity"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

// CommitDevData holds aggregate dev stats for a single commit.
type CommitDevData struct {
	Commits   int                              `json:"commits"`
	Added     int                              `json:"lines_added"`
	Removed   int                              `json:"lines_removed"`
	Changed   int                              `json:"lines_changed"`
	AuthorID  int                              `json:"author_id"`
	Languages map[string]pkgplumbing.LineStats `json:"languages,omitempty"`
}

// DevTick is the statistics for a development tick and a particular developer.
type DevTick struct {
	pkgplumbing.LineStats

	Languages map[string]pkgplumbing.LineStats
	Commits   int
}

// TickDevData is the per-tick aggregated payload stored in analyze.TICK.Data.
// It groups all per-commit developer data within one time bucket.
type TickDevData struct {
	// DevData maps commit hash hex to per-commit developer statistics.
	DevData map[string]*CommitDevData
}

// Configuration option keys for the devs analyzer.
const (
	ConfigDevsConsiderEmptyCommits = "Devs.ConsiderEmptyCommits"
	ConfigDevsAnonymize            = "Devs.Anonymize"

	defaultHoursPerDay = 24
)

// Analyzer calculates per-developer line statistics across commit history.
type Analyzer struct {
	*analyze.BaseHistoryAnalyzer[*ComputedMetrics]

	Identity             *plumbing.IdentityDetector
	TreeDiff             *plumbing.TreeDiffAnalyzer
	Ticks                *plumbing.TicksSinceStart
	Languages            *plumbing.LanguagesDetectionAnalyzer
	LineStats            *plumbing.LinesStatsCalculator
	commitsByTick        map[int][]gitlib.Hash
	merges               *analyze.MergeTracker
	reversedPeopleDict   []string
	tickSize             time.Duration
	ConsiderEmptyCommits bool
	Anonymize            bool
}

// NewAnalyzer creates a new devs analyzer.
func NewAnalyzer() *Analyzer {
	a := &Analyzer{}
	a.BaseHistoryAnalyzer = &analyze.BaseHistoryAnalyzer[*ComputedMetrics]{
		Desc: analyze.Descriptor{
			ID:          "history/devs",
			Mode:        analyze.ModeHistory,
			Description: "Calculates the number of commits, added, removed and changed lines per developer through time.",
		},
		Sequential: true,
		ConfigOptions: []pipeline.ConfigurationOption{
			{
				Name:        ConfigDevsConsiderEmptyCommits,
				Description: "Take into account empty commits such as trivial merges.",
				Flag:        "empty-commits",
				Type:        pipeline.BoolConfigurationOption,
				Default:     false,
			},
			{
				Name:        ConfigDevsAnonymize,
				Description: "Anonymize developer names in output (e.g., Developer-A, Developer-B).",
				Flag:        "anonymize",
				Type:        pipeline.BoolConfigurationOption,
				Default:     false,
			},
		},
		ComputeMetricsFn: computeMetricsSafe,
		AggregatorFn:     newAggregator,
	}

	a.TicksToReportFn = func(ctx context.Context, ticks []analyze.TICK) analyze.Report {
		return ticksToReport(ctx, ticks, a.commitsByTick, a.getReversedPeopleDict(), a.tickSize, a.Anonymize)
	}

	return a
}

func (a *Analyzer) getReversedPeopleDict() []string {
	if a.Identity != nil && len(a.Identity.ReversedPeopleDict) > 0 {
		return a.Identity.ReversedPeopleDict
	}

	return a.reversedPeopleDict
}

func computeMetricsSafe(report analyze.Report) (*ComputedMetrics, error) {
	if len(report) == 0 {
		return &ComputedMetrics{}, nil
	}

	return ComputeAllMetrics(report)
}

// Configure configures the analyzer with the given facts.
func (a *Analyzer) Configure(facts map[string]any) error {
	if val, exists := facts[ConfigDevsConsiderEmptyCommits].(bool); exists {
		a.ConsiderEmptyCommits = val
	}

	if val, exists := facts[ConfigDevsAnonymize].(bool); exists {
		a.Anonymize = val
	}

	if val, exists := facts[identity.FactIdentityDetectorReversedPeopleDict].([]string); exists {
		a.reversedPeopleDict = val
	}

	if val, exists := facts[pkgplumbing.FactTickSize].(time.Duration); exists {
		a.tickSize = val
	}

	if val, exists := facts[pkgplumbing.FactCommitsByTick].(map[int][]gitlib.Hash); exists {
		a.commitsByTick = val
	}

	return nil
}

// Initialize prepares the analyzer for processing commits.
func (a *Analyzer) Initialize(_ *gitlib.Repository) error {
	RegisterDevPlotSections()

	if a.tickSize == 0 {
		a.tickSize = defaultHoursPerDay * time.Hour // Default fallback.
	}

	a.merges = analyze.NewMergeTracker()

	return nil
}

// Consume processes a single commit and returns a TC with per-commit dev stats.
func (a *Analyzer) Consume(_ context.Context, ac *analyze.Context) (analyze.TC, error) {
	commit := ac.Commit
	commitHash := commit.Hash()

	if commit.NumParents() > 1 {
		if a.merges.SeenOrAdd(commitHash) {
			return analyze.TC{}, nil
		}
	}

	treeDiff := a.TreeDiff.Changes
	if len(treeDiff) == 0 && !a.ConsiderEmptyCommits {
		return analyze.TC{}, nil
	}

	cdd := &CommitDevData{
		Commits:   1,
		AuthorID:  a.Identity.AuthorID,
		Languages: make(map[string]pkgplumbing.LineStats),
	}

	if !ac.IsMerge {
		a.accumulateLineStats(cdd)
	}

	return analyze.TC{
		Data:       cdd,
		CommitHash: commitHash,
	}, nil
}

func (a *Analyzer) accumulateLineStats(cdd *CommitDevData) {
	langs := a.Languages.Languages()

	for changeEntry, stats := range a.LineStats.LineStats {
		cdd.Added += stats.Added
		cdd.Removed += stats.Removed
		cdd.Changed += stats.Changed

		lang := langs[changeEntry.Hash]
		cddLangStats := cdd.Languages[lang]
		cdd.Languages[lang] = pkgplumbing.LineStats{
			Added:   cddLangStats.Added + stats.Added,
			Removed: cddLangStats.Removed + stats.Removed,
			Changed: cddLangStats.Changed + stats.Changed,
		}
	}
}

// Fork creates independent copies of the analyzer for parallel processing.
func (a *Analyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)

	for i := range n {
		clone := *a

		clone.Identity = &plumbing.IdentityDetector{}
		clone.TreeDiff = &plumbing.TreeDiffAnalyzer{}
		clone.Ticks = &plumbing.TicksSinceStart{}
		clone.Languages = &plumbing.LanguagesDetectionAnalyzer{}
		clone.LineStats = &plumbing.LinesStatsCalculator{}

		res[i] = &clone
	}

	return res
}

// NewAggregator creates an aggregator for this analyzer.
func (a *Analyzer) NewAggregator(opts analyze.AggregatorOptions) analyze.Aggregator {
	return a.AggregatorFn(opts)
}

// Merge is a no-op.
func (a *Analyzer) Merge(_ []analyze.HistoryAnalyzer) {}

// SnapshotPlumbing captures the current plumbing state.
func (a *Analyzer) SnapshotPlumbing() analyze.PlumbingSnapshot {
	return plumbing.Snapshot{
		Changes:   a.TreeDiff.Changes,
		Tick:      a.Ticks.Tick,
		AuthorID:  a.Identity.AuthorID,
		Languages: a.Languages.Languages(),
		LineStats: a.LineStats.LineStats,
	}
}

// ApplySnapshot restores plumbing state from a snapshot.
func (a *Analyzer) ApplySnapshot(snap analyze.PlumbingSnapshot) {
	snapshot, ok := snap.(plumbing.Snapshot)
	if !ok {
		return
	}

	a.TreeDiff.Changes = snapshot.Changes
	a.Ticks.Tick = snapshot.Tick
	a.Identity.AuthorID = snapshot.AuthorID
	a.Languages.SetLanguages(snapshot.Languages)
	a.LineStats.LineStats = snapshot.LineStats
}

// ReleaseSnapshot is a no-op for devs.
func (a *Analyzer) ReleaseSnapshot(_ analyze.PlumbingSnapshot) {}

// Serialize writes the analysis result to the given writer.
func (a *Analyzer) Serialize(result analyze.Report, format string, writer io.Writer) error {
	if format == analyze.FormatText {
		return a.generateText(result, writer)
	}

	if format == analyze.FormatPlot {
		return GenerateDashboard(result, writer)
	}

	if a.BaseHistoryAnalyzer != nil {
		return a.BaseHistoryAnalyzer.Serialize(result, format, writer)
	}

	return (&analyze.BaseHistoryAnalyzer[*ComputedMetrics]{
		ComputeMetricsFn: computeMetricsSafe,
	}).Serialize(result, format, writer)
}

// SerializeTICKs converts aggregated TICKs into the final report and serializes it.
func (a *Analyzer) SerializeTICKs(ticks []analyze.TICK, format string, writer io.Writer) error {
	if format == analyze.FormatText || format == analyze.FormatPlot {
		report, err := a.ReportFromTICKs(context.Background(), ticks)
		if err != nil {
			return err
		}

		if format == analyze.FormatPlot {
			return GenerateDashboard(report, writer)
		}

		return a.generateText(report, writer)
	}

	if a.BaseHistoryAnalyzer != nil {
		return a.BaseHistoryAnalyzer.SerializeTICKs(ticks, format, writer)
	}

	return (&analyze.BaseHistoryAnalyzer[*ComputedMetrics]{
		ComputeMetricsFn: computeMetricsSafe,
		TicksToReportFn: func(ctx context.Context, t []analyze.TICK) analyze.Report {
			return ticksToReport(ctx, t, a.commitsByTick, a.getReversedPeopleDict(), a.tickSize, a.Anonymize)
		},
	}).SerializeTICKs(ticks, format, writer)
}

// ReportFromTICKs converts aggregated TICKs into a Report.
func (a *Analyzer) ReportFromTICKs(ctx context.Context, ticks []analyze.TICK) (analyze.Report, error) {
	return a.TicksToReportFn(ctx, ticks), nil
}

// ExtractCommitTimeSeries extracts per-commit dev stats from a finalized report.
func (a *Analyzer) ExtractCommitTimeSeries(report analyze.Report) map[string]any {
	commitData, ok := report["CommitDevData"].(map[string]*CommitDevData)
	if !ok || len(commitData) == 0 {
		return nil
	}

	result := make(map[string]any, len(commitData))

	for hash, cdd := range commitData {
		entry := map[string]any{
			"commits":       cdd.Commits,
			"lines_added":   cdd.Added,
			"lines_removed": cdd.Removed,
			"lines_changed": cdd.Changed,
			"net_change":    cdd.Added - cdd.Removed,
			"author_id":     cdd.AuthorID,
		}
		if len(cdd.Languages) > 0 {
			entry["languages"] = cdd.Languages
		}

		result[hash] = entry
	}

	return result
}

// Extract properties for GenericAggregator.

const (
	commitEntryOverhead = 128 // map entry + struct overhead per commit.
	bytesPerLangEntry   = 48  // language map entry in CommitDevData.
)

func extractTC(tc analyze.TC, byTick map[int]*TickDevData) error {
	cdd, isCDD := tc.Data.(*CommitDevData)
	if !isCDD || cdd == nil {
		return nil
	}

	state, ok := byTick[tc.Tick]
	if !ok || state == nil {
		state = &TickDevData{DevData: make(map[string]*CommitDevData)}
		byTick[tc.Tick] = state
	}

	state.DevData[tc.CommitHash.String()] = cdd

	return nil
}

func mergeState(existing, incoming *TickDevData) *TickDevData {
	if existing == nil {
		return incoming
	}

	if incoming == nil {
		return existing
	}

	if existing.DevData == nil {
		existing.DevData = make(map[string]*CommitDevData)
	}

	for k, v := range incoming.DevData {
		if ext, ok := existing.DevData[k]; ok {
			existing.DevData[k] = mergeCommitDevData(ext, v)
		} else {
			existing.DevData[k] = v
		}
	}

	return existing
}

func sizeState(state *TickDevData) int64 {
	if state == nil || state.DevData == nil {
		return 0
	}

	var size int64

	for _, cdd := range state.DevData {
		size += commitEntryOverhead
		size += int64(len(cdd.Languages)) * bytesPerLangEntry
	}

	return size
}

func buildTick(tick int, state *TickDevData) (analyze.TICK, error) {
	if state == nil || len(state.DevData) == 0 {
		return analyze.TICK{Tick: tick}, nil
	}

	return analyze.TICK{
		Tick: tick,
		Data: state,
	}, nil
}

func newAggregator(opts analyze.AggregatorOptions) analyze.Aggregator {
	agg := analyze.NewGenericAggregator[*TickDevData, *TickDevData](
		opts,
		extractTC,
		mergeState,
		sizeState,
		buildTick,
	)
	agg.DrainCommitDataFn = drainDevCommitData

	return agg
}

func drainDevCommitData(state *TickDevData) (stats map[string]any, tickHashes map[int][]gitlib.Hash) {
	if state == nil || len(state.DevData) == 0 {
		return nil, nil
	}

	result := make(map[string]any, len(state.DevData))
	for hash, cdd := range state.DevData {
		entry := map[string]any{
			"commits":       cdd.Commits,
			"lines_added":   cdd.Added,
			"lines_removed": cdd.Removed,
			"lines_changed": cdd.Changed,
			"net_change":    cdd.Added - cdd.Removed,
			"author_id":     cdd.AuthorID,
		}
		if len(cdd.Languages) > 0 {
			entry["languages"] = cdd.Languages
		}

		result[hash] = entry
	}

	state.DevData = make(map[string]*CommitDevData)

	return result, nil
}

func mergeCommitDevData(existing, incoming *CommitDevData) *CommitDevData {
	existing.Commits += incoming.Commits
	existing.Added += incoming.Added
	existing.Removed += incoming.Removed
	existing.Changed += incoming.Changed

	if existing.Languages == nil {
		existing.Languages = make(map[string]pkgplumbing.LineStats)
	}

	for lang, stats := range incoming.Languages {
		ls := existing.Languages[lang]
		existing.Languages[lang] = pkgplumbing.LineStats{
			Added:   ls.Added + stats.Added,
			Removed: ls.Removed + stats.Removed,
			Changed: ls.Changed + stats.Changed,
		}
	}

	return existing
}

// ticksToReport converts aggregated TICKs into the analyze.Report format.
func ticksToReport(
	_ context.Context,
	ticks []analyze.TICK,
	commitsByTick map[int][]gitlib.Hash,
	names []string,
	tickSize time.Duration,
	anonymize bool,
) analyze.Report {
	if anonymize {
		names = anonymizeNames(names)
	}

	collected := make(map[string]*CommitDevData)

	for _, tick := range ticks {
		td, ok := tick.Data.(*TickDevData)
		if !ok || td == nil {
			continue
		}

		maps.Copy(collected, td.DevData)
	}

	return analyze.Report{
		"CommitDevData":      collected,
		"CommitsByTick":      commitsByTick,
		"ReversedPeopleDict": names,
		"TickSize":           tickSize,
	}
}
