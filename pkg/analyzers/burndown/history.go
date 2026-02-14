// Package burndown provides burndown functionality.
package burndown

import (
	"encoding/json"
	"errors"
	"fmt"
	"hash/fnv"
	"io"
	"maps"
	"path/filepath"
	"runtime"
	"sort"
	"sync"
	"time"
	"unicode/utf8"

	"github.com/sergi/go-diff/diffmatchpatch"
	"gopkg.in/yaml.v3"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/reportutil"
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
	globalHistory     sparseHistory
	peopleHistories   []sparseHistory
	matrix            []map[int]int64
	mergedByID        map[PathID]bool
	deletionsByID     map[PathID]bool
	mu                sync.Mutex
}

// HistoryAnalyzer tracks line survival rates across commit history.
type HistoryAnalyzer struct {
	BlobCache            *plumbing.BlobCacheAnalyzer
	pathInterner         *PathInterner
	renames              map[string]string          // from → to.
	renamesReverse       map[string]map[string]bool // to → set of from (avoids range renames in handleDeletion).
	globalHistory        sparseHistory
	repository           *gitlib.Repository
	Ticks                *plumbing.TicksSinceStart
	Identity             *plumbing.IdentityDetector
	FileDiff             *plumbing.FileDiffAnalyzer
	TreeDiff             *plumbing.TreeDiffAnalyzer
	HibernationDirectory string
	peopleHistories      []sparseHistory
	shards               []*Shard
	reversedPeopleDict   []string
	matrix               []map[int]int64
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

type sparseHistory = map[int]map[int]int64

// DenseHistory is a two-dimensional matrix of line counts over time intervals.
type DenseHistory = [][]int64

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

// Name returns the name of the analyzer.
func (b *HistoryAnalyzer) Name() string {
	return "Burndown"
}

// Flag returns the CLI flag for the analyzer.
func (b *HistoryAnalyzer) Flag() string {
	return "burndown"
}

// Description returns a human-readable description of the analyzer.
func (b *HistoryAnalyzer) Description() string {
	return b.Descriptor().Description
}

// Descriptor returns stable analyzer metadata.
func (b *HistoryAnalyzer) Descriptor() analyze.Descriptor {
	return analyze.Descriptor{
		ID: "history/burndown",
		Description: "Line burndown stats indicate the numbers of lines which were last edited " +
			"within specific time intervals through time.",
		Mode: analyze.ModeHistory,
	}
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
	b.globalHistory = sparseHistory{}

	if b.PeopleNumber < 0 {
		return fmt.Errorf("%w: %d", errPeopleNumberNegative, b.PeopleNumber)
	}

	b.peopleHistories = make([]sparseHistory, b.PeopleNumber)

	if b.HibernationThreshold == 0 {
		b.HibernationThreshold = DefaultBurndownHibernationThreshold
	}

	if b.pathInterner == nil {
		b.pathInterner = NewPathInterner()
	}

	b.shards = make([]*Shard, b.Goroutines)
	for i := range b.Goroutines {
		b.shards[i] = &Shard{
			filesByID:         nil,
			fileHistoriesByID: nil,
			activeIDs:         nil,
			globalHistory:     sparseHistory{},
			peopleHistories:   make([]sparseHistory, b.PeopleNumber),
			matrix:            make([]map[int]int64, b.PeopleNumber),
			mergedByID:        map[PathID]bool{},
			deletionsByID:     map[PathID]bool{},
		}
	}

	b.renames = map[string]string{}
	b.renamesReverse = map[string]map[string]bool{}
	b.matrix = nil          // Aggregated in Finalize.
	b.peopleHistories = nil // Aggregated in Finalize.
	b.globalHistory = nil   // Aggregated in Finalize.
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
func (b *HistoryAnalyzer) Consume(ctx *analyze.Context) error {
	author := b.Identity.AuthorID
	tick := b.Ticks.Tick
	isMerge := ctx.IsMerge
	b.isMerge = isMerge

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
		return err
	}

	for _, change := range renames {
		renameErr := b.handleModificationRename(change, author, cache, fileDiffs)
		if renameErr != nil {
			return renameErr
		}
	}

	b.tick = tick
	b.lastCommitTime = ctx.Time

	return nil
}

// ConsumePrepared processes a pre-prepared commit.
// This is used by the pipelined runner for parallel commit preparation.
func (b *HistoryAnalyzer) ConsumePrepared(prepared *analyze.PreparedCommit) error {
	author := prepared.AuthorID
	tick := prepared.Tick
	isMerge := prepared.Ctx.IsMerge
	b.isMerge = isMerge

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

	for _, change := range renames {
		renameErr := b.handleModificationRename(change, author, cache, prepared.FileDiffs)
		if renameErr != nil {
			return renameErr
		}
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

			for _, change := range changes {
				action := change.Action

				var err error

				switch action {
				case gitlib.Insert:
					err = b.handleInsertion(shard, change, author, cache)
				case gitlib.Delete:
					err = b.handleDeletion(shard, change, author, cache)
				case gitlib.Modify:
					err = b.handleModification(shard, change, author, cache, fileDiffs)
				}

				if err != nil {
					errs[idx] = err

					return
				}
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
				filesByID:         nil,
				fileHistoriesByID: nil,
				activeIDs:         nil,
				globalHistory:     sparseHistory{},
				peopleHistories:   make([]sparseHistory, b.PeopleNumber),
				matrix:            make([]map[int]int64, b.PeopleNumber),
				mergedByID:        map[PathID]bool{},
				deletionsByID:     map[PathID]bool{},
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

// mergeShards merges shard statistics from another analyzer.
func (b *HistoryAnalyzer) mergeShards(other *HistoryAnalyzer) {
	for i, otherShard := range other.shards {
		if i >= len(b.shards) {
			break
		}

		shard := b.shards[i]

		shard.mu.Lock()
		otherShard.mu.Lock()

		b.mergeShardGlobalHistory(shard, otherShard)
		b.mergeShardPeopleHistories(shard, otherShard)
		b.mergeShardMatrix(shard, otherShard)

		otherShard.mu.Unlock()
		shard.mu.Unlock()
	}
}

// mergeShardGlobalHistory merges global history from another shard.
func (b *HistoryAnalyzer) mergeShardGlobalHistory(shard, otherShard *Shard) {
	for tick, counts := range otherShard.globalHistory {
		if shard.globalHistory[tick] == nil {
			shard.globalHistory[tick] = map[int]int64{}
		}

		for prevTick, count := range counts {
			shard.globalHistory[tick][prevTick] += count
		}
	}
}

// mergeShardPeopleHistories merges people histories from another shard.
func (b *HistoryAnalyzer) mergeShardPeopleHistories(shard, otherShard *Shard) {
	for person, history := range otherShard.peopleHistories {
		if len(history) == 0 {
			continue
		}

		if shard.peopleHistories[person] == nil {
			shard.peopleHistories[person] = sparseHistory{}
		}

		for tick, counts := range history {
			if shard.peopleHistories[person][tick] == nil {
				shard.peopleHistories[person][tick] = map[int]int64{}
			}

			for prevTick, count := range counts {
				shard.peopleHistories[person][tick][prevTick] += count
			}
		}
	}
}

// mergeShardMatrix merges the interaction matrix from another shard.
func (b *HistoryAnalyzer) mergeShardMatrix(shard, otherShard *Shard) {
	for author, row := range otherShard.matrix {
		if len(row) == 0 {
			continue
		}

		if shard.matrix[author] == nil {
			shard.matrix[author] = map[int]int64{}
		}

		for otherAuthor, count := range row {
			shard.matrix[author][otherAuthor] += count
		}
	}
}

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

// SequentialOnly returns true because burndown tracks cumulative per-file
// line state across all commits and cannot be parallelized.
func (b *HistoryAnalyzer) SequentialOnly() bool { return true }

// CPUHeavy returns false because burndown tracks line ownership without UAST processing.
func (b *HistoryAnalyzer) CPUHeavy() bool { return false }

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
// Clears per-shard tracking maps (mergedByID, deletionsByID) that are only
// needed within a chunk. Also compacts file timelines to reduce memory usage.
func (b *HistoryAnalyzer) Hibernate() error {
	for _, shard := range b.shards {
		shard.mu.Lock()

		// Clear per-commit tracking maps.
		shard.mergedByID = make(map[PathID]bool)
		shard.deletionsByID = make(map[PathID]bool)

		// Compact file timelines to reduce memory.
		// MergeAdjacentSameValue coalesces consecutive segments with the same time.
		for _, id := range shard.activeIDs {
			if int(id) < len(shard.filesByID) {
				if file := shard.filesByID[id]; file != nil {
					file.MergeAdjacentSameValue()
				}
			}
		}

		shard.mu.Unlock()
	}

	return nil
}

// Boot performs early initialization before repository processing.
// Ensures per-shard tracking maps are ready for the next chunk.
func (b *HistoryAnalyzer) Boot() error {
	for _, shard := range b.shards {
		shard.mu.Lock()

		if shard.mergedByID == nil {
			shard.mergedByID = make(map[PathID]bool)
		}

		if shard.deletionsByID == nil {
			shard.deletionsByID = make(map[PathID]bool)
		}

		shard.mu.Unlock()
	}

	return nil
}

// Finalize completes the analysis and returns the result.
func (b *HistoryAnalyzer) Finalize() (analyze.Report, error) {
	b.initAggregationState()
	b.aggregateShards()

	globalHistory, lastTick := b.groupSparseHistory(b.globalHistory, -1)
	fileHistories, fileOwnership := b.collectFileHistories(lastTick)
	peopleHistories := b.buildPeopleHistories(globalHistory, lastTick)
	peopleMatrix := b.buildPeopleMatrix()

	projectName := "project"
	if b.repository != nil && b.repository.Path() != "" {
		projectName = filepath.Base(b.repository.Path())
	}

	report := analyze.Report{
		"GlobalHistory":      globalHistory,
		"FileHistories":      fileHistories,
		"FileOwnership":      fileOwnership,
		"PeopleHistories":    peopleHistories,
		"PeopleMatrix":       peopleMatrix,
		"TickSize":           b.TickSize,
		"ReversedPeopleDict": b.reversedPeopleDict,
		"Sampling":           b.Sampling,
		"Granularity":        b.Granularity,
		"ProjectName":        projectName,
	}
	if !b.lastCommitTime.IsZero() {
		report["EndTime"] = b.lastCommitTime
	}

	return report, nil
}

// initAggregationState initializes the aggregation state for Finalize.
func (b *HistoryAnalyzer) initAggregationState() {
	b.globalHistory = sparseHistory{}
	b.peopleHistories = make([]sparseHistory, b.PeopleNumber)
	b.matrix = make([]map[int]int64, b.PeopleNumber)
}

// aggregateShards merges all shard data into the main analyzer state.
func (b *HistoryAnalyzer) aggregateShards() {
	for _, shard := range b.shards {
		b.mergeGlobalHistory(shard)
		b.mergePeopleHistories(shard)
		b.mergeMatrix(shard)
	}
}

// mergeGlobalHistory merges a shard's global history into the main state.
func (b *HistoryAnalyzer) mergeGlobalHistory(shard *Shard) {
	for tick, counts := range shard.globalHistory {
		if b.globalHistory[tick] == nil {
			b.globalHistory[tick] = map[int]int64{}
		}

		for prevTick, count := range counts {
			b.globalHistory[tick][prevTick] += count
		}
	}
}

// mergePeopleHistories merges a shard's people histories into the main state.
func (b *HistoryAnalyzer) mergePeopleHistories(shard *Shard) {
	for person, history := range shard.peopleHistories {
		if len(history) == 0 {
			continue
		}

		if b.peopleHistories[person] == nil {
			b.peopleHistories[person] = sparseHistory{}
		}

		for tick, counts := range history {
			if b.peopleHistories[person][tick] == nil {
				b.peopleHistories[person][tick] = map[int]int64{}
			}

			for prevTick, count := range counts {
				b.peopleHistories[person][tick][prevTick] += count
			}
		}
	}
}

// mergeMatrix merges a shard's matrix into the main state.
func (b *HistoryAnalyzer) mergeMatrix(shard *Shard) {
	for author, row := range shard.matrix {
		if len(row) == 0 {
			continue
		}

		if b.matrix[author] == nil {
			b.matrix[author] = map[int]int64{}
		}

		for otherAuthor, count := range row {
			b.matrix[author][otherAuthor] += count
		}
	}
}

// collectFileHistories builds dense file histories and ownership maps from all shards.
// Iterates over activeIDs (slice) per shard instead of map iteration (Track B).
func (b *HistoryAnalyzer) collectFileHistories(lastTick int) (histories map[string]DenseHistory, owners map[string]map[int]int) {
	fileHistories := map[string]DenseHistory{}
	fileOwnership := map[string]map[int]int{}

	for _, shard := range b.shards {
		for _, id := range shard.activeIDs {
			if int(id) >= len(shard.fileHistoriesByID) || int(id) >= len(shard.filesByID) {
				continue
			}

			history := shard.fileHistoriesByID[id]
			if len(history) == 0 {
				continue
			}

			key := b.pathInterner.Lookup(id)
			fileHistories[key], _ = b.groupSparseHistory(history, lastTick)

			file := shard.filesByID[id]
			if file == nil {
				continue
			}

			previousLine := 0
			previousAuthor := identity.AuthorMissing
			ownership := map[int]int{}
			fileOwnership[key] = ownership

			file.ForEach(func(line, value int) {
				length := line - previousLine
				if length > 0 {
					ownership[previousAuthor] += length
				}

				previousLine = line

				previousAuthor, _ = b.unpackPersonWithTick(value)
				if previousAuthor == identity.AuthorMissing {
					previousAuthor = -1
				}
			})
		}
	}

	return fileHistories, fileOwnership
}

// buildPeopleHistories constructs dense histories for each person.
func (b *HistoryAnalyzer) buildPeopleHistories(globalHistory DenseHistory, lastTick int) []DenseHistory {
	peopleHistories := make([]DenseHistory, b.PeopleNumber)

	for i, history := range b.peopleHistories {
		if len(history) > 0 {
			peopleHistories[i], _ = b.groupSparseHistory(history, lastTick)
		} else {
			peopleHistories[i] = make(DenseHistory, len(globalHistory))
			for j, gh := range globalHistory {
				peopleHistories[i][j] = make([]int64, len(gh))
			}
		}
	}

	return peopleHistories
}

// buildPeopleMatrix constructs the people interaction matrix.
func (b *HistoryAnalyzer) buildPeopleMatrix() DenseHistory {
	if len(b.matrix) == 0 {
		return nil
	}

	peopleMatrix := make(DenseHistory, b.PeopleNumber)

	for i, row := range b.matrix {
		mrow := make([]int64, b.PeopleNumber+mrowValue)
		peopleMatrix[i] = mrow

		for key, val := range row {
			switch key {
			case identity.AuthorMissing:
				key = -1
			case authorSelf:
				key = -keyValue
			}

			mrow[key+2] = val
		}
	}

	return peopleMatrix
}

// Serialize writes the analysis result to the given writer.
func (b *HistoryAnalyzer) Serialize(result analyze.Report, format string, writer io.Writer) error {
	switch format {
	case analyze.FormatJSON:
		return b.serializeJSON(result, writer)
	case analyze.FormatYAML:
		return b.serializeYAML(result, writer)
	case analyze.FormatPlot:
		return b.generatePlot(result, writer)
	case analyze.FormatBinary:
		return b.serializeBinary(result, writer)
	default:
		return fmt.Errorf("%w: %s", analyze.ErrUnsupportedFormat, format)
	}
}

func (b *HistoryAnalyzer) serializeJSON(result analyze.Report, writer io.Writer) error {
	metrics, err := ComputeAllMetrics(result)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

	err = json.NewEncoder(writer).Encode(metrics)
	if err != nil {
		return fmt.Errorf("json encode: %w", err)
	}

	return nil
}

func (b *HistoryAnalyzer) serializeYAML(result analyze.Report, writer io.Writer) error {
	metrics, err := ComputeAllMetrics(result)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

	data, err := yaml.Marshal(metrics)
	if err != nil {
		return fmt.Errorf("yaml marshal: %w", err)
	}

	_, err = writer.Write(data)
	if err != nil {
		return fmt.Errorf("yaml write: %w", err)
	}

	return nil
}

func (b *HistoryAnalyzer) serializeBinary(result analyze.Report, writer io.Writer) error {
	metrics, err := ComputeAllMetrics(result)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

	err = reportutil.EncodeBinaryEnvelope(metrics, writer)
	if err != nil {
		return fmt.Errorf("binary encode: %w", err)
	}

	return nil
}

// FormatReport writes the formatted analysis report to the given writer.
func (b *HistoryAnalyzer) FormatReport(report analyze.Report, writer io.Writer) error {
	return b.Serialize(report, analyze.FormatYAML, writer)
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

func (b *HistoryAnalyzer) updateGlobal(shard *Shard, currentTime, previousTime, delta int) {
	_, curTick := b.unpackPersonWithTick(currentTime)
	_, prevTick := b.unpackPersonWithTick(previousTime)

	currentHistory := shard.globalHistory[curTick]
	if currentHistory == nil {
		currentHistory = map[int]int64{}
		shard.globalHistory[curTick] = currentHistory
	}

	currentHistory[prevTick] += int64(delta)
}

func (b *HistoryAnalyzer) updateFile(history sparseHistory, currentTime, previousTime, delta int) {
	_, curTick := b.unpackPersonWithTick(currentTime)
	_, prevTick := b.unpackPersonWithTick(previousTime)

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

	history := shard.peopleHistories[previousAuthor]
	if history == nil {
		history = sparseHistory{}
		shard.peopleHistories[previousAuthor] = history
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

	row := shard.matrix[oldAuthor]
	if row == nil {
		row = map[int]int64{}
		shard.matrix[oldAuthor] = row
	}

	cell, exists := row[newAuthor]
	if !exists {
		row[newAuthor] = 0
		cell = 0
	}

	row[newAuthor] = cell + int64(delta)
}

func (b *HistoryAnalyzer) createUpdaters(shard *Shard, pathID PathID) []burndown.Updater {
	const maxUpdaters = 4 // global + file + author + matrix.

	updaters := make([]burndown.Updater, 0, maxUpdaters)

	updaters = append(updaters, func(currentTime, previousTime, delta int) {
		b.updateGlobal(shard, currentTime, previousTime, delta)
	})

	if b.TrackFiles {
		history := shard.fileHistoriesByID[pathID]
		if history == nil {
			history = sparseHistory{}
			shard.fileHistoriesByID[pathID] = history
		}

		updaters = append(updaters, func(currentTime, previousTime, delta int) {
			b.updateFile(history, currentTime, previousTime, delta)
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
) (grouped DenseHistory, finalTick int) {
	if len(history) == 0 {
		return DenseHistory{}, lastTick
	}

	ticks, lastTick := b.normalizeTicks(history, lastTick)

	samples := lastTick/b.Sampling + 1
	bands := lastTick/b.Granularity + 1

	result := make(DenseHistory, samples)
	for i := range samples {
		result[i] = make([]int64, bands)
	}

	b.fillDenseHistory(result, ticks, history)

	return result, lastTick
}

// normalizeTicks extracts sorted tick keys from a sparse history and resolves lastTick.
func (b *HistoryAnalyzer) normalizeTicks(history sparseHistory, lastTick int) (ticks []int, resolvedLastTick int) {
	ticks = make([]int, 0, len(history))
	for tick := range history {
		ticks = append(ticks, tick)
	}

	sort.Ints(ticks)

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
