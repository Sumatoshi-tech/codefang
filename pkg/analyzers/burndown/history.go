// Package burndown provides burndown functionality.
package burndown

import (
	"context"
	"errors"
	"fmt"
	"hash/fnv"
	"io"
	"maps"
	"os"
	"runtime"
	"sort"
	"sync"
	"time"
	"unicode/utf8"

	"github.com/sergi/go-diff/diffmatchpatch"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/burndown"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/identity"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

// Sentinel errors for burndown analysis.
var (
	errPeopleNumberNegative    = errors.New("PeopleNumber is negative")
	errReversedPeopleDictType  = errors.New("expected []string for reversedPeopleDict")
	errMissingBlob             = errors.New("missing blob")
	errFileAlreadyExists       = errors.New("file already exists")
	errFileNotExist            = errors.New("file does not exist")
	errUnexpectedBinary        = errors.New("previous version unexpectedly became binary")
	errInternalIntegritySource = errors.New("internal integrity error src mismatch")
)

// Configuration constants for burndown analysis.
const (
	// TickSizeThresholdHigh is the maximum tick size in hours for burndown granularity.
	TickSizeThresholdHigh = 24
	keyValue              = 2
	mrowValue             = 2
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

	BlobCache            *plumbing.BlobCacheAnalyzer
	pathInterner         *PathInterner
	renames              map[string]string          // from → to.
	renamesReverse       map[string]map[string]bool // to → set of from (avoids range renames in handleDeletion).
	repository           *gitlib.Repository
	Ticks                *plumbing.TicksSinceStart
	Identity             *plumbing.IdentityDetector
	FileDiff             *plumbing.FileDiffAnalyzer
	TreeDiff             *plumbing.TreeDiffAnalyzer
	HibernationDirectory string
	shards               []*Shard
	shardSpills          []shardSpillState // per-shard spill tracking for file treaps.
	spillDir             string            // parent temp dir for shard file spills.
	reversedPeopleDict   []string
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
		EstimatedStateSize: 950 * 1024, //nolint:mnd // Estimated size.
		EstimatedTCSize:    74 * 1024,  //nolint:mnd // Estimated size.
		ComputeMetricsFn:   ComputeAllMetrics,
		AggregatorFn:       ha.NewAggregator,
		TicksToReportFn: func(ctx context.Context, ticks []analyze.TICK) analyze.Report {
			return ticksToReport(
				ctx, ticks,
				ha.Granularity, ha.Sampling, ha.PeopleNumber,
				ha.TrackFiles, ha.TickSize,
				ha.reversedPeopleDict, ha.pathInterner,
			)
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
	}

	if val, exists := facts[ConfigBurndownDebug].(bool); exists {
		b.Debug = val
	}

	if val, exists := facts[ConfigBurndownGoroutines].(int); exists {
		b.Goroutines = val
	}

	if val, exists := facts[pkgplumbing.FactTickSize].(time.Duration); exists {
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

	val, peopleCountExists := facts[identity.FactIdentityDetectorPeopleCount].(int)
	if !peopleCountExists {
		return nil
	}

	if val < 0 {
		return fmt.Errorf("%w: %d", errPeopleNumberNegative, val)
	}

	b.PeopleNumber = val

	rpd, ok := facts[identity.FactIdentityDetectorReversedPeopleDict].([]string)
	if !ok {
		return errReversedPeopleDictType
	}

	b.reversedPeopleDict = rpd

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

	b.shards = make([]*Shard, b.Goroutines)
	for i := range b.Goroutines {
		b.shards[i] = &Shard{
			mergedByID:    map[PathID]bool{},
			deletionsByID: map[PathID]bool{},
		}
	}

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
	author := b.Identity.AuthorID
	tick := b.Ticks.Tick
	isMerge := ac.IsMerge
	b.isMerge = isMerge

	b.resetDeltaBuffers()

	if !isMerge {
		b.tick = tick
		b.onNewTick()
	} else {
		b.tick = tick
		b.mergedAuthor = author

		for _, shard := range b.shards {
			shard.mergedByID = map[PathID]bool{}
		}
	}

	cache := b.BlobCache.Cache
	fileDiffs := b.FileDiff.FileDiffs
	shardChanges, renames := b.groupChangesByShard(b.TreeDiff.Changes)

	err := b.processShardChanges(shardChanges, author, cache, fileDiffs)
	if err != nil {
		return analyze.TC{}, err
	}

	renameRouter := plumbing.ChangeRouter{
		OnRename: func(_, _ string, change *gitlib.Change) error {
			return b.handleModificationRename(change, author, cache, fileDiffs)
		},
	}

	err = renameRouter.Route(renames)
	if err != nil {
		return analyze.TC{}, err
	}

	b.tick = tick
	b.lastCommitTime = ac.Time

	result := b.collectDeltas()

	return analyze.TC{Data: result}, nil
}

// ConsumePrepared processes a pre-prepared commit.
// This is used by the pipelined runner for parallel commit preparation.
func (b *HistoryAnalyzer) ConsumePrepared(prepared *analyze.PreparedCommit) error {
	author := prepared.AuthorID
	tick := prepared.Tick
	isMerge := prepared.Ctx.IsMerge
	b.isMerge = isMerge

	b.resetDeltaBuffers()

	if !isMerge {
		b.tick = tick
		b.onNewTick()
	} else {
		b.tick = tick
		b.mergedAuthor = author

		for _, shard := range b.shards {
			shard.mergedByID = map[PathID]bool{}
		}
	}

	// Convert cache type (gitlib.CachedBlob to pkgplumbing.CachedBlob - they're aliased).
	cache := make(map[gitlib.Hash]*pkgplumbing.CachedBlob, len(prepared.Cache))
	maps.Copy(cache, prepared.Cache)

	shardChanges, renames := b.groupChangesByShard(prepared.Changes)

	err := b.processShardChanges(shardChanges, author, cache, prepared.FileDiffs)
	if err != nil {
		return err
	}

	renameRouter := plumbing.ChangeRouter{
		OnRename: func(_, _ string, change *gitlib.Change) error {
			return b.handleModificationRename(change, author, cache, prepared.FileDiffs)
		},
	}

	err = renameRouter.Route(renames)
	if err != nil {
		return err
	}

	b.tick = tick
	b.lastCommitTime = prepared.Ctx.Time

	return nil
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

// Fork creates a copy of the analyzer for parallel processing.
func (b *HistoryAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)

	for i := range n {
		clone := &HistoryAnalyzer{
			// Share dependencies (injected by framework, read-only).
			pathInterner: b.pathInterner, // Shared for consistent PathIDs.
			repository:   b.repository,

			// Independent plumbing state (not shared with parent).
			Identity:  &plumbing.IdentityDetector{},
			TreeDiff:  &plumbing.TreeDiffAnalyzer{},
			Ticks:     &plumbing.TicksSinceStart{},
			BlobCache: &plumbing.BlobCacheAnalyzer{},
			FileDiff:  &plumbing.FileDiffAnalyzer{},

			// Copy configuration.
			HibernationDirectory: b.HibernationDirectory,
			HibernationThreshold: b.HibernationThreshold,
			Granularity:          b.Granularity,
			PeopleNumber:         b.PeopleNumber,
			TickSize:             b.TickSize,
			Goroutines:           b.Goroutines,
			Sampling:             b.Sampling,
			Debug:                b.Debug,
			TrackFiles:           b.TrackFiles,
			HibernationToDisk:    b.HibernationToDisk,
			reversedPeopleDict:   b.reversedPeopleDict,
		}

		// Create fresh shards for this fork.
		clone.shards = make([]*Shard, b.Goroutines)
		for j := range b.Goroutines {
			clone.shards[j] = &Shard{
				mergedByID:    map[PathID]bool{},
				deletionsByID: map[PathID]bool{},
			}
		}

		// Fresh rename tracking.
		clone.renames = map[string]string{}
		clone.renamesReverse = map[string]map[string]bool{}

		// Reset per-chunk state.
		clone.tick = 0
		clone.previousTick = 0
		clone.isMerge = false
		clone.mergedAuthor = 0

		res[i] = clone
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (b *HistoryAnalyzer) Merge(branches []analyze.HistoryAnalyzer) {
	for _, branch := range branches {
		other, ok := branch.(*HistoryAnalyzer)
		if !ok {
			continue
		}

		b.mergeShards(other)
		b.mergeRenameTracking(other)
		b.mergeTicks(other)
	}
}

// mergeShards is a no-op after TC migration — history maps no longer live in shards.
// Delta buffers are per-commit and collected into TCs, not accumulated across commits.
func (b *HistoryAnalyzer) mergeShards(_ *HistoryAnalyzer) {}

// mergeRenameTracking merges rename tracking from another analyzer.
func (b *HistoryAnalyzer) mergeRenameTracking(other *HistoryAnalyzer) {
	b.GlobalMu.Lock()
	defer b.GlobalMu.Unlock()

	for from, to := range other.renames {
		if to == "" {
			continue
		}

		b.renames[from] = to

		if b.renamesReverse[to] == nil {
			b.renamesReverse[to] = map[string]bool{}
		}

		b.renamesReverse[to][from] = true
	}
}

// mergeTicks updates tick tracking from another analyzer.
func (b *HistoryAnalyzer) mergeTicks(other *HistoryAnalyzer) {
	if other.tick > b.tick {
		b.tick = other.tick
	}

	if other.previousTick > b.previousTick {
		b.previousTick = other.previousTick
	}

	if !other.lastCommitTime.IsZero() && other.lastCommitTime.After(b.lastCommitTime) {
		b.lastCommitTime = other.lastCommitTime
	}
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

// Hibernate releases resources between processing phases.
// Spills file treaps and file histories to disk to free memory.
// History maps no longer live in shards — they are in the aggregator.
func (b *HistoryAnalyzer) Hibernate() error {
	err := b.ensureSpillDir()
	if err != nil {
		return fmt.Errorf("burndown spill dir: %w", err)
	}

	for i, shard := range b.shards {
		shard.mu.Lock()

		if b.spillDir != "" {
			// Spill file treaps and file histories to disk, freeing treap nodes.
			filesErr := spillShardFiles(shard, &b.shardSpills[i], b.spillDir, i)
			if filesErr != nil {
				shard.mu.Unlock()

				return fmt.Errorf("burndown spill shard %d files: %w", i, filesErr)
			}
		}

		// Clear per-commit tracking maps.
		shard.mergedByID = make(map[PathID]bool)
		shard.deletionsByID = make(map[PathID]bool)

		shard.mu.Unlock()
	}

	return nil
}

// ensureSpillDir creates the parent temp directory for shard history spills.
func (b *HistoryAnalyzer) ensureSpillDir() error {
	if b.spillDir != "" {
		return nil
	}

	dir, err := os.MkdirTemp("", "codefang-burndown-spill-*")
	if err != nil {
		return fmt.Errorf("create burndown spill dir: %w", err)
	}

	b.spillDir = dir

	return nil
}

// Boot performs early initialization before repository processing.
// Restores spilled file treaps and file histories, re-attaches updaters,
// and ensures per-shard tracking maps are ready for the next chunk.
func (b *HistoryAnalyzer) Boot() error {
	for i, shard := range b.shards {
		shard.mu.Lock()

		// Restore file treaps and file histories from the last spill.
		if b.spillDir != "" && i < len(b.shardSpills) && b.shardSpills[i].fileSpillN > 0 {
			err := loadSpilledFiles(shard, &b.shardSpills[i])
			if err != nil {
				shard.mu.Unlock()

				return fmt.Errorf("restore shard %d files: %w", i, err)
			}

			// Re-attach updaters to restored files.
			// createUpdaters() closures capture shard map references that were
			// invalidated when fileHistoriesByID was restored from disk.
			for _, id := range shard.activeIDs {
				if int(id) >= len(shard.filesByID) {
					continue
				}

				file := shard.filesByID[id]
				if file != nil {
					file.ReplaceUpdaters(b.createUpdaters(shard, id))
				}
			}
		}

		if shard.mergedByID == nil {
			shard.mergedByID = make(map[PathID]bool)
		}

		if shard.deletionsByID == nil {
			shard.deletionsByID = make(map[PathID]bool)
		}

		shard.mu.Unlock()
	}

	// Initialize delta buffers so updaters attached during Boot have valid targets.
	// Consume() calls resetDeltaBuffers() again before processing each commit.
	b.resetDeltaBuffers()

	return nil
}

// workingStateSize is the estimated bytes of working state per commit
// for the burndown analyzer (per-line ownership maps, file timelines, history matrices).
// Burndown is the heaviest analyzer — state grows proportional to the number of
// active files and ownership map entries, not linearly at multi-MiB per commit.
const workingStateSize = 950 * 1024

// avgTCSize is the estimated bytes of TC payload per commit
// for the burndown analyzer.
const avgTCSize = 74 * 1024

// WorkingStateSize returns the estimated bytes of working state per commit.
func (b *HistoryAnalyzer) WorkingStateSize() int64 { return workingStateSize }

// AvgTCSize returns the estimated bytes of TC payload per commit.
func (b *HistoryAnalyzer) AvgTCSize() int64 { return avgTCSize }

// CleanupSpills removes all shard spill temp files. Safe to call multiple times.
func (b *HistoryAnalyzer) CleanupSpills() {
	for i := range b.shardSpills {
		cleanupShardSpills(&b.shardSpills[i])
	}

	if b.spillDir != "" {
		os.RemoveAll(b.spillDir)
		b.spillDir = ""
	}
}

// Serialize writes the analysis result to the given writer.
func (b *HistoryAnalyzer) Serialize(result analyze.Report, format string, writer io.Writer) error {
	if format == analyze.FormatPlot {
		return b.generatePlot(result, writer)
	}

	if format == analyze.FormatText {
		return b.generateText(result, writer)
	}

	if b.BaseHistoryAnalyzer != nil {
		return b.BaseHistoryAnalyzer.Serialize(result, format, writer)
	}

	// Create temporary embedded to handle other formats properly when only called from tests without properly
	// initialized analyzer structure.
	return (&analyze.BaseHistoryAnalyzer[*ComputedMetrics]{
		ComputeMetricsFn: ComputeAllMetrics,
	}).Serialize(result, format, writer)
}

// SerializeTICKs converts aggregated TICKs into the final report and serializes it.
func (b *HistoryAnalyzer) SerializeTICKs(ticks []analyze.TICK, format string, writer io.Writer) error {
	if format == analyze.FormatPlot || format == analyze.FormatText {
		report, err := b.ReportFromTICKs(context.Background(), ticks)
		if err != nil {
			return err
		}

		if format == analyze.FormatPlot {
			return b.generatePlot(report, writer)
		}

		return b.generateText(report, writer)
	}

	if b.BaseHistoryAnalyzer != nil {
		return b.BaseHistoryAnalyzer.SerializeTICKs(ticks, format, writer)
	}

	return (&analyze.BaseHistoryAnalyzer[*ComputedMetrics]{
		ComputeMetricsFn: ComputeAllMetrics,
		TicksToReportFn: func(ctx context.Context, t []analyze.TICK) analyze.Report {
			report, err := b.ReportFromTICKs(ctx, t)
			if err != nil {
				return nil
			}

			return report
		},
	}).SerializeTICKs(ticks, format, writer)
}

// NewAggregator creates a burndown aggregator that accumulates sparse history
// deltas from the TC stream and produces dense history matrices for the report.
func (b *HistoryAnalyzer) NewAggregator(opts analyze.AggregatorOptions) analyze.Aggregator {
	return newAggregator(
		opts,
		b.Granularity, b.Sampling, b.PeopleNumber,
		b.TrackFiles, b.TickSize,
		b.reversedPeopleDict, b.pathInterner,
	)
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

// resetDeltaBuffers clears per-shard delta buffers before processing a new commit.
func (b *HistoryAnalyzer) resetDeltaBuffers() {
	for _, shard := range b.shards {
		shard.deltas.globalDeltas = sparseHistory{}

		if b.PeopleNumber > 0 {
			shard.deltas.peopleDeltas = map[int]sparseHistory{}
			shard.deltas.matrixDeltas = nil
		}

		if b.TrackFiles {
			shard.deltas.fileDeltas = map[PathID]sparseHistory{}
		}
	}
}

// collectDeltas merges delta buffers from all shards into a single CommitResult.
func (b *HistoryAnalyzer) collectDeltas() *CommitResult {
	result := &CommitResult{
		GlobalDeltas: sparseHistory{},
	}

	for _, shard := range b.shards {
		mergeSparseHistory(result.GlobalDeltas, shard.deltas.globalDeltas)
		b.collectPeopleDeltas(result, shard)
		b.collectMatrixDeltas(result, shard)
		b.collectFileDeltas(result, shard)
	}

	if b.TrackFiles && b.PeopleNumber > 0 {
		result.FileOwnership = b.collectFileOwnership()
	}

	return result
}

// collectFileOwnership extracts per-file author ownership from live file
// segments across all shards. Each file's treap stores packed [author|tick]
// values; we iterate segments to sum line counts per author.
func (b *HistoryAnalyzer) collectFileOwnership() map[PathID]map[int]int {
	ownership := map[PathID]map[int]int{}

	for _, shard := range b.shards {
		for pathID, file := range shard.filesByID {
			if file == nil {
				continue
			}

			pid := PathID(pathID)
			fileOwnership := extractFileOwnership(file, b.unpackPersonWithTick)

			if len(fileOwnership) == 0 {
				continue
			}

			if ownership[pid] == nil {
				ownership[pid] = fileOwnership
			} else {
				for author, count := range fileOwnership {
					ownership[pid][author] += count
				}
			}
		}
	}

	return ownership
}

// extractFileOwnership iterates a file's segments and sums line counts per
// author by unpacking the packed [author|tick] value stored in each segment.
func extractFileOwnership(
	file *burndown.File,
	unpack func(int) (int, int),
) map[int]int {
	result := map[int]int{}

	for _, seg := range file.Segments() {
		if seg.Value == burndown.TreeEnd {
			continue
		}

		author, _ := unpack(int(seg.Value))
		if author != identity.AuthorMissing {
			result[author] += seg.Length
		}
	}

	return result
}

func (b *HistoryAnalyzer) collectPeopleDeltas(result *CommitResult, shard *Shard) {
	if b.PeopleNumber == 0 {
		return
	}

	for author, history := range shard.deltas.peopleDeltas {
		if len(history) == 0 {
			continue
		}

		if result.PeopleDeltas == nil {
			result.PeopleDeltas = map[int]sparseHistory{}
		}

		if result.PeopleDeltas[author] == nil {
			result.PeopleDeltas[author] = sparseHistory{}
		}

		mergeSparseHistory(result.PeopleDeltas[author], history)
	}
}

func (b *HistoryAnalyzer) collectMatrixDeltas(result *CommitResult, shard *Shard) {
	if b.PeopleNumber == 0 {
		return
	}

	for author, row := range shard.deltas.matrixDeltas {
		if len(row) == 0 {
			continue
		}

		for len(result.MatrixDeltas) <= author {
			result.MatrixDeltas = append(result.MatrixDeltas, nil)
		}

		if result.MatrixDeltas[author] == nil {
			result.MatrixDeltas[author] = map[int]int64{}
		}

		for other, count := range row {
			result.MatrixDeltas[author][other] += count
		}
	}
}

func (b *HistoryAnalyzer) collectFileDeltas(result *CommitResult, shard *Shard) {
	if !b.TrackFiles {
		return
	}

	for pathID, history := range shard.deltas.fileDeltas {
		if len(history) == 0 {
			continue
		}

		if result.FileDeltas == nil {
			result.FileDeltas = map[PathID]sparseHistory{}
		}

		if result.FileDeltas[pathID] == nil {
			result.FileDeltas[pathID] = sparseHistory{}
		}

		mergeSparseHistory(result.FileDeltas[pathID], history)
	}
}

func (b *HistoryAnalyzer) updateGlobal(shard *Shard, currentTime, previousTime, delta int) {
	_, curTick := b.unpackPersonWithTick(currentTime)
	_, prevTick := b.unpackPersonWithTick(previousTime)

	currentHistory := shard.deltas.globalDeltas[curTick]
	if currentHistory == nil {
		currentHistory = map[int]int64{}
		shard.deltas.globalDeltas[curTick] = currentHistory
	}

	currentHistory[prevTick] += int64(delta)
}

func (b *HistoryAnalyzer) updateFile(shard *Shard, pathID PathID, currentTime, previousTime, delta int) {
	_, curTick := b.unpackPersonWithTick(currentTime)
	_, prevTick := b.unpackPersonWithTick(previousTime)

	history := shard.deltas.fileDeltas[pathID]
	if history == nil {
		history = sparseHistory{}
		shard.deltas.fileDeltas[pathID] = history
	}

	currentHistory := history[curTick]
	if currentHistory == nil {
		currentHistory = map[int]int64{}
		history[curTick] = currentHistory
	}

	currentHistory[prevTick] += int64(delta)
}

func (b *HistoryAnalyzer) updateAuthor(shard *Shard, currentTime, previousTime, delta int) {
	previousAuthor, prevTick := b.unpackPersonWithTick(previousTime)
	if previousAuthor == identity.AuthorMissing {
		return
	}

	_, curTick := b.unpackPersonWithTick(currentTime)

	history := shard.deltas.peopleDeltas[previousAuthor]
	if history == nil {
		history = sparseHistory{}
		shard.deltas.peopleDeltas[previousAuthor] = history
	}

	currentHistory := history[curTick]
	if currentHistory == nil {
		currentHistory = map[int]int64{}
		history[curTick] = currentHistory
	}

	currentHistory[prevTick] += int64(delta)
}

func (b *HistoryAnalyzer) updateMatrix(shard *Shard, currentTime, previousTime, delta int) {
	newAuthor, _ := b.unpackPersonWithTick(currentTime)
	oldAuthor, _ := b.unpackPersonWithTick(previousTime)

	if oldAuthor == identity.AuthorMissing {
		return
	}

	if newAuthor == oldAuthor && delta > 0 {
		newAuthor = authorSelf
	}

	// Grow matrixDeltas if needed.
	for len(shard.deltas.matrixDeltas) <= oldAuthor {
		shard.deltas.matrixDeltas = append(shard.deltas.matrixDeltas, nil)
	}

	row := shard.deltas.matrixDeltas[oldAuthor]
	if row == nil {
		row = map[int]int64{}
		shard.deltas.matrixDeltas[oldAuthor] = row
	}

	row[newAuthor] += int64(delta)
}

func (b *HistoryAnalyzer) createUpdaters(shard *Shard, pathID PathID) []burndown.Updater {
	const maxUpdaters = 4 // global + file + author + matrix.

	updaters := make([]burndown.Updater, 0, maxUpdaters)

	updaters = append(updaters, func(currentTime, previousTime, delta int) {
		b.updateGlobal(shard, currentTime, previousTime, delta)
	})

	if b.TrackFiles {
		updaters = append(updaters, func(currentTime, previousTime, delta int) {
			b.updateFile(shard, pathID, currentTime, previousTime, delta)
		})
	}

	if b.PeopleNumber > 0 {
		updaters = append(updaters, func(currentTime, previousTime, delta int) {
			b.updateAuthor(shard, currentTime, previousTime, delta)
		}, func(currentTime, previousTime, delta int) {
			b.updateMatrix(shard, currentTime, previousTime, delta)
		})
	}

	return updaters
}

func (b *HistoryAnalyzer) newFile(
	shard *Shard, pathID PathID, author int, tick int, size int,
) *burndown.File {
	updaters := b.createUpdaters(shard, pathID)

	if b.PeopleNumber > 0 {
		tick = b.packPersonWithTick(author, tick)
	}

	return burndown.NewFile(tick, size, updaters...)
}

func (b *HistoryAnalyzer) handleInsertion(
	shard *Shard, change *gitlib.Change, author int, cache map[gitlib.Hash]*pkgplumbing.CachedBlob,
) error {
	blob := cache[change.To.Hash]
	if blob == nil {
		return fmt.Errorf("%w for insertion %s (%s)", errMissingBlob, change.To.Name, change.To.Hash)
	}

	lines, err := blob.CountLines()
	if err != nil {
		return nil
	}

	name := change.To.Name
	id := b.pathInterner.Intern(name)
	b.ensureCapacity(shard, id)

	if shard.filesByID[id] != nil {
		return fmt.Errorf("%w: %s", errFileAlreadyExists, name)
	}

	file := b.newFile(shard, id, author, b.tick, lines)
	shard.filesByID[id] = file
	shard.activeIDs = append(shard.activeIDs, id)

	delete(shard.deletionsByID, id)

	if b.isMerge {
		shard.mergedByID[id] = true
	}

	return nil
}

func (b *HistoryAnalyzer) handleDeletion(
	shard *Shard, change *gitlib.Change, author int, cache map[gitlib.Hash]*pkgplumbing.CachedBlob,
) error {
	var name string
	if change.To.Hash != gitlib.ZeroHash() {
		name = change.To.Name
	} else {
		name = change.From.Name
	}

	id := b.pathInterner.Intern(name)
	b.ensureCapacity(shard, id)

	file := shard.filesByID[id]
	if file == nil {
		return nil
	}

	blob := cache[change.From.Hash]
	if blob == nil {
		return fmt.Errorf("%w for deletion %s (%s)", errMissingBlob, name, change.From.Hash)
	}

	lines, err := blob.CountLines()
	if err != nil {
		return fmt.Errorf("%w: %s", errUnexpectedBinary, name)
	}

	tick := b.tick

	isDeletion := shard.deletionsByID[id]
	shard.deletionsByID[id] = true

	if b.isMerge && !isDeletion {
		tick = 0
	}

	file.Update(b.packPersonWithTick(author, tick), 0, 0, lines)
	file.Delete()

	shard.filesByID[id] = nil
	shard.fileHistoriesByID[id] = nil
	b.removeActiveID(shard, id)

	stack := []string{name}

	b.GlobalMu.Lock()

	for len(stack) > 0 {
		head := stack[len(stack)-1]
		stack = stack[:len(stack)-1]

		oldTo := b.renames[head]
		if oldTo != "" {
			delete(b.renamesReverse[oldTo], head)

			if len(b.renamesReverse[oldTo]) == 0 {
				delete(b.renamesReverse, oldTo)
			}
		}

		b.renames[head] = ""

		for child := range b.renamesReverse[head] {
			stack = append(stack, child)
			b.renames[child] = "" // clear so when child is popped we don't use stale oldTo.
		}

		delete(b.renamesReverse, head)
	}

	b.GlobalMu.Unlock()

	if b.isMerge {
		shard.mergedByID[id] = false
	}

	return nil
}

func (b *HistoryAnalyzer) handleModification(
	shard *Shard, change *gitlib.Change, author int,
	cache map[gitlib.Hash]*pkgplumbing.CachedBlob, diffs map[string]pkgplumbing.FileDiffData,
) error {
	// This method handles modification WITHOUT rename (checked in Consume).
	id := b.pathInterner.Intern(change.From.Name)
	b.ensureCapacity(shard, id)

	if b.isMerge {
		shard.mergedByID[id] = true
	}

	file := shard.filesByID[id]
	if file == nil {
		return b.handleInsertion(shard, change, author, cache)
	}

	blobFrom := cache[change.From.Hash]
	if blobFrom == nil {
		return fmt.Errorf("%w: blobFrom for modification %s (%s)", errMissingBlob, change.From.Name, change.From.Hash)
	}

	_, errFrom := blobFrom.CountLines()

	blobTo := cache[change.To.Hash]
	if blobTo == nil {
		return fmt.Errorf("%w: blobTo for modification %s (%s)", errMissingBlob, change.To.Name, change.To.Hash)
	}

	_, errTo := blobTo.CountLines()
	if !errors.Is(errFrom, errTo) { // Error comparison is intentional.
		if errFrom != nil {
			return b.handleInsertion(shard, change, author, cache)
		}

		return b.handleDeletion(shard, change, author, cache)
	} else if errFrom != nil {
		return nil
	}

	thisDiffs := diffs[change.To.Name]
	if file.Len() != thisDiffs.OldLinesOfCode {
		return fmt.Errorf("%w: %s src %d != %d",
			errInternalIntegritySource, change.To.Name, thisDiffs.OldLinesOfCode, file.Len())
	}

	b.applyDiffs(file, thisDiffs, author)

	return nil
}

func (b *HistoryAnalyzer) handleModificationRename(
	change *gitlib.Change, author int,
	cache map[gitlib.Hash]*pkgplumbing.CachedBlob, diffs map[string]pkgplumbing.FileDiffData,
) error {
	// Handles modification WITH rename (From != To).
	// This runs sequentially, so we can access shards safely if we look them up.
	shardFrom := b.getShard(change.From.Name)
	fromID := b.pathInterner.Intern(change.From.Name)
	b.ensureCapacity(shardFrom, fromID)

	file := shardFrom.filesByID[fromID]
	if file == nil {
		// Fallback to insertion in To shard.
		shardTo := b.getShard(change.To.Name)

		return b.handleInsertion(shardTo, change, author, cache)
	}

	if change.To.Name != change.From.Name {
		err := b.handleRename(change.From.Name, change.To.Name)
		if err != nil {
			return err
		}
		// File is now at change.To.Name in correct shard.
		shardTo := b.getShard(change.To.Name)
		toID := b.pathInterner.Intern(change.To.Name)
		b.ensureCapacity(shardTo, toID)
		file = shardTo.filesByID[toID]
	}

	blobFrom := cache[change.From.Hash]
	if blobFrom == nil {
		return fmt.Errorf("%w: blobFrom for rename %s (%s)", errMissingBlob, change.From.Name, change.From.Hash)
	}

	_, errFrom := blobFrom.CountLines()

	blobTo := cache[change.To.Hash]
	if blobTo == nil {
		return fmt.Errorf("%w: blobTo for rename %s (%s)", errMissingBlob, change.To.Name, change.To.Hash)
	}

	_, errTo := blobTo.CountLines()
	if !errors.Is(errFrom, errTo) { // Error comparison is intentional.
		if errFrom != nil {
			shardTo := b.getShard(change.To.Name)

			return b.handleInsertion(shardTo, change, author, cache)
		}
		// HandleDeletion on new name? Or old?
		// Logic suggests if it became binary/error, we delete it.
		// But we just renamed it.
		// Let's defer to deletion logic on To name.
		shardTo := b.getShard(change.To.Name)

		return b.handleDeletion(shardTo, change, author, cache)
	} else if errFrom != nil {
		return nil
	}

	thisDiffs := diffs[change.To.Name]
	if file.Len() != thisDiffs.OldLinesOfCode {
		return fmt.Errorf("%w: %s src %d != %d",
			errInternalIntegritySource, change.To.Name, thisDiffs.OldLinesOfCode, file.Len())
	}

	b.applyDiffs(file, thisDiffs, author)

	return nil
}

// diffApplier holds state for applying a sequence of diffs to a burndown file.
type diffApplier struct {
	b        *HistoryAnalyzer
	file     *burndown.File
	author   int
	position int
	pending  diffmatchpatch.Diff
}

func (d *diffApplier) packValue() int {
	return d.b.packPersonWithTick(d.author, d.b.tick)
}

func (d *diffApplier) applySingle(edit diffmatchpatch.Diff) {
	length := utf8.RuneCountInString(edit.Text)
	if edit.Type == diffmatchpatch.DiffInsert {
		d.file.Update(d.packValue(), d.position, length, 0)
		d.position += length
	} else {
		d.file.Update(d.packValue(), d.position, 0, length)
	}

	if d.b.Debug {
		d.file.Validate()
	}
}

func (d *diffApplier) flushPending() {
	if d.pending.Text != "" {
		d.applySingle(d.pending)
		d.pending.Text = ""
	}
}

func (d *diffApplier) handleInsert(edit diffmatchpatch.Diff) {
	length := utf8.RuneCountInString(edit.Text)

	if d.pending.Text != "" {
		d.file.Update(d.packValue(), d.position, length, utf8.RuneCountInString(d.pending.Text))

		if d.b.Debug {
			d.file.Validate()
		}

		d.position += length
		d.pending.Text = ""
	} else {
		d.pending = edit
	}
}

func (b *HistoryAnalyzer) applyDiffs(
	file *burndown.File, thisDiffs pkgplumbing.FileDiffData, author int,
) {
	da := &diffApplier{b: b, file: file, author: author, pending: diffmatchpatch.Diff{Text: ""}}

	for _, edit := range thisDiffs.Diffs {
		switch edit.Type {
		case diffmatchpatch.DiffEqual:
			da.flushPending()
			da.position += utf8.RuneCountInString(edit.Text)
		case diffmatchpatch.DiffInsert:
			da.handleInsert(edit)
		case diffmatchpatch.DiffDelete:
			da.pending = edit
		}
	}

	da.flushPending()
}

// migrateFileHistory moves a file's sparse history from one shard to another during a rename.
func (b *HistoryAnalyzer) migrateFileHistory(shardFrom, shardTo *Shard, fromID, toID PathID) {
	b.ensureCapacity(shardFrom, fromID)
	b.ensureCapacity(shardTo, toID)

	history := shardFrom.fileHistoriesByID[fromID]
	if history == nil {
		history = sparseHistory{}
	}

	shardFrom.fileHistoriesByID[fromID] = nil
	shardTo.fileHistoriesByID[toID] = history
}

func (b *HistoryAnalyzer) handleRename(from, to string) error {
	if from == to {
		return nil
	}

	shardFrom := b.getShard(from)
	fromID := b.pathInterner.Intern(from)
	toID := b.pathInterner.Intern(to)
	b.ensureCapacity(shardFrom, fromID)

	file := shardFrom.filesByID[fromID]
	if file == nil {
		return fmt.Errorf("%w: %s > %s", errFileNotExist, from, to)
	}

	shardTo := b.getShard(to)
	b.ensureCapacity(shardTo, toID)

	if shardFrom == shardTo {
		shardFrom.filesByID[fromID] = nil
		b.removeActiveID(shardFrom, fromID)
		shardFrom.filesByID[toID] = file
		shardFrom.activeIDs = append(shardFrom.activeIDs, toID)
	} else {
		// Cross-shard move: deep clone timeline.
		newFile := file.CloneDeep()
		// Rebind updaters to the new shard.
		newFile.ReplaceUpdaters(b.createUpdaters(shardTo, toID))

		shardTo.filesByID[toID] = newFile
		shardTo.activeIDs = append(shardTo.activeIDs, toID)

		file.Delete()

		shardFrom.filesByID[fromID] = nil
		b.removeActiveID(shardFrom, fromID)
	}

	if b.TrackFiles {
		b.migrateFileHistory(shardFrom, shardTo, fromID, toID)
	}

	delete(shardTo.deletionsByID, toID)

	b.GlobalMu.Lock()

	if oldTo := b.renames[from]; oldTo != "" {
		delete(b.renamesReverse[oldTo], from)

		if len(b.renamesReverse[oldTo]) == 0 {
			delete(b.renamesReverse, oldTo)
		}
	}

	b.renames[from] = to
	if b.renamesReverse[to] == nil {
		b.renamesReverse[to] = map[string]bool{}
	}

	b.renamesReverse[to][from] = true
	b.GlobalMu.Unlock()

	return nil
}

func (b *HistoryAnalyzer) groupSparseHistory(
	history sparseHistory, lastTick int,
) DenseHistory {
	if len(history) == 0 {
		return DenseHistory{}
	}

	ticks, lastTick := b.normalizeTicks(history, lastTick)

	samples := lastTick/b.Sampling + 1
	bands := lastTick/b.Granularity + 1

	result := make(DenseHistory, samples)
	for i := range samples {
		result[i] = make([]int64, bands)
	}

	b.fillDenseHistory(result, ticks, history)

	return result
}

// normalizeTicks extracts sorted tick keys from a sparse history and resolves lastTick.
func (b *HistoryAnalyzer) normalizeTicks(history sparseHistory, lastTick int) (ticks []int, resolvedLastTick int) {
	ticks = make([]int, 0, len(history))
	for tick := range history {
		ticks = append(ticks, tick)
	}

	sort.Ints(ticks)

	if len(ticks) == 0 {
		return ticks, max(lastTick, 0)
	}

	if lastTick >= 0 {
		if ticks[len(ticks)-1] < lastTick {
			ticks = append(ticks, lastTick)
		}
	} else {
		lastTick = ticks[len(ticks)-1]
	}

	return ticks, lastTick
}

// fillDenseHistory populates a pre-allocated dense history from sparse tick data.
func (b *HistoryAnalyzer) fillDenseHistory(result DenseHistory, ticks []int, history sparseHistory) {
	prevsi := 0

	for _, tick := range ticks {
		si := tick / b.Sampling
		if si > prevsi {
			state := result[prevsi]
			for i := prevsi + 1; i <= si; i++ {
				copy(result[i], state)
			}

			prevsi = si
		}

		sample := result[si]
		for t, value := range history[tick] {
			if t/b.Granularity < len(sample) {
				sample[t/b.Granularity] += value
			}
		}
	}
}
