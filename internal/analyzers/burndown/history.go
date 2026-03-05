// Package burndown provides burndown functionality.
package burndown

import (
	"context"
	"errors"
	"fmt"
	"hash/fnv"
	"io"
	"maps"
	"runtime"
	"sync"
	"time"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/internal/burndown"
	"github.com/Sumatoshi-tech/codefang/internal/identity"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

// Sentinel errors for burndown analysis.
var (
	errPeopleNumberNegative   = errors.New("PeopleNumber is negative")
	errReversedPeopleDictType = errors.New("expected []string for reversedPeopleDict")
	errMissingBlob            = errors.New("missing blob")
	errFileNotExist           = errors.New("file does not exist")
	errUnexpectedBinary       = errors.New("previous version unexpectedly became binary")
)

// Configuration constants for burndown analysis.
const (
	// TickSizeThresholdHigh is the maximum tick size in hours for burndown granularity.
	TickSizeThresholdHigh = 24
	keyValue              = 2
	mrowValue             = 2
	renameCapDivisor      = 2

	// kib is 1 KiB in bytes.
	kib = 1024

	// estimatedStateSizeKiB is the per-commit working state growth estimate in KiB.
	estimatedStateSizeKiB = 950

	// estimatedTCSizeKiB is the per-commit TC payload size estimate in KiB.
	estimatedTCSizeKiB = 74
)

// Shard holds per-file burndown data within a partition.
// Uses PathID-indexed slices and activeIDs so iteration is over a slice (touched list), not map iteration (Track B).
type Shard struct {
	filesByID         []*burndown.File
	fileHistoriesByID []sparseHistory
	activeIDs         []PathID
	deltas            deltaBuffer
	mergedByID        map[PathID]bool
	deletionsByID     map[PathID]bool
	mu                sync.Mutex
}

type sparseHistory = map[int]map[int]int64

// DenseHistory is a two-dimensional matrix of line counts over time intervals.
type DenseHistory = [][]int64

// HistoryAnalyzer tracks line survival rates across commit history.
type HistoryAnalyzer struct {
	*analyze.BaseHistoryAnalyzer[*ComputedMetrics]
	common.IdentityMixin

	BlobCache            *plumbing.BlobCacheAnalyzer
	pathInterner         *PathInterner
	renames              map[string]string          // from → to.
	renamesReverse       map[string]map[string]bool // to → set of from (avoids range renames in handleDeletion).
	repository           *gitlib.Repository
	Ticks                *plumbing.TicksSinceStart
	FileDiff             *plumbing.FileDiffAnalyzer
	TreeDiff             *plumbing.TreeDiffAnalyzer
	HibernationDirectory string
	shards               []*Shard
	shardSpills          []shardSpillState // per-shard spill tracking for file treaps.
	spillDir             string            // parent temp dir for shard file spills.
	mergedAuthor         int
	HibernationThreshold int
	Granularity          int
	PeopleNumber         int
	TickSize             time.Duration
	Goroutines           int
	tick                 int
	isMerge              bool
	previousTick         int
	Sampling             int
	GlobalMu             sync.Mutex
	Debug                bool
	TrackFiles           bool
	HibernationToDisk    bool
	lastCommitTime       time.Time
}

const (
	// ConfigBurndownGranularity is the configuration key for the burndown band granularity.
	ConfigBurndownGranularity = "Burndown.Granularity"
	// ConfigBurndownSampling is the configuration key for the burndown sampling rate.
	ConfigBurndownSampling = "Burndown.Sampling"
	// ConfigBurndownTrackFiles is the configuration key for enabling per-file burndown tracking.
	ConfigBurndownTrackFiles = "Burndown.TrackFiles"
	// ConfigBurndownTrackPeople is the configuration key for enabling per-developer burndown tracking.
	ConfigBurndownTrackPeople = "Burndown.TrackPeople"
	// ConfigBurndownHibernationThreshold defines the hibernation memory threshold.
	ConfigBurndownHibernationThreshold = "Burndown.HibernationThreshold"
	// ConfigBurndownHibernationToDisk defines the hibernation to disk configuration constant.
	ConfigBurndownHibernationToDisk = "Burndown.HibernationOnDisk"
	// ConfigBurndownHibernationDirectory defines the hibernation directory configuration constant.
	ConfigBurndownHibernationDirectory = "Burndown.HibernationDirectory"
	// ConfigBurndownDebug defines the debug mode configuration constant.
	ConfigBurndownDebug = "Burndown.Debug"
	// ConfigBurndownGoroutines defines the goroutines configuration constant.
	ConfigBurndownGoroutines = "Burndown.Goroutines"
	// DefaultBurndownGranularity defines the default granularity in days.
	DefaultBurndownGranularity = 30
	// DefaultBurndownSampling defines the default sampling in ticks.
	// Matches Hercules: sampling equals granularity (30) for comparable output.
	DefaultBurndownSampling = 30
	// DefaultBurndownHibernationThreshold defines the default node count threshold for hibernation.
	DefaultBurndownHibernationThreshold = 1000
	// Sentinel value representing the current author.
	authorSelf = identity.AuthorMissing - 1
)

// NewHistoryAnalyzer creates a new burndown history analyzer.
func NewHistoryAnalyzer() *HistoryAnalyzer {
	ha := &HistoryAnalyzer{}

	ha.BaseHistoryAnalyzer = &analyze.BaseHistoryAnalyzer[*ComputedMetrics]{
		Desc: analyze.Descriptor{
			ID: "history/burndown",
			Description: "Line burndown stats indicate the numbers of lines which were last edited " +
				"within specific time intervals through time.",
			Mode: analyze.ModeHistory,
		},
		Sequential:         true,
		CPUHeavyFlag:       false,
		EstimatedStateSize: estimatedStateSizeKiB * kib,
		EstimatedTCSize:    estimatedTCSizeKiB * kib,
		ComputeMetricsFn:   ComputeAllMetrics,
		AggregatorFn:       ha.NewAggregator,
		TicksToReportFn: func(ctx context.Context, ticks []analyze.TICK) analyze.Report {
			return ticksToReport(
				ctx, ticks,
				ha.Granularity, ha.Sampling, ha.PeopleNumber,
				ha.TrackFiles, ha.TickSize,
				ha.GetReversedPeopleDict(), ha.pathInterner,
			)
		},
		SerializeTextFn: func(result analyze.Report, writer io.Writer) error {
			return ha.generateText(result, writer)
		},
		SerializePlotFn: func(result analyze.Report, writer io.Writer) error {
			return ha.generatePlot(result, writer)
		},
	}

	return ha
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (b *HistoryAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{
		{
			Name:        ConfigBurndownGranularity,
			Description: "How many time ticks there are in a single band.",
			Flag:        "granularity",
			Type:        pipeline.IntConfigurationOption,
			Default:     DefaultBurndownGranularity,
		},
		{
			Name:        ConfigBurndownSampling,
			Description: "How frequently to record the state in time ticks.",
			Flag:        "sampling",
			Type:        pipeline.IntConfigurationOption,
			Default:     DefaultBurndownSampling,
		},
		{
			Name:        ConfigBurndownTrackFiles,
			Description: "Record detailed statistics per each file.",
			Flag:        "burndown-files",
			Type:        pipeline.BoolConfigurationOption,
			Default:     false,
		},
		{
			Name:        ConfigBurndownTrackPeople,
			Description: "Record detailed statistics per each developer.",
			Flag:        "burndown-people",
			Type:        pipeline.BoolConfigurationOption,
			Default:     false,
		},
		{
			Name:        ConfigBurndownHibernationThreshold,
			Description: "The minimum size for the allocated memory in each branch to be compressed.",
			Flag:        "burndown-hibernation-threshold",
			Type:        pipeline.IntConfigurationOption,
			Default:     DefaultBurndownHibernationThreshold,
		},
		{
			Name:        ConfigBurndownHibernationToDisk,
			Description: "If true, save hibernated state to disk (no-op with default treap timeline).",
			Flag:        "burndown-hibernation-disk",
			Type:        pipeline.BoolConfigurationOption,
			Default:     true,
		},
		{
			Name:        ConfigBurndownHibernationDirectory,
			Description: "Temporary directory for hibernated state (no-op with default treap timeline).",
			Flag:        "burndown-hibernation-dir",
			Type:        pipeline.PathConfigurationOption,
			Default:     "",
		},
		{
			Name:        ConfigBurndownDebug,
			Description: "Validate the trees at each step.",
			Flag:        "burndown-debug",
			Type:        pipeline.BoolConfigurationOption,
			Default:     false,
		},
		{
			Name:        ConfigBurndownGoroutines,
			Description: "Number of goroutines to use for parallel processing.",
			Flag:        "burndown-goroutines",
			Type:        pipeline.IntConfigurationOption,
			Default:     runtime.NumCPU(),
		},
	}
}

// Configure sets up the analyzer with the provided facts.
func (b *HistoryAnalyzer) Configure(facts map[string]any) error {
	if val, exists := facts[ConfigBurndownGranularity].(int); exists {
		b.Granularity = val
	}

	if val, exists := facts[ConfigBurndownSampling].(int); exists {
		b.Sampling = val
	}

	if val, exists := facts[ConfigBurndownTrackFiles].(bool); exists {
		b.TrackFiles = val
	}

	err := b.configurePeopleTracking(facts)
	if err != nil {
		return err
	}

	if val, exists := facts[ConfigBurndownHibernationThreshold].(int); exists {
		b.HibernationThreshold = val
	}

	if val, exists := facts[ConfigBurndownHibernationToDisk].(bool); exists {
		b.HibernationToDisk = val
	} else {
		b.HibernationToDisk = true
	}

	if val, exists := facts[ConfigBurndownHibernationDirectory].(string); exists {
		b.HibernationDirectory = val
	} else if tmpVal, tmpExists := facts[analyze.ConfigTmpDir].(string); tmpExists && tmpVal != "" {
		b.HibernationDirectory = tmpVal
	}

	if val, exists := facts[ConfigBurndownDebug].(bool); exists {
		b.Debug = val
	}

	if val, exists := facts[ConfigBurndownGoroutines].(int); exists {
		b.Goroutines = val
	}

	if val, ok := pkgplumbing.GetTickSize(facts); ok {
		b.TickSize = val
	}

	return nil
}

// configurePeopleTracking sets up people tracking from the provided facts.
func (b *HistoryAnalyzer) configurePeopleTracking(facts map[string]any) error {
	people, exists := facts[ConfigBurndownTrackPeople].(bool)
	if !people || !exists {
		return nil
	}

	val, peopleCountExists := pkgplumbing.GetPeopleCount(facts)
	if !peopleCountExists {
		return nil
	}

	if val < 0 {
		return fmt.Errorf("%w: %d", errPeopleNumberNegative, val)
	}

	b.PeopleNumber = val

	rpd, ok := pkgplumbing.GetReversedPeopleDict(facts)
	if !ok {
		return errReversedPeopleDictType
	}

	b.ReversedPeopleDict = rpd

	return nil
}

// Initialize prepares the analyzer for processing commits.
func (b *HistoryAnalyzer) Initialize(repository *gitlib.Repository) error {
	if b.Granularity <= 0 {
		b.Granularity = DefaultBurndownGranularity
	}

	if b.Sampling <= 0 {
		b.Sampling = DefaultBurndownSampling
	}

	if b.Sampling > b.Granularity {
		b.Sampling = b.Granularity
	}

	if b.TickSize == 0 {
		b.TickSize = TickSizeThresholdHigh * time.Hour
	}

	if b.Goroutines <= 0 {
		b.Goroutines = runtime.NumCPU()
	}

	b.repository = repository

	if b.PeopleNumber < 0 {
		return fmt.Errorf("%w: %d", errPeopleNumberNegative, b.PeopleNumber)
	}

	if b.HibernationThreshold == 0 {
		b.HibernationThreshold = DefaultBurndownHibernationThreshold
	}

	if b.pathInterner == nil {
		b.pathInterner = NewPathInterner()
	}

	b.initFreshShards(b.Goroutines)
	b.shardSpills = make([]shardSpillState, b.Goroutines)
	b.spillDir = ""
	b.renames = map[string]string{}
	b.renamesReverse = map[string]map[string]bool{}
	b.tick = 0
	b.previousTick = 0

	return nil
}

// getShard returns the shard for a given file name.
func (b *HistoryAnalyzer) getShard(name string) *Shard {
	return b.shards[b.getShardIndex(name)]
}

func (b *HistoryAnalyzer) getShardIndex(name string) int {
	h := fnv.New32a()
	h.Write([]byte(name))

	idx := int(h.Sum32()) % len(b.shards)
	if idx < 0 {
		idx = -idx
	}

	return idx
}

// ensureCapacity grows shard slices so id is a valid index (Track B).
func (b *HistoryAnalyzer) ensureCapacity(shard *Shard, id PathID) {
	n := int(id) + 1
	if n <= len(shard.filesByID) {
		return
	}

	if cap(shard.filesByID) >= n {
		shard.filesByID = shard.filesByID[:n]
		shard.fileHistoriesByID = shard.fileHistoriesByID[:n]

		return
	}

	newFiles := make([]*burndown.File, n)
	copy(newFiles, shard.filesByID)
	shard.filesByID = newFiles
	newHistories := make([]sparseHistory, n)
	copy(newHistories, shard.fileHistoriesByID)
	shard.fileHistoriesByID = newHistories
}

// removeActiveID removes id from shard.activeIDs (swap-remove) (Track B).
func (b *HistoryAnalyzer) removeActiveID(shard *Shard, id PathID) {
	for i, aid := range shard.activeIDs {
		if aid == id {
			last := len(shard.activeIDs) - 1
			shard.activeIDs[i] = shard.activeIDs[last]
			shard.activeIDs = shard.activeIDs[:last]

			return
		}
	}
}

// Consume processes a single commit with the provided dependency results.
func (b *HistoryAnalyzer) Consume(_ context.Context, ac *analyze.Context) (analyze.TC, error) {
	input := commitInput{
		author:    b.Identity.AuthorID,
		tick:      b.Ticks.Tick,
		isMerge:   ac.Commit.NumParents() > 1,
		cache:     b.BlobCache.Cache,
		changes:   b.TreeDiff.Changes,
		fileDiffs: b.FileDiff.FileDiffs,
		time:      ac.Time,
	}

	err := b.consumeCommit(input)
	if err != nil {
		return analyze.TC{}, err
	}

	result := b.collectDeltas()
	computeCommitLineStats(result, input.tick)

	return analyze.TC{
		Data:       result,
		CommitHash: ac.Commit.Hash(),
	}, nil
}

// computeCommitLineStats derives LinesAdded/LinesRemoved from GlobalDeltas.
func computeCommitLineStats(cr *CommitResult, curTick int) {
	if cr == nil || len(cr.GlobalDeltas) == 0 {
		return
	}

	for prevTick, delta := range cr.GlobalDeltas[curTick] {
		if prevTick == curTick && delta > 0 {
			cr.LinesAdded += delta
		} else if delta < 0 {
			cr.LinesRemoved += -delta
		}
	}
}

// commitInput holds the resolved inputs for processing a single commit.
type commitInput struct {
	author    int
	tick      int
	isMerge   bool
	cache     map[gitlib.Hash]*pkgplumbing.CachedBlob
	changes   []*gitlib.Change
	fileDiffs map[string]pkgplumbing.FileDiffData
	time      time.Time
}

// consumeCommit processes a single commit through the full burndown pipeline.
func (b *HistoryAnalyzer) consumeCommit(input commitInput) error {
	b.resetDeltaBuffers()
	b.applyTickState(input)

	shardChanges, renames := b.groupChangesByShard(input.changes)

	err := b.processShardChanges(shardChanges, input.author, input.cache, input.fileDiffs)
	if err != nil {
		return err
	}

	err = b.routeRenames(renames, input.author, input.cache, input.fileDiffs)
	if err != nil {
		return err
	}

	b.tick = input.tick
	b.lastCommitTime = input.time

	return nil
}

// applyTickState sets the tick and either advances previousTick (normal) or initializes merge state.
func (b *HistoryAnalyzer) applyTickState(input commitInput) {
	b.isMerge = input.isMerge
	b.tick = input.tick

	if !input.isMerge {
		b.onNewTick()

		return
	}

	b.mergedAuthor = input.author
	b.resetMergedTracking()
}

// resetMergedTracking clears per-shard merge tracking maps for a new merge commit.
func (b *HistoryAnalyzer) resetMergedTracking() {
	for _, shard := range b.shards {
		shard.mergedByID = map[PathID]bool{}
	}
}

// routeRenames processes rename changes sequentially after parallel shard processing.
func (b *HistoryAnalyzer) routeRenames(
	renames []*gitlib.Change, author int,
	cache map[gitlib.Hash]*pkgplumbing.CachedBlob,
	fileDiffs map[string]pkgplumbing.FileDiffData,
) error {
	router := plumbing.ChangeRouter{
		OnRename: func(_, _ string, change *gitlib.Change) error {
			return b.handleModificationRename(change, author, cache, fileDiffs)
		},
	}

	return router.Route(renames)
}

// ConsumePrepared processes a pre-prepared commit.
// This is used by the pipelined runner for parallel commit preparation.
func (b *HistoryAnalyzer) ConsumePrepared(prepared *analyze.PreparedCommit) error {
	cache := make(map[gitlib.Hash]*pkgplumbing.CachedBlob, len(prepared.Cache))
	maps.Copy(cache, prepared.Cache)

	return b.consumeCommit(commitInput{
		author:    prepared.AuthorID,
		tick:      prepared.Tick,
		isMerge:   prepared.Ctx.Commit.NumParents() > 1,
		cache:     cache,
		changes:   prepared.Changes,
		fileDiffs: prepared.FileDiffs,
		time:      prepared.Ctx.Time,
	})
}

// groupChangesByShard partitions tree changes into per-shard slices and collects renames separately.
func (b *HistoryAnalyzer) groupChangesByShard(
	treeDiffs []*gitlib.Change,
) (shardChanges [][]*gitlib.Change, renames []*gitlib.Change) {
	shardChanges = make([][]*gitlib.Change, b.Goroutines)
	renames = make([]*gitlib.Change, 0)

	for _, change := range treeDiffs {
		action := change.Action

		if action == gitlib.Modify && change.From.Name != change.To.Name {
			renames = append(renames, change)

			continue
		}

		name := change.To.Name
		if action == gitlib.Delete {
			name = change.From.Name
		}

		idx := b.getShardIndex(name)
		shardChanges[idx] = append(shardChanges[idx], change)
	}

	return shardChanges, renames
}

// processShardChanges processes grouped changes across shards in parallel.
func (b *HistoryAnalyzer) processShardChanges(
	shardChanges [][]*gitlib.Change, author int, cache map[gitlib.Hash]*pkgplumbing.CachedBlob,
	fileDiffs map[string]pkgplumbing.FileDiffData,
) error {
	var wg sync.WaitGroup

	errs := make([]error, b.Goroutines)

	for i := range b.Goroutines {
		changes := shardChanges[i]
		if len(changes) == 0 {
			continue
		}

		wg.Add(1)

		go func(idx int, changes []*gitlib.Change) {
			defer wg.Done()

			shard := b.shards[idx]

			router := plumbing.ChangeRouter{
				OnInsert: func(change *gitlib.Change) error {
					return b.handleInsertion(shard, change, author, cache)
				},
				OnDelete: func(change *gitlib.Change) error {
					return b.handleDeletion(shard, change, author, cache)
				},
				OnModify: func(change *gitlib.Change) error {
					return b.handleModification(shard, change, author, cache, fileDiffs)
				},
			}

			err := router.Route(changes)
			if err != nil {
				errs[idx] = err
			}
		}(i, changes)
	}

	wg.Wait()

	for _, err := range errs {
		if err != nil {
			return err
		}
	}

	return nil
}

// SnapshotPlumbing captures the current plumbing state.
func (b *HistoryAnalyzer) SnapshotPlumbing() analyze.PlumbingSnapshot {
	return plumbing.Snapshot{
		Changes:   b.TreeDiff.Changes,
		BlobCache: b.BlobCache.Cache,
		FileDiffs: b.FileDiff.FileDiffs,
		Tick:      b.Ticks.Tick,
		AuthorID:  b.Identity.AuthorID,
	}
}

// ApplySnapshot restores plumbing state from a snapshot.
func (b *HistoryAnalyzer) ApplySnapshot(snap analyze.PlumbingSnapshot) {
	snapshot, ok := snap.(plumbing.Snapshot)
	if !ok {
		return
	}

	b.TreeDiff.Changes = snapshot.Changes
	b.BlobCache.Cache = snapshot.BlobCache
	b.FileDiff.FileDiffs = snapshot.FileDiffs
	b.Ticks.Tick = snapshot.Tick
	b.Identity.AuthorID = snapshot.AuthorID
}

// ReleaseSnapshot is a no-op for burndown (no UAST resources).
func (b *HistoryAnalyzer) ReleaseSnapshot(_ analyze.PlumbingSnapshot) {}

// NewAggregator creates a burndown aggregator that accumulates sparse history
// deltas from the TC stream and produces dense history matrices for the report.
func (b *HistoryAnalyzer) NewAggregator(opts analyze.AggregatorOptions) analyze.Aggregator {
	return newAggregator(
		opts,
		b.Granularity, b.Sampling, b.PeopleNumber,
		b.TrackFiles, b.TickSize,
		b.GetReversedPeopleDict(), b.pathInterner,
	)
}

// ExtractCommitTimeSeries implements analyze.CommitTimeSeriesProvider.
// It extracts per-commit burndown summary data for the unified timeseries output.
func (b *HistoryAnalyzer) ExtractCommitTimeSeries(report analyze.Report) map[string]any {
	commitStats, ok := report["commit_stats"].(map[string]*CommitSummary)
	if !ok || len(commitStats) == 0 {
		return nil
	}

	result := make(map[string]any, len(commitStats))

	for hash, cs := range commitStats {
		result[hash] = map[string]any{
			"lines_added":   cs.LinesAdded,
			"lines_removed": cs.LinesRemoved,
		}
	}

	return result
}

// Helpers.

func (b *HistoryAnalyzer) packPersonWithTick(person, tick int) int {
	if b.PeopleNumber == 0 {
		return tick
	}

	result := tick & burndown.TreeMergeMark
	result |= person << burndown.TreeMaxBinPower

	return result
}

func (b *HistoryAnalyzer) unpackPersonWithTick(value int) (person, tick int) {
	if b.PeopleNumber == 0 {
		return identity.AuthorMissing, value
	}

	return value >> burndown.TreeMaxBinPower, value & burndown.TreeMergeMark
}

func (b *HistoryAnalyzer) onNewTick() {
	if b.tick > b.previousTick {
		b.previousTick = b.tick
	}

	b.mergedAuthor = identity.AuthorMissing
}
