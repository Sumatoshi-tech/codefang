package imports

import (
	"context"
	"maps"
	"time"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/internal/identity"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

const (
	defaultTickHours    = 24
	estimatedImportSize = 24
)

// Map maps file paths to their import lists.
// author -> lang -> import -> tick -> count.
type Map = map[int]map[string]map[string]map[int]int64

// ImportEntry represents a single import extracted from a commit.
// It carries the language and import path for aggregation.
type ImportEntry struct {
	Lang   string
	Import string
}

// ImportsCommitSummary holds per-commit summary data for timeseries output.
type ImportsCommitSummary struct { //nolint:revive // used across packages.
	ImportCount int            `json:"import_count"`
	Languages   map[string]int `json:"languages"`
}

// TickData is the per-tick aggregated payload stored in analyze.TICK.Data.
// It holds the accumulated 4-level imports map for the tick.
type TickData struct {
	Imports     Map
	CommitStats map[string]*ImportsCommitSummary
}

// tickAccumulator holds the in-memory state during aggregation for a single tick.
type tickAccumulator struct {
	imports     Map
	commitStats map[string]*ImportsCommitSummary
}

// HistoryAnalyzer tracks import usage across commit history.
// It consumes pre-parsed UAST trees from the framework's UAST pipeline
// rather than maintaining its own tree-sitter parser.
type HistoryAnalyzer struct {
	*analyze.BaseHistoryAnalyzer[*ComputedMetrics]

	UAST               *plumbing.UASTChangesAnalyzer
	Identity           *plumbing.IdentityDetector
	Ticks              *plumbing.TicksSinceStart
	reversedPeopleDict []string
	TickSize           time.Duration
}

// NewHistoryAnalyzer creates a new HistoryAnalyzer.
func NewHistoryAnalyzer() *HistoryAnalyzer {
	a := &HistoryAnalyzer{
		TickSize: defaultTickHours * time.Hour,
	}

	a.BaseHistoryAnalyzer = &analyze.BaseHistoryAnalyzer[*ComputedMetrics]{
		Desc: analyze.Descriptor{
			ID:          "history/imports",
			Description: "Extracts imports from changed files and tracks usage per author.",
			Mode:        analyze.ModeHistory,
		},
		Sequential: false,
		ComputeMetricsFn: func(report analyze.Report) (*ComputedMetrics, error) {
			if len(report) == 0 {
				return &ComputedMetrics{}, nil
			}

			return ComputeAllMetrics(report)
		},
		AggregatorFn: func(opts analyze.AggregatorOptions) analyze.Aggregator {
			agg := analyze.NewGenericAggregator[*tickAccumulator, *TickData](
				opts,
				a.extractTC,
				a.mergeState,
				a.sizeState,
				a.buildTick,
			)
			agg.DrainCommitDataFn = drainImportsCommitData

			return agg
		},
		TicksToReportFn: func(ctx context.Context, ticks []analyze.TICK) analyze.Report {
			return ticksToReport(ctx, ticks, a.reversedPeopleDict, a.TickSize)
		},
	}

	return a
}

// Name returns the name of the analyzer.
func (h *HistoryAnalyzer) Name() string {
	return "ImportsPerDeveloper"
}

// Flag returns the CLI flag for the analyzer.
func (h *HistoryAnalyzer) Flag() string {
	return "imports-per-dev"
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (h *HistoryAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return nil
}

// Configure sets up the analyzer with the provided facts.
func (h *HistoryAnalyzer) Configure(facts map[string]any) error {
	if val, exists := facts[identity.FactIdentityDetectorReversedPeopleDict].([]string); exists {
		h.reversedPeopleDict = val
	}

	if val, exists := facts[pkgplumbing.FactTickSize].(time.Duration); exists {
		h.TickSize = val
	}

	return nil
}

// Initialize prepares the analyzer for processing commits.
func (h *HistoryAnalyzer) Initialize(_ *gitlib.Repository) error {
	if h.TickSize == 0 {
		h.TickSize = time.Hour * defaultTickHours
	}

	return nil
}

// NeedsUAST returns true to enable the framework's UAST pipeline.
func (h *HistoryAnalyzer) NeedsUAST() bool { return true }

// Consume processes a single commit using pre-parsed UAST trees from the
// framework's pipeline.
func (h *HistoryAnalyzer) Consume(ctx context.Context, ac *analyze.Context) (analyze.TC, error) {
	changesList := h.UAST.Changes(ctx)

	var entries []ImportEntry

	for _, change := range changesList {
		// Only extract imports from the "after" version (Insert or Modify).
		if change.After == nil {
			continue
		}

		imports := extractImportsFromUAST(change.After)
		if len(imports) == 0 {
			continue
		}

		lang := h.UAST.GetLanguage(change.Change.To.Name)
		if lang == "" {
			lang = "uast"
		}

		for _, imp := range imports {
			entries = append(entries, ImportEntry{
				Lang:   lang,
				Import: imp,
			})
		}
	}

	tc := analyze.TC{
		Tick:       h.Ticks.Tick,
		CommitHash: ac.Commit.Hash(),
	}

	if len(entries) > 0 {
		tc.Data = map[string]any{
			"entries":  entries,
			"authorID": h.Identity.AuthorID,
		}
	}

	return tc, nil
}

func (h *HistoryAnalyzer) extractTC(tc analyze.TC, byTick map[int]*tickAccumulator) error {
	if tc.Data == nil {
		return nil
	}

	data, ok := tc.Data.(map[string]any)
	if !ok {
		return nil
	}

	entries, ok := data["entries"].([]ImportEntry)
	if !ok || len(entries) == 0 {
		return nil
	}

	authorID, ok := data["authorID"].(int)
	if !ok {
		return nil
	}

	acc, exists := byTick[tc.Tick]
	if !exists {
		acc = &tickAccumulator{
			imports:     make(Map),
			commitStats: make(map[string]*ImportsCommitSummary),
		}
		byTick[tc.Tick] = acc
	}

	addEntriesToMap(acc.imports, entries, authorID, tc.Tick)

	if !tc.CommitHash.IsZero() {
		languages := make(map[string]int)
		for _, e := range entries {
			languages[e.Lang]++
		}

		acc.commitStats[tc.CommitHash.String()] = &ImportsCommitSummary{
			ImportCount: len(entries),
			Languages:   languages,
		}
	}

	return nil
}

func (h *HistoryAnalyzer) mergeState(dst, src *tickAccumulator) *tickAccumulator {
	mergeImportMaps(dst.imports, src.imports)

	if dst.commitStats == nil {
		dst.commitStats = make(map[string]*ImportsCommitSummary)
	}

	maps.Copy(dst.commitStats, src.commitStats)

	return dst
}

func (h *HistoryAnalyzer) sizeState(acc *tickAccumulator) int64 {
	size := int64(0)

	for _, langs := range acc.imports {
		for _, imps := range langs {
			for _, ticks := range imps {
				size += int64(len(ticks) * estimatedImportSize)
			}
		}
	}

	return size
}

func (h *HistoryAnalyzer) buildTick(tick int, acc *tickAccumulator) (analyze.TICK, error) {
	return analyze.TICK{
		Tick: tick,
		Data: &TickData{
			Imports:     acc.imports,
			CommitStats: acc.commitStats,
		},
	}, nil
}

// NewAggregator creates an imports Aggregator that collects per-commit entries.
func (h *HistoryAnalyzer) NewAggregator(opts analyze.AggregatorOptions) analyze.Aggregator {
	return h.AggregatorFn(opts)
}

// ReportFromTICKs converts aggregated TICKs into a Report.
func (h *HistoryAnalyzer) ReportFromTICKs(ctx context.Context, ticks []analyze.TICK) (analyze.Report, error) {
	return ticksToReport(ctx, ticks, h.reversedPeopleDict, h.TickSize), nil
}

// ExtractCommitTimeSeries implements analyze.CommitTimeSeriesProvider.
// It extracts per-commit import usage data for the unified timeseries output.
func (h *HistoryAnalyzer) ExtractCommitTimeSeries(report analyze.Report) map[string]any {
	commitStats, ok := report["commit_stats"].(map[string]*ImportsCommitSummary)
	if !ok || len(commitStats) == 0 {
		return nil
	}

	result := make(map[string]any, len(commitStats))

	for hash, cs := range commitStats {
		result[hash] = map[string]any{
			"import_count": cs.ImportCount,
			"languages":    cs.Languages,
		}
	}

	return result
}

func drainImportsCommitData(state *tickAccumulator) (stats map[string]any, tickHashes map[int][]gitlib.Hash) {
	if state == nil || len(state.commitStats) == 0 {
		return nil, nil
	}

	result := make(map[string]any, len(state.commitStats))
	for hash, cs := range state.commitStats {
		result[hash] = map[string]any{
			"import_count": cs.ImportCount,
			"languages":    cs.Languages,
		}
	}

	state.commitStats = make(map[string]*ImportsCommitSummary)

	return result, nil
}

// Helper methods.

func mergeImportMaps(dst, src Map) {
	for auth, srcLangs := range src {
		dstLangs, ok := dst[auth]
		if !ok {
			dstLangs = make(map[string]map[string]map[int]int64)
			dst[auth] = dstLangs
		}

		mergeLangImports(dstLangs, srcLangs)
	}
}

func mergeLangImports(dstLangs, srcLangs map[string]map[string]map[int]int64) {
	for lang, srcImps := range srcLangs {
		dstImps, ok := dstLangs[lang]
		if !ok {
			dstImps = make(map[string]map[int]int64)
			dstLangs[lang] = dstImps
		}

		mergeTicks(dstImps, srcImps)
	}
}

func mergeTicks(dstImps, srcImps map[string]map[int]int64) {
	for imp, srcTicks := range srcImps {
		dstTicks, ok := dstImps[imp]
		if !ok {
			dstTicks = make(map[int]int64)
			dstImps[imp] = dstTicks
		}

		for tick, count := range srcTicks {
			dstTicks[tick] += count
		}
	}
}

func addEntriesToMap(m Map, entries []ImportEntry, authorID, tick int) {
	langs, hasAuthor := m[authorID]
	if !hasAuthor {
		langs = make(map[string]map[string]map[int]int64)
		m[authorID] = langs
	}

	for _, entry := range entries {
		imps, hasLang := langs[entry.Lang]
		if !hasLang {
			imps = make(map[string]map[int]int64)
			langs[entry.Lang] = imps
		}

		timps, hasImp := imps[entry.Import]
		if !hasImp {
			timps = make(map[int]int64)
			imps[entry.Import] = timps
		}

		timps[tick]++
	}
}

// ticksToReport converts aggregated TICKs into the analyze.Report format.
func ticksToReport(
	_ context.Context,
	ticks []analyze.TICK,
	reversedPeopleDict []string,
	tickSize time.Duration,
) analyze.Report {
	merged := Map{}
	commitStats := make(map[string]*ImportsCommitSummary)
	commitsByTick := make(map[int][]gitlib.Hash)

	for _, tick := range ticks {
		td, ok := tick.Data.(*TickData)
		if !ok || td == nil {
			continue
		}

		mergeImportMaps(merged, td.Imports)

		for hash, cs := range td.CommitStats {
			commitStats[hash] = cs
			commitsByTick[tick.Tick] = append(commitsByTick[tick.Tick], gitlib.NewHash(hash))
		}
	}

	report := analyze.Report{
		"imports":      merged,
		"author_index": reversedPeopleDict,
		"tick_size":    tickSize,
	}

	if len(commitStats) > 0 {
		report["commit_stats"] = commitStats
		report["commits_by_tick"] = commitsByTick
	}

	return report
}

// SnapshotPlumbing captures the current plumbing output state for one commit.
func (h *HistoryAnalyzer) SnapshotPlumbing() analyze.PlumbingSnapshot {
	return plumbing.Snapshot{
		UASTChanges: h.UAST.TransferChanges(),
		Tick:        h.Ticks.Tick,
		AuthorID:    h.Identity.AuthorID,
	}
}

// ApplySnapshot restores plumbing state from a previously captured snapshot.
func (h *HistoryAnalyzer) ApplySnapshot(snap analyze.PlumbingSnapshot) {
	snapshot, ok := snap.(plumbing.Snapshot)
	if !ok {
		return
	}

	h.UAST.SetChanges(snapshot.UASTChanges)
	h.Ticks.Tick = snapshot.Tick
	h.Identity.AuthorID = snapshot.AuthorID
}

// ReleaseSnapshot releases UAST trees owned by the snapshot.
func (h *HistoryAnalyzer) ReleaseSnapshot(snap analyze.PlumbingSnapshot) {
	ss, ok := snap.(plumbing.Snapshot)
	if !ok {
		return
	}

	plumbing.ReleaseSnapshotUAST(ss)
}

// Fork creates a copy of the analyzer for parallel processing.
// Each fork gets independent mutable state while sharing read-only config.
func (h *HistoryAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	forks := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		clone := &HistoryAnalyzer{
			BaseHistoryAnalyzer: h.BaseHistoryAnalyzer,
			UAST:                &plumbing.UASTChangesAnalyzer{},
			Identity:            &plumbing.IdentityDetector{},
			Ticks:               &plumbing.TicksSinceStart{},
			reversedPeopleDict:  h.reversedPeopleDict,
			TickSize:            h.TickSize,
		}

		forks[i] = clone
	}

	return forks
}

// Merge is a no-op since state is managed by the GenericAggregator.
func (h *HistoryAnalyzer) Merge(_ []analyze.HistoryAnalyzer) {}
