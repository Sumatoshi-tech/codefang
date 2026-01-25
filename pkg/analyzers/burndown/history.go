package burndown

import (
	"encoding/json"
	"fmt"
	"hash/fnv"
	"io"
	"io/ioutil"
	"os"
	"sort"
	"sync"
	"time"
	"unicode/utf8"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/burndown"
	"github.com/Sumatoshi-tech/codefang/pkg/identity"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/rbtree"
	"github.com/go-git/go-git/v6"
	gitplumbing "github.com/go-git/go-git/v6/plumbing"
	"github.com/go-git/go-git/v6/plumbing/object"
	"github.com/go-git/go-git/v6/utils/merkletrie"
	"github.com/sergi/go-diff/diffmatchpatch"
)

type BurndownShard struct {
	files         map[string]*burndown.File
	fileHistories map[string]sparseHistory
	allocator     *rbtree.Allocator
	mu            sync.Mutex
}

type BurndownHistoryAnalyzer struct {
	// Configuration
	Granularity          int
	Sampling             int
	TrackFiles           bool
	PeopleNumber         int
	TickSize             time.Duration
	HibernationThreshold int
	HibernationToDisk    bool
	HibernationDirectory string
	Debug                bool
	Goroutines           int

	// Dependencies
	FileDiff  *plumbing.FileDiffAnalyzer
	TreeDiff  *plumbing.TreeDiffAnalyzer
	BlobCache *plumbing.BlobCacheAnalyzer
	Identity  *plumbing.IdentityDetector
	Ticks     *plumbing.TicksSinceStart

	// State
	repository      *git.Repository
	globalHistory   sparseHistory
	peopleHistories []sparseHistory

	shardedAllocator *rbtree.ShardedAllocator
	shards           []*BurndownShard
	GlobalMu         sync.Mutex

	hibernatedFileName string
	mergedFiles        map[string]bool
	mergedAuthor       int
	renames            map[string]string
	deletions          map[string]bool
	matrix             []map[int]int64
	tick               int
	previousTick       int
	reversedPeopleDict []string

	// Internal
	l interface {
		Warnf(format string, args ...interface{})
		Errorf(format string, args ...interface{})
		Infof(format string, args ...interface{})
	}
}

type sparseHistory = map[int]map[int]int64
type DenseHistory = [][]int64

const (
	ConfigBurndownGranularity          = "Burndown.Granularity"
	ConfigBurndownSampling             = "Burndown.Sampling"
	ConfigBurndownTrackFiles           = "Burndown.TrackFiles"
	ConfigBurndownTrackPeople          = "Burndown.TrackPeople"
	ConfigBurndownHibernationThreshold = "Burndown.HibernationThreshold"
	ConfigBurndownHibernationToDisk    = "Burndown.HibernationOnDisk"
	ConfigBurndownHibernationDirectory = "Burndown.HibernationDirectory"
	ConfigBurndownDebug                = "Burndown.Debug"
	ConfigBurndownGoroutines           = "Burndown.Goroutines"
	DefaultBurndownGranularity         = 30
	authorSelf                         = identity.AuthorMissing - 1
)

func (b *BurndownHistoryAnalyzer) Name() string {
	return "Burndown"
}

func (b *BurndownHistoryAnalyzer) Flag() string {
	return "burndown"
}

func (b *BurndownHistoryAnalyzer) Description() string {
	return "Line burndown stats indicate the numbers of lines which were last edited within specific time intervals through time."
}

func (b *BurndownHistoryAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
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
			Default:     DefaultBurndownGranularity,
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
			Default:     0,
		},
		{
			Name:        ConfigBurndownHibernationToDisk,
			Description: "Save hibernated RBTree allocators to disk rather than keep it in memory.",
			Flag:        "burndown-hibernation-disk",
			Type:        pipeline.BoolConfigurationOption,
			Default:     false,
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
			Default:     4, // Reasonable default
		},
	}
}

func (b *BurndownHistoryAnalyzer) Configure(facts map[string]interface{}) error {
	if val, exists := facts[ConfigBurndownGranularity].(int); exists {
		b.Granularity = val
	}
	if val, exists := facts[ConfigBurndownSampling].(int); exists {
		b.Sampling = val
	}
	if val, exists := facts[ConfigBurndownTrackFiles].(bool); exists {
		b.TrackFiles = val
	}
	if people, exists := facts[ConfigBurndownTrackPeople].(bool); people && exists {
		if val, exists := facts[identity.FactIdentityDetectorPeopleCount].(int); exists {
			if val < 0 {
				return fmt.Errorf("PeopleNumber is negative: %d", val)
			}
			b.PeopleNumber = val
			b.reversedPeopleDict = facts[identity.FactIdentityDetectorReversedPeopleDict].([]string)
		}
	}
	if val, exists := facts[ConfigBurndownHibernationThreshold].(int); exists {
		b.HibernationThreshold = val
	}
	if val, exists := facts[ConfigBurndownHibernationToDisk].(bool); exists {
		b.HibernationToDisk = val
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

func (b *BurndownHistoryAnalyzer) Initialize(repository *git.Repository) error {
	if b.Granularity <= 0 {
		b.Granularity = DefaultBurndownGranularity
	}
	if b.Sampling <= 0 {
		b.Sampling = DefaultBurndownGranularity
	}
	if b.Sampling > b.Granularity {
		b.Sampling = b.Granularity
	}
	if b.TickSize == 0 {
		b.TickSize = 24 * time.Hour
	}
	if b.Goroutines <= 0 {
		b.Goroutines = 4
	}
	b.repository = repository
	b.globalHistory = sparseHistory{}

	if b.PeopleNumber < 0 {
		return fmt.Errorf("PeopleNumber is negative: %d", b.PeopleNumber)
	}
	b.peopleHistories = make([]sparseHistory, b.PeopleNumber)

	b.shardedAllocator = rbtree.NewShardedAllocator(b.Goroutines, b.HibernationThreshold)
	b.shards = make([]*BurndownShard, b.Goroutines)
	allocators := b.shardedAllocator.Shards()
	for i := 0; i < b.Goroutines; i++ {
		b.shards[i] = &BurndownShard{
			files:         map[string]*burndown.File{},
			fileHistories: map[string]sparseHistory{},
			allocator:     allocators[i],
		}
	}

	b.mergedFiles = map[string]bool{}
	b.mergedAuthor = identity.AuthorMissing
	b.renames = map[string]string{}
	b.deletions = map[string]bool{}
	b.matrix = make([]map[int]int64, b.PeopleNumber)
	b.tick = 0
	b.previousTick = 0
	return nil
}

// getShard returns the shard for a given file name.
func (b *BurndownHistoryAnalyzer) getShard(name string) *BurndownShard {
	return b.shards[b.getShardIndex(name)]
}

func (b *BurndownHistoryAnalyzer) getShardIndex(name string) int {
	h := fnv.New32a()
	h.Write([]byte(name))
	idx := int(h.Sum32()) % len(b.shards)
	if idx < 0 {
		idx = -idx
	}
	return idx
}

func (b *BurndownHistoryAnalyzer) Consume(ctx *analyze.Context) error {
	// Check if any shard is hibernated (checking first one as proxy or check all?)
	// Hibernation state is managed by ShardedAllocator mostly.
	// But b.shards[0].files is map.
	// Original check: if b.fileAllocator.Size() == 0 && len(b.files) > 0

	// We can check one shard or ShardedAllocator
	// ShardedAllocator doesn't expose total size easily.
	// Assume consistent state.

	author := b.Identity.AuthorID
	tick := b.Ticks.Tick
	isMerge := ctx.IsMerge

	if !isMerge {
		b.tick = tick
		b.onNewTick()
	} else {
		b.tick = burndown.TreeMergeMark
		b.mergedFiles = map[string]bool{}
		b.mergedAuthor = author
	}

	cache := b.BlobCache.Cache
	treeDiffs := b.TreeDiff.Changes
	fileDiffs := b.FileDiff.FileDiffs

	// Group changes by shard
	shardChanges := make([][]*object.Change, b.Goroutines)
	renames := make([]*object.Change, 0)

	for _, change := range treeDiffs {
		action, _ := change.Action()

		if action == merkletrie.Modify && change.From.Name != change.To.Name {
			renames = append(renames, change)
			continue
		}

		name := change.To.Name
		if action == merkletrie.Delete {
			name = change.From.Name
		}

		idx := b.getShardIndex(name)
		shardChanges[idx] = append(shardChanges[idx], change)
	}

	// Process shards in parallel
	var wg sync.WaitGroup
	errs := make([]error, b.Goroutines)

	for i := 0; i < b.Goroutines; i++ {
		changes := shardChanges[i]
		if len(changes) == 0 {
			continue
		}

		wg.Add(1)
		go func(idx int, changes []*object.Change) {
			defer wg.Done()
			shard := b.shards[idx]

			for _, change := range changes {
				action, _ := change.Action()
				var err error
				switch action {
				case merkletrie.Insert:
					err = b.handleInsertion(shard, change, author, cache)
				case merkletrie.Delete:
					err = b.handleDeletion(shard, change, author, cache)
				case merkletrie.Modify:
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

	// Process renames sequentially
	for _, change := range renames {
		// Renames involve modifications too
		err := b.handleModificationRename(change, author, cache, fileDiffs)
		if err != nil {
			return err
		}
	}

	b.tick = tick
	return nil
}

func (b *BurndownHistoryAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	// Fork is used for branching logic.
	// Since ShardedAllocator doesn't support cloning yet, we panic.
	panic("Fork not implemented for ShardedAllocator yet")
}

func (b *BurndownHistoryAnalyzer) Merge(branches []analyze.HistoryAnalyzer) {
	panic("Merge not implemented for ShardedAllocator yet")
}

func (b *BurndownHistoryAnalyzer) Hibernate() error {
	b.shardedAllocator.Hibernate()
	if b.HibernationToDisk {
		file, err := ioutil.TempFile(b.HibernationDirectory, "*-codefang.bin")
		if err != nil {
			return err
		}
		b.hibernatedFileName = file.Name()
		err = file.Close()
		if err != nil {
			b.hibernatedFileName = ""
			return err
		}
		// Clean up the temp file as Serialize will create its own files with suffix
		err = os.Remove(b.hibernatedFileName)
		if err != nil {
			b.hibernatedFileName = ""
			return err
		}

		err = b.shardedAllocator.Serialize(b.hibernatedFileName)
		if err != nil {
			b.hibernatedFileName = ""
			return err
		}
	}
	return nil
}

func (b *BurndownHistoryAnalyzer) Boot() error {
	if b.hibernatedFileName != "" {
		err := b.shardedAllocator.Deserialize(b.hibernatedFileName)
		if err != nil {
			return err
		}
		// Cleanup happens implicitly if user deletes the files?
		// Or we should clean up here?
		// The original code: err = os.Remove(b.hibernatedFileName)
		// Now we have .shard.N files.
		for i := 0; i < len(b.shards); i++ {
			_ = os.Remove(fmt.Sprintf("%s.shard.%d", b.hibernatedFileName, i))
		}
		b.hibernatedFileName = ""
	}
	b.shardedAllocator.Boot()
	return nil
}

func (b *BurndownHistoryAnalyzer) Finalize() (analyze.Report, error) {
	globalHistory, lastTick := b.groupSparseHistory(b.globalHistory, -1)
	fileHistories := map[string]DenseHistory{}
	fileOwnership := map[string]map[int]int{}

	// Iterate over all shards to collect files
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
				previousAuthor, _ = b.unpackPersonWithTick(int(value))
				if previousAuthor == identity.AuthorMissing {
					previousAuthor = -1
				}
			})
		}
	}

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
	var peopleMatrix DenseHistory
	if len(b.matrix) > 0 {
		peopleMatrix = make(DenseHistory, b.PeopleNumber)
		for i, row := range b.matrix {
			mrow := make([]int64, b.PeopleNumber+2)
			peopleMatrix[i] = mrow
			for key, val := range row {
				if key == identity.AuthorMissing {
					key = -1
				} else if key == authorSelf {
					key = -2
				}
				mrow[key+2] = val
			}
		}
	}
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

func (b *BurndownHistoryAnalyzer) Serialize(result analyze.Report, binary bool, writer io.Writer) error {
	enc := json.NewEncoder(writer)
	enc.SetIndent("", "  ")
	return enc.Encode(result)
}

func (b *BurndownHistoryAnalyzer) FormatReport(report analyze.Report, writer io.Writer) error {
	return b.Serialize(report, false, writer)
}

// Helpers

func (b *BurndownHistoryAnalyzer) packPersonWithTick(person int, tick int) int {
	if b.PeopleNumber == 0 {
		return tick
	}
	result := tick & burndown.TreeMergeMark
	result |= person << burndown.TreeMaxBinPower
	return result
}

func (b *BurndownHistoryAnalyzer) unpackPersonWithTick(value int) (int, int) {
	if b.PeopleNumber == 0 {
		return identity.AuthorMissing, value
	}
	return value >> burndown.TreeMaxBinPower, value & burndown.TreeMergeMark
}

func (b *BurndownHistoryAnalyzer) onNewTick() {
	if b.tick > b.previousTick {
		b.previousTick = b.tick
	}
	b.mergedAuthor = identity.AuthorMissing
}

func (b *BurndownHistoryAnalyzer) updateGlobal(currentTime, previousTime, delta int) {
	_, curTick := b.unpackPersonWithTick(currentTime)
	_, prevTick := b.unpackPersonWithTick(previousTime)

	currentHistory := b.globalHistory[curTick]
	if currentHistory == nil {
		currentHistory = map[int]int64{}
		b.globalHistory[curTick] = currentHistory
	}
	currentHistory[prevTick] += int64(delta)
}

func (b *BurndownHistoryAnalyzer) updateFile(history sparseHistory, currentTime, previousTime, delta int) {
	_, curTick := b.unpackPersonWithTick(currentTime)
	_, prevTick := b.unpackPersonWithTick(previousTime)

	currentHistory := history[curTick]
	if currentHistory == nil {
		currentHistory = map[int]int64{}
		history[curTick] = currentHistory
	}
	currentHistory[prevTick] += int64(delta)
}

func (b *BurndownHistoryAnalyzer) updateAuthor(currentTime, previousTime, delta int) {
	previousAuthor, prevTick := b.unpackPersonWithTick(previousTime)
	if previousAuthor == identity.AuthorMissing {
		return
	}
	_, curTick := b.unpackPersonWithTick(currentTime)
	history := b.peopleHistories[previousAuthor]
	if history == nil {
		history = sparseHistory{}
		b.peopleHistories[previousAuthor] = history
	}
	currentHistory := history[curTick]
	if currentHistory == nil {
		currentHistory = map[int]int64{}
		history[curTick] = currentHistory
	}
	currentHistory[prevTick] += int64(delta)
}

func (b *BurndownHistoryAnalyzer) updateMatrix(currentTime, previousTime, delta int) {
	newAuthor, _ := b.unpackPersonWithTick(currentTime)
	oldAuthor, _ := b.unpackPersonWithTick(previousTime)

	if oldAuthor == identity.AuthorMissing {
		return
	}
	if newAuthor == oldAuthor && delta > 0 {
		newAuthor = authorSelf
	}
	row := b.matrix[oldAuthor]
	if row == nil {
		row = map[int]int64{}
		b.matrix[oldAuthor] = row
	}
	cell, exists := row[newAuthor]
	if !exists {
		row[newAuthor] = 0
		cell = 0
	}
	row[newAuthor] = cell + int64(delta)
}

func (b *BurndownHistoryAnalyzer) newFile(shard *BurndownShard, hash gitplumbing.Hash, name string, author int, tick int, size int) (*burndown.File, error) {
	updaters := make([]burndown.Updater, 1)
	updaters[0] = func(currentTime, previousTime, delta int) {
		b.GlobalMu.Lock()
		defer b.GlobalMu.Unlock()
		b.updateGlobal(currentTime, previousTime, delta)
	}
	if b.TrackFiles {
		history := shard.fileHistories[name]
		if history == nil {
			history = sparseHistory{}
		}
		shard.fileHistories[name] = history
		updaters = append(updaters, func(currentTime, previousTime, delta int) {
			b.updateFile(history, currentTime, previousTime, delta)
		})
	}
	if b.PeopleNumber > 0 {
		updaters = append(updaters, func(currentTime, previousTime, delta int) {
			b.GlobalMu.Lock()
			defer b.GlobalMu.Unlock()
			b.updateAuthor(currentTime, previousTime, delta)
		})
		updaters = append(updaters, func(currentTime, previousTime, delta int) {
			b.GlobalMu.Lock()
			defer b.GlobalMu.Unlock()
			b.updateMatrix(currentTime, previousTime, delta)
		})
		tick = b.packPersonWithTick(author, tick)
	}
	return burndown.NewFile(tick, size, shard.allocator, updaters...), nil
}

func (b *BurndownHistoryAnalyzer) handleInsertion(shard *BurndownShard, change *object.Change, author int, cache map[gitplumbing.Hash]*pkgplumbing.CachedBlob) error {
	blob := cache[change.To.TreeEntry.Hash]
	lines, err := blob.CountLines()
	if err != nil {
		return nil
	}
	name := change.To.Name
	file, exists := shard.files[name]
	if exists {
		return fmt.Errorf("file %s already exists", name)
	}
	var hash gitplumbing.Hash
	if b.tick != burndown.TreeMergeMark {
		hash = blob.Hash
	}
	file, err = b.newFile(shard, hash, name, author, b.tick, lines)
	shard.files[name] = file
	// renames and deletions maps also need protection or sharding?
	// deletions is map[string]bool. Used for special logic in handleDeletion.
	// We can shard it too or use sync map.
	// Since deletions is accessed by filename, and we shard by filename, we can put it in shard.
	// But struct doesn't have deletions map in shard yet.
	// For now, let's use GlobalMu for deletions/renames maps access in these methods if they are not heavily contended or shard them.
	// Renames are global. Deletions are global.
	// But deletions[name] is only accessed when processing 'name'.
	// So if we shard deletions map...
	// For now, lock GlobalMu.

	b.GlobalMu.Lock()
	delete(b.deletions, name)
	if b.tick == burndown.TreeMergeMark {
		b.mergedFiles[name] = true
	}
	b.GlobalMu.Unlock()
	return err
}

func (b *BurndownHistoryAnalyzer) handleDeletion(shard *BurndownShard, change *object.Change, author int, cache map[gitplumbing.Hash]*pkgplumbing.CachedBlob) error {
	var name string
	if change.To.TreeEntry.Hash != gitplumbing.ZeroHash {
		name = change.To.Name
	} else {
		name = change.From.Name
	}
	file, exists := shard.files[name]
	if !exists {
		return nil
	}
	blob := cache[change.From.TreeEntry.Hash]
	lines, err := blob.CountLines()
	if err != nil {
		return fmt.Errorf("previous version of %s unexpectedly became binary", name)
	}
	tick := b.tick

	b.GlobalMu.Lock()
	isDeletion := b.deletions[name]
	b.deletions[name] = true
	b.GlobalMu.Unlock()

	if b.tick == burndown.TreeMergeMark && !isDeletion {
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
	if b.tick == burndown.TreeMergeMark {
		b.mergedFiles[name] = false
	}
	b.GlobalMu.Unlock()

	return nil
}

func (b *BurndownHistoryAnalyzer) handleModification(shard *BurndownShard, change *object.Change, author int, cache map[gitplumbing.Hash]*pkgplumbing.CachedBlob, diffs map[string]pkgplumbing.FileDiffData) error {
	// This method handles modification WITHOUT rename (checked in Consume)
	b.GlobalMu.Lock()
	if b.tick == burndown.TreeMergeMark {
		b.mergedFiles[change.To.Name] = true
	}
	b.GlobalMu.Unlock()

	file, exists := shard.files[change.From.Name]
	if !exists {
		return b.handleInsertion(shard, change, author, cache)
	}

	blobFrom := cache[change.From.TreeEntry.Hash]
	_, errFrom := blobFrom.CountLines()
	blobTo := cache[change.To.TreeEntry.Hash]
	_, errTo := blobTo.CountLines()
	if errFrom != errTo {
		if errFrom != nil {
			return b.handleInsertion(shard, change, author, cache)
		}
		return b.handleDeletion(shard, change, author, cache)
	} else if errFrom != nil {
		return nil
	}

	thisDiffs := diffs[change.To.Name]
	if file.Len() != thisDiffs.OldLinesOfCode {
		return fmt.Errorf("%s: internal integrity error src %d != %d", change.To.Name, thisDiffs.OldLinesOfCode, file.Len())
	}

	b.applyDiffs(file, thisDiffs, author)
	return nil
}

func (b *BurndownHistoryAnalyzer) handleModificationRename(change *object.Change, author int, cache map[gitplumbing.Hash]*pkgplumbing.CachedBlob, diffs map[string]pkgplumbing.FileDiffData) error {
	// Handles modification WITH rename (From != To)
	// This runs sequentially, so we can access shards safely if we look them up

	b.GlobalMu.Lock()
	if b.tick == burndown.TreeMergeMark {
		b.mergedFiles[change.To.Name] = true
	}
	b.GlobalMu.Unlock()

	shardFrom := b.getShard(change.From.Name)
	file, exists := shardFrom.files[change.From.Name]
	if !exists {
		// Fallback to insertion in To shard
		shardTo := b.getShard(change.To.Name)
		return b.handleInsertion(shardTo, change, author, cache)
	}

	if change.To.Name != change.From.Name {
		err := b.handleRename(change.From.Name, change.To.Name)
		if err != nil {
			return err
		}
		// File is now at change.To.Name in correct shard
		shardTo := b.getShard(change.To.Name)
		file = shardTo.files[change.To.Name]
	}

	blobFrom := cache[change.From.TreeEntry.Hash]
	_, errFrom := blobFrom.CountLines()
	blobTo := cache[change.To.TreeEntry.Hash]
	_, errTo := blobTo.CountLines()
	if errFrom != errTo {
		if errFrom != nil {
			shardTo := b.getShard(change.To.Name)
			return b.handleInsertion(shardTo, change, author, cache)
		}
		// handleDeletion on new name? Or old?
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
		return fmt.Errorf("%s: internal integrity error src %d != %d", change.To.Name, thisDiffs.OldLinesOfCode, file.Len())
	}

	b.applyDiffs(file, thisDiffs, author)
	return nil
}

func (b *BurndownHistoryAnalyzer) applyDiffs(file *burndown.File, thisDiffs pkgplumbing.FileDiffData, author int) {
	position := 0
	pending := diffmatchpatch.Diff{Text: ""}

	apply := func(edit diffmatchpatch.Diff) {
		length := utf8.RuneCountInString(edit.Text)
		if edit.Type == diffmatchpatch.DiffInsert {
			file.Update(b.packPersonWithTick(author, b.tick), position, length, 0)
			position += length
		} else {
			file.Update(b.packPersonWithTick(author, b.tick), position, 0, length)
		}
		if b.Debug {
			file.Validate()
		}
	}

	for _, edit := range thisDiffs.Diffs {
		length := utf8.RuneCountInString(edit.Text)
		switch edit.Type {
		case diffmatchpatch.DiffEqual:
			if pending.Text != "" {
				apply(pending)
				pending.Text = ""
			}
			position += length
		case diffmatchpatch.DiffInsert:
			if pending.Text != "" {
				file.Update(b.packPersonWithTick(author, b.tick), position, length,
					utf8.RuneCountInString(pending.Text))
				if b.Debug {
					file.Validate()
				}
				position += length
				pending.Text = ""
			} else {
				pending = edit
			}
		case diffmatchpatch.DiffDelete:
			pending = edit
		}
	}
	if pending.Text != "" {
		apply(pending)
		pending.Text = ""
	}
}

func (b *BurndownHistoryAnalyzer) handleRename(from, to string) error {
	if from == to {
		return nil
	}

	shardFrom := b.getShard(from)
	file, exists := shardFrom.files[from]
	if !exists {
		return fmt.Errorf("file %s > %s does not exist", from, to)
	}

	shardTo := b.getShard(to)

	if shardFrom == shardTo {
		delete(shardFrom.files, from)
		shardFrom.files[to] = file
		if b.TrackFiles {
			history := shardFrom.fileHistories[from]
			if history == nil {
				history = sparseHistory{}
			}
			delete(shardFrom.fileHistories, from)
			shardFrom.fileHistories[to] = history
		}
	} else {
		// Cross-shard move: deep clone to new allocator
		newFile := file.CloneDeep(shardTo.allocator)
		shardTo.files[to] = newFile
		file.Delete()
		delete(shardFrom.files, from)

		if b.TrackFiles {
			history := shardFrom.fileHistories[from]
			if history == nil {
				history = sparseHistory{}
			}
			delete(shardFrom.fileHistories, from)
			shardTo.fileHistories[to] = history
		}
	}

	b.GlobalMu.Lock()
	delete(b.deletions, to)
	if b.tick == burndown.TreeMergeMark {
		b.mergedFiles[from] = false
	}
	b.renames[from] = to
	b.GlobalMu.Unlock()
	return nil
}

func (b *BurndownHistoryAnalyzer) groupSparseHistory(history sparseHistory, lastTick int) (DenseHistory, int) {
	if len(history) == 0 {
		return DenseHistory{}, lastTick
	}
	var ticks []int
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
	samples := lastTick/b.Sampling + 1
	bands := lastTick/b.Granularity + 1
	result := make(DenseHistory, samples)
	for i := 0; i < samples; i++ {
		result[i] = make([]int64, bands)
	}
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
	return result, lastTick
}
