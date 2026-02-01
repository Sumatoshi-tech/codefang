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

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/burndown"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/identity"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
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
	l interface {
		Warnf(format string, args ...any)
		Errorf(format string, args ...any)
		Infof(format string, args ...any)
	}
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
	return "Line burndown stats indicate the numbers of lines which were last edited within specific time intervals through time."
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
		return fmt.Errorf("PeopleNumber is negative: %d", val)
	}

	b.PeopleNumber = val

	rpd, ok := facts[identity.FactIdentityDetectorReversedPeopleDict].([]string)
	if !ok {
		return errors.New("expected []string for reversedPeopleDict")
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
		return fmt.Errorf("PeopleNumber is negative: %d", b.PeopleNumber)
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
func (b *HistoryAnalyzer) Fork(_ int) []analyze.HistoryAnalyzer {
	panic("Fork not implemented yet")
}

// Merge combines results from forked analyzer branches.
func (b *HistoryAnalyzer) Merge(_ []analyze.HistoryAnalyzer) {
	panic("Merge not implemented yet")
}

// Hibernate releases resources between processing phases.
// Hibernation of timeline allocators is no-op when using the default treap timeline.
func (b *HistoryAnalyzer) Hibernate() error {
	return nil
}

// Boot performs early initialization before repository processing.
// No-op when using the default treap timeline (no allocator to boot).
func (b *HistoryAnalyzer) Boot() error {
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
		reportKeyProjectName: projectName,
	}
	if !b.lastCommitTime.IsZero() {
		report[reportKeyEndTime] = b.lastCommitTime
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

				previousAuthor, _ = b.unpackPersonWithTick(int(value)) //nolint:unconvert // conversion is needed for type safety.
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
			if key == identity.AuthorMissing { //nolint:staticcheck // QF1003 is acceptable.
				key = -1
			} else if key == authorSelf {
				key = -keyValue
			}

			mrow[key+2] = val
		}
	}

	return peopleMatrix
}

// Serialize writes the analysis result to the given writer.
func (b *HistoryAnalyzer) Serialize(result analyze.Report, format string, writer io.Writer) error {
	if format == analyze.FormatPlot {
		return b.generatePlot(result, writer)
	}

	enc := json.NewEncoder(writer)

	// For YAML format, use indentation for readability (burndown default is JSON-like).
	if format == analyze.FormatYAML {
		enc.SetIndent("", "  ")
	}

	err := enc.Encode(result)
	if err != nil {
		return fmt.Errorf("serialize: %w", err)
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
	updaters := make([]burndown.Updater, 1)

	updaters[0] = func(currentTime, previousTime, delta int) {
		b.updateGlobal(shard, currentTime, previousTime, delta)
	}

	if b.TrackFiles {
		history := shard.fileHistoriesByID[pathID]
		if history == nil {
			history = sparseHistory{}
			shard.fileHistoriesByID[pathID] = history
		}

		updaters = append(updaters, func(currentTime, previousTime, delta int) { //nolint:makezero // zero-length init is intentional.
			b.updateFile(history, currentTime, previousTime, delta)
		})
	}

	if b.PeopleNumber > 0 {
		updaters = append(updaters, func(currentTime, previousTime, delta int) { //nolint:makezero // zero-length init is intentional.
			b.updateAuthor(shard, currentTime, previousTime, delta)
		}, func(currentTime, previousTime, delta int) {
			b.updateMatrix(shard, currentTime, previousTime, delta)
		})
	}

	return updaters
}

func (b *HistoryAnalyzer) newFile(
	shard *Shard, _ gitlib.Hash, pathID PathID, author int, tick int, size int,
) (*burndown.File, error) { //nolint:unparam // short name is clear in context.
	updaters := b.createUpdaters(shard, pathID)

	if b.PeopleNumber > 0 {
		tick = b.packPersonWithTick(author, tick)
	}

	return burndown.NewFile(tick, size, updaters...), nil
}

func (b *HistoryAnalyzer) handleInsertion(
	shard *Shard, change *gitlib.Change, author int, cache map[gitlib.Hash]*pkgplumbing.CachedBlob,
) error {
	blob := cache[change.To.Hash]
	if blob == nil {
		return fmt.Errorf("missing blob for insertion %s (%s)", change.To.Name, change.To.Hash)
	}

	lines, err := blob.CountLines()
	if err != nil {
		return nil //nolint:nilerr // nil error return is intentional.
	}

	name := change.To.Name
	id := b.pathInterner.Intern(name)
	b.ensureCapacity(shard, id)
	if shard.filesByID[id] != nil {
		return fmt.Errorf("file %s already exists", name)
	}

	var hash gitlib.Hash
	if !b.isMerge {
		hash = blob.Hash()
	}

	file, err := b.newFile(shard, hash, id, author, b.tick, lines)
	shard.filesByID[id] = file
	shard.activeIDs = append(shard.activeIDs, id)

	delete(shard.deletionsByID, id)

	if b.isMerge {
		shard.mergedByID[id] = true
	}

	return err
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
		return fmt.Errorf("missing blob for deletion %s (%s)", name, change.From.Hash)
	}

	lines, err := blob.CountLines()
	if err != nil {
		return fmt.Errorf("previous version of %s unexpectedly became binary", name)
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
			b.renames[child] = "" // clear so when child is popped we don't use stale oldTo
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
		return fmt.Errorf("missing blobFrom for modification %s (%s)", change.From.Name, change.From.Hash)
	}
	_, errFrom := blobFrom.CountLines()
	blobTo := cache[change.To.Hash]
	if blobTo == nil {
		return fmt.Errorf("missing blobTo for modification %s (%s)", change.To.Name, change.To.Hash)
	}

	_, errTo := blobTo.CountLines()
	if !errors.Is(errFrom, errTo) { // Error comparison is intentional.
		if errFrom != nil {
			return b.handleInsertion(shard, change, author, cache)
		}

		return b.handleDeletion(shard, change, author, cache)
	} else if errFrom != nil {
		return nil //nolint:nilerr // nil error return is intentional.
	}

	thisDiffs := diffs[change.To.Name]
	if file.Len() != thisDiffs.OldLinesOfCode {
		return fmt.Errorf("%s: internal integrity error src %d != %d",
			change.To.Name, thisDiffs.OldLinesOfCode, file.Len())
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
		return fmt.Errorf("missing blobFrom for rename %s (%s)", change.From.Name, change.From.Hash)
	}
	_, errFrom := blobFrom.CountLines()
	blobTo := cache[change.To.Hash]
	if blobTo == nil {
		return fmt.Errorf("missing blobTo for rename %s (%s)", change.To.Name, change.To.Hash)
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
		return nil //nolint:nilerr // nil error return is intentional.
	}

	thisDiffs := diffs[change.To.Name]
	if file.Len() != thisDiffs.OldLinesOfCode {
		return fmt.Errorf("%s: internal integrity error src %d != %d",
			change.To.Name, thisDiffs.OldLinesOfCode, file.Len())
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
		return fmt.Errorf("file %s > %s does not exist", from, to)
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
