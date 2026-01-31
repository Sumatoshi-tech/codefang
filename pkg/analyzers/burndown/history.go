// Package burndown provides burndown functionality.
package burndown

import (
	"encoding/json"
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
	"github.com/Sumatoshi-tech/codefang/pkg/rbtree"
)

// Configuration constants for burndown analysis.
const (
	// TickSizeThresholdHigh is the maximum tick size in hours for burndown granularity.
	TickSizeThresholdHigh = 24
	keyValue              = 2
	mrowValue             = 2
)

// Shard holds per-file burndown data within a partition.
type Shard struct {
	files           map[string]*burndown.File
	fileHistories   map[string]sparseHistory
	allocator       *rbtree.Allocator
	globalHistory   sparseHistory
	peopleHistories []sparseHistory
	matrix          []map[int]int64
	mergedFiles     map[string]bool
	deletions       map[string]bool
	mu              sync.Mutex //nolint:unused // acknowledged.
}

// HistoryAnalyzer tracks line survival rates across commit history.
type HistoryAnalyzer struct {
	l interface { //nolint:unused // used via dependency injection.
		Warnf(format string, args ...any)
		Errorf(format string, args ...any)
		Infof(format string, args ...any)
	}
	BlobCache            *plumbing.BlobCacheAnalyzer
	renames              map[string]string
	shardedAllocator     *rbtree.ShardedAllocator
	globalHistory        sparseHistory
	repository           *gitlib.Repository
	Ticks                *plumbing.TicksSinceStart
	Identity             *plumbing.IdentityDetector
	FileDiff             *plumbing.FileDiffAnalyzer
	TreeDiff             *plumbing.TreeDiffAnalyzer
	HibernationDirectory string
	hibernatedFileName   string
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
	DefaultBurndownSampling = 1
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
			Description: "Save hibernated RBTree allocators to disk rather than keep it in memory.",
			Flag:        "burndown-hibernation-disk",
			Type:        pipeline.BoolConfigurationOption,
			Default:     true,
		},
		{
			Name:        ConfigBurndownHibernationDirectory,
			Description: "Temporary directory where to save the hibernated RBTree allocators.",
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
		return fmt.Errorf("PeopleNumber is negative: %d", val) //nolint:err113 // dynamic error is acceptable here.
	}

	b.PeopleNumber = val

	rpd, ok := facts[identity.FactIdentityDetectorReversedPeopleDict].([]string)
	if !ok {
		return errors.New("expected []string for reversedPeopleDict") //nolint:err113 // type assertion error.
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
		return fmt.Errorf("PeopleNumber is negative: %d", b.PeopleNumber) //nolint:err113 // dynamic error is acceptable here.
	}

	b.peopleHistories = make([]sparseHistory, b.PeopleNumber)

	if b.HibernationThreshold == 0 {
		b.HibernationThreshold = DefaultBurndownHibernationThreshold
	}

	if b.shardedAllocator == nil {
		b.shardedAllocator = rbtree.NewShardedAllocator(b.Goroutines, b.HibernationThreshold)
	}

	b.shards = make([]*Shard, b.Goroutines)

	allocators := b.shardedAllocator.Shards()
	for i := range b.Goroutines {
		b.shards[i] = &Shard{
			files:           map[string]*burndown.File{},
			fileHistories:   map[string]sparseHistory{},
			allocator:       allocators[i],
			globalHistory:   sparseHistory{},
			peopleHistories: make([]sparseHistory, b.PeopleNumber),
			matrix:          make([]map[int]int64, b.PeopleNumber),
			mergedFiles:     map[string]bool{},
			deletions:       map[string]bool{},
		}
	}

	b.renames = map[string]string{}
	b.matrix = nil          // Aggregated in Finalize
	b.peopleHistories = nil // Aggregated in Finalize
	b.globalHistory = nil   // Aggregated in Finalize
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
			shard.mergedFiles = map[string]bool{}
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
			shard.mergedFiles = map[string]bool{}
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
//
//nolint:gocognit // complexity is inherent to parallel shard coordination logic.
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
	// Fork is used for branching logic.
	// Since ShardedAllocator doesn't support cloning yet, we panic.
	panic("Fork not implemented for ShardedAllocator yet")
}

// Merge combines results from forked analyzer branches.
func (b *HistoryAnalyzer) Merge(_ []analyze.HistoryAnalyzer) {
	panic("Merge not implemented for ShardedAllocator yet")
}

// Hibernate releases resources between processing phases.
func (b *HistoryAnalyzer) Hibernate() error {
	b.shardedAllocator.Hibernate()

	if !b.HibernationToDisk {
		return nil
	}

	return b.hibernateToDisk()
}

// hibernateToDisk serializes the sharded allocator to a temporary file on disk.
func (b *HistoryAnalyzer) hibernateToDisk() error {
	file, err := os.CreateTemp(b.HibernationDirectory, "*-codefang.bin")
	if err != nil {
		return fmt.Errorf("hibernate: %w", err)
	}

	b.hibernatedFileName = file.Name()

	err = file.Close()
	if err != nil {
		b.hibernatedFileName = ""

		return fmt.Errorf("hibernate: %w", err)
	}

	// Clean up the temp file as Serialize will create its own files with suffix.
	err = os.Remove(b.hibernatedFileName)
	if err != nil {
		b.hibernatedFileName = ""

		return fmt.Errorf("hibernate: %w", err)
	}

	err = b.shardedAllocator.Serialize(b.hibernatedFileName)
	if err != nil {
		b.hibernatedFileName = ""

		return err
	}

	return nil
}

// Boot performs early initialization before repository processing.
func (b *HistoryAnalyzer) Boot() error {
	if b.hibernatedFileName != "" {
		err := b.shardedAllocator.Deserialize(b.hibernatedFileName)
		if err != nil {
			return err
		}
		// Cleanup happens implicitly if user deletes the files?
		// Or we should clean up here?
		// The original code: err = os.Remove(b.hibernatedFileName)
		// Now we have .shard.N files.
		for i := range len(b.shards) {
			_ = os.Remove(fmt.Sprintf("%s.shard.%d", b.hibernatedFileName, i))
		}

		b.hibernatedFileName = ""
	}

	b.shardedAllocator.Boot()

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

	return analyze.Report{
		"GlobalHistory":      globalHistory,
		"FileHistories":      fileHistories,
		"FileOwnership":      fileOwnership,
		"PeopleHistories":    peopleHistories,
		"PeopleMatrix":       peopleMatrix,
		"TickSize":           b.TickSize,
		"ReversedPeopleDict": b.reversedPeopleDict,
		"Sampling":           b.Sampling,
		"Granularity":        b.Granularity,
	}, nil
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
func (b *HistoryAnalyzer) collectFileHistories(lastTick int) (histories map[string]DenseHistory, owners map[string]map[int]int) {
	fileHistories := map[string]DenseHistory{}
	fileOwnership := map[string]map[int]int{}

	for _, shard := range b.shards {
		for key, history := range shard.fileHistories {
			if len(history) == 0 {
				continue
			}

			fileHistories[key], _ = b.groupSparseHistory(history, lastTick)
			file := shard.files[key]
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

func (b *HistoryAnalyzer) createUpdaters(shard *Shard, name string) []burndown.Updater {
	updaters := make([]burndown.Updater, 1)

	updaters[0] = func(currentTime, previousTime, delta int) {
		b.updateGlobal(shard, currentTime, previousTime, delta)
	}

	if b.TrackFiles {
		history := shard.fileHistories[name]
		if history == nil {
			history = sparseHistory{}
			shard.fileHistories[name] = history
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
	shard *Shard, _ gitlib.Hash, name string, author int, tick int, size int,
) (*burndown.File, error) { //nolint:unparam // short name is clear in context.
	updaters := b.createUpdaters(shard, name)

	if b.PeopleNumber > 0 {
		tick = b.packPersonWithTick(author, tick)
	}

	return burndown.NewFile(tick, size, shard.allocator, updaters...), nil
}

func (b *HistoryAnalyzer) handleInsertion(
	shard *Shard, change *gitlib.Change, author int, cache map[gitlib.Hash]*pkgplumbing.CachedBlob,
) error {
	blob := cache[change.To.Hash]

	lines, err := blob.CountLines()
	if err != nil {
		return nil //nolint:nilerr // nil error return is intentional.
	}

	name := change.To.Name

	file, exists := shard.files[name] //nolint:ineffassign,staticcheck,wastedassign // assignment is needed for clarity.
	if exists {
		return fmt.Errorf("file %s already exists", name) //nolint:err113 // dynamic error is acceptable here.
	}

	var hash gitlib.Hash
	if !b.isMerge {
		hash = blob.Hash()
	}

	file, err = b.newFile(shard, hash, name, author, b.tick, lines)
	shard.files[name] = file

	// Renames and deletions maps also need protection or sharding?
	// Deletions is map[string]bool. Used for special logic in handleDeletion.
	// We can shard it too or use sync map.
	// Since deletions is accessed by filename, and we shard by filename, we can put it in shard.
	// But struct doesn't have deletions map in shard yet.
	// For now, let's use GlobalMu for deletions/renames maps access in these methods if they are not heavily contended or shard them.
	// Renames are global. Deletions are global.
	// But deletions[name] is only accessed when processing 'name'.
	// So if we shard deletions map...
	// For now, lock GlobalMu.
	delete(shard.deletions, name)

	if b.isMerge {
		shard.mergedFiles[name] = true
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

	file, exists := shard.files[name]
	if !exists {
		return nil
	}

	blob := cache[change.From.Hash]

	lines, err := blob.CountLines()
	if err != nil {
		return fmt.Errorf("previous version of %s unexpectedly became binary", name) //nolint:err113 // dynamic error is acceptable here.
	}

	tick := b.tick

	isDeletion := shard.deletions[name]
	shard.deletions[name] = true

	if b.isMerge && !isDeletion {
		tick = 0
	}

	file.Update(b.packPersonWithTick(author, tick), 0, 0, lines)
	file.Delete()
	delete(shard.files, name)
	delete(shard.fileHistories, name)

	stack := []string{name}

	b.GlobalMu.Lock()

	for len(stack) > 0 {
		head := stack[len(stack)-1]
		stack = stack[:len(stack)-1]

		b.renames[head] = ""
		for key, val := range b.renames {
			if val == head {
				stack = append(stack, key)
			}
		}
	}

	b.GlobalMu.Unlock()

	if b.isMerge {
		shard.mergedFiles[name] = false
	}

	return nil
}

func (b *HistoryAnalyzer) handleModification(
	shard *Shard, change *gitlib.Change, author int,
	cache map[gitlib.Hash]*pkgplumbing.CachedBlob, diffs map[string]pkgplumbing.FileDiffData,
) error {
	// This method handles modification WITHOUT rename (checked in Consume).
	if b.isMerge {
		shard.mergedFiles[change.To.Name] = true
	}

	file, exists := shard.files[change.From.Name]
	if !exists {
		return b.handleInsertion(shard, change, author, cache)
	}

	blobFrom := cache[change.From.Hash]
	_, errFrom := blobFrom.CountLines()
	blobTo := cache[change.To.Hash]

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
		//nolint:err113 // dynamic error is acceptable here.
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

	file, exists := shardFrom.files[change.From.Name]
	if !exists {
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
		file = shardTo.files[change.To.Name]
	}

	blobFrom := cache[change.From.Hash]
	_, errFrom := blobFrom.CountLines()
	blobTo := cache[change.To.Hash]

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
		//nolint:err113 // dynamic error is acceptable here.
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
func (b *HistoryAnalyzer) migrateFileHistory(shardFrom, shardTo *Shard, from, to string) {
	history := shardFrom.fileHistories[from]
	if history == nil {
		history = sparseHistory{}
	}

	delete(shardFrom.fileHistories, from)
	shardTo.fileHistories[to] = history
}

func (b *HistoryAnalyzer) handleRename(from, to string) error {
	if from == to {
		return nil
	}

	shardFrom := b.getShard(from)

	file, exists := shardFrom.files[from]
	if !exists {
		return fmt.Errorf("file %s > %s does not exist", from, to) //nolint:err113 // dynamic error is acceptable here.
	}

	shardTo := b.getShard(to)

	if shardFrom == shardTo {
		delete(shardFrom.files, from)
		shardFrom.files[to] = file
	} else {
		// Cross-shard move: deep clone to new allocator.
		newFile := file.CloneDeep(shardTo.allocator)
		// Rebind updaters to the new shard.
		newFile.ReplaceUpdaters(b.createUpdaters(shardTo, to))

		shardTo.files[to] = newFile

		file.Delete()
		delete(shardFrom.files, from)
	}

	if b.TrackFiles {
		b.migrateFileHistory(shardFrom, shardTo, from, to)
	}

	delete(shardTo.deletions, to)

	b.GlobalMu.Lock()
	b.renames[from] = to
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
