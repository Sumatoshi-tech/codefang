package couples

import (
	"context"
	"maps"
	"sort"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/spillstore"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/bloom"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/mapx"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// Bloom filter configuration for file membership pre-filtering.
const (
	bloomFilesFPRate  = 0.01 // 1% false-positive rate for file membership.
	bloomFilesMinSize = 64   // Minimum bloom filter size.
)

// Memory estimation constants for aggregator state size.
// These account for map entry overhead + string key storage (file paths
// average ~60 bytes in large repos like kubernetes). Accurate estimates
// are critical: underestimation delays spills and causes OOM.
const (
	fileCouplingEntryBytes = 200 // map overhead (80) + key string (~60) + inner key string (~60).
	personFileEntryBytes   = 130 // map overhead (60) + file path string (~60) + padding.
	renameEntryBytes       = 100
	personCommitBytes      = 8
)

// CommitSummary holds per-commit summary data for timeseries output.
type CommitSummary struct {
	FilesTouched int `json:"files_touched"`
	AuthorID     int `json:"author_id"`
}

// Aggregator implements analyze.Aggregator for the couples analyzer.
// It accumulates the file co-occurrence matrix, per-person file touches,
// per-person commit counts, and rename tracking from the TC stream.
type Aggregator struct {
	files         *spillstore.SpillStore[map[string]int]
	people        []map[string]int
	peopleCommits []int
	renames       []RenamePair
	commitStats   map[string]*CommitSummary
	commitsByTick map[int][]gitlib.Hash
	opts          analyze.AggregatorOptions
	peopleNumber  int
	reversedNames []string
	lastCommit    analyze.CommitLike
	closed        bool
}

func newAggregator(
	opts analyze.AggregatorOptions,
	peopleNumber int,
	reversedNames []string,
	lastCommit analyze.CommitLike,
) *Aggregator {
	people := make([]map[string]int, peopleNumber+1)
	for i := range people {
		people[i] = make(map[string]int)
	}

	return &Aggregator{
		files:         spillstore.New[map[string]int](opts.SpillDir),
		people:        people,
		peopleCommits: make([]int, peopleNumber+1),
		commitStats:   make(map[string]*CommitSummary),
		commitsByTick: make(map[int][]gitlib.Hash),
		opts:          opts,
		peopleNumber:  peopleNumber,
		reversedNames: reversedNames,
		lastCommit:    lastCommit,
	}
}

// Add ingests a single per-commit TC into the aggregator.
func (a *Aggregator) Add(tc analyze.TC) error {
	cd, ok := tc.Data.(*CommitData)
	if !ok || cd == nil {
		return nil
	}

	author := tc.AuthorID
	a.ensureCapacity(author + 1)

	if cd.CommitCounted {
		a.peopleCommits[author]++
	}

	a.addAuthorFiles(cd.AuthorFiles, author)
	a.addFileCouplings(cd.CouplingFiles)
	a.renames = append(a.renames, cd.Renames...)

	if !tc.CommitHash.IsZero() {
		hashStr := tc.CommitHash.String()
		a.commitStats[hashStr] = &CommitSummary{
			FilesTouched: len(cd.CouplingFiles),
			AuthorID:     author,
		}
		a.commitsByTick[tc.Tick] = append(a.commitsByTick[tc.Tick], tc.CommitHash)
	}

	if a.opts.SpillBudget > 0 && a.EstimatedStateSize() > a.opts.SpillBudget {
		_, err := a.Spill()
		if err != nil {
			return err
		}
	}

	return nil
}

// addAuthorFiles merges per-commit author file touches into the aggregator.
func (a *Aggregator) addAuthorFiles(authorFiles map[string]int, author int) {
	for file, count := range authorFiles {
		a.people[author][file] += count
	}
}

// batchCouplingThreshold is the commit size above which we pre-allocate
// lane maps with the known coupling set size to reduce map growth overhead.
const batchCouplingThreshold = 100

// addFileCouplings updates the file co-occurrence matrix.
//
// For large commits (>= batchCouplingThreshold files), pre-allocates lane
// maps at the correct capacity to avoid incremental map growth from O(C²)
// insertions.
func (a *Aggregator) addFileCouplings(couplingFiles []string) {
	n := len(couplingFiles)

	for _, file := range couplingFiles {
		lane, ok := a.files.Get(file)
		if !ok {
			// Pre-allocate to the known size for large commits.
			initCap := 0
			if n >= batchCouplingThreshold {
				initCap = n
			}

			lane = make(map[string]int, initCap)
		}

		for _, other := range couplingFiles {
			lane[other]++
		}

		a.files.Put(file, lane)
	}
}

// ensureCapacity grows people and peopleCommits slices if needed.
func (a *Aggregator) ensureCapacity(minSize int) {
	if minSize <= len(a.people) {
		return
	}

	newPeople := make([]map[string]int, minSize)
	copy(newPeople, a.people)

	for i := len(a.people); i < minSize; i++ {
		newPeople[i] = make(map[string]int)
	}

	a.people = newPeople

	newCommits := make([]int, minSize)
	copy(newCommits, a.peopleCommits)
	a.peopleCommits = newCommits
}

// FlushTick returns the aggregated TICK for the given tick index.
func (a *Aggregator) FlushTick(tick int) (analyze.TICK, error) {
	if a.files.Len() == 0 && len(a.renames) == 0 {
		return analyze.TICK{Tick: tick}, nil
	}

	td := &TickData{
		Files:         mapx.CloneNested(a.files.Current()),
		People:        clonePeopleSlice(a.people),
		PeopleCommits: mapx.CloneSlice(a.peopleCommits),
		Renames:       a.renames,
		CommitStats:   a.commitStats,
	}

	return analyze.TICK{
		Tick: tick,
		Data: td,
	}, nil
}

// FlushAllTicks returns a single TICK containing all accumulated coupling data.
// Couples accumulates cumulatively across all commits.
func (a *Aggregator) FlushAllTicks() ([]analyze.TICK, error) {
	if a.files.Len() == 0 && len(a.renames) == 0 {
		return nil, nil
	}

	t, err := a.FlushTick(0)
	if err != nil {
		return nil, err
	}

	return []analyze.TICK{t}, nil
}

// DiscardState clears all in-memory cumulative state without serialization.
func (a *Aggregator) DiscardState() {
	a.files = spillstore.New[map[string]int](a.opts.SpillDir)

	a.people = make([]map[string]int, a.peopleNumber+1)
	for i := range a.people {
		a.people[i] = make(map[string]int)
	}

	a.peopleCommits = make([]int, a.peopleNumber+1)
	a.renames = nil
}

// Spill writes accumulated file coupling state to disk to free memory.
func (a *Aggregator) Spill() (int64, error) {
	if a.files.Len() == 0 {
		return 0, nil
	}

	sizeBefore := a.EstimatedStateSize()

	err := a.files.Spill()
	if err != nil {
		return 0, err
	}

	return sizeBefore, nil
}

// Collect reloads spilled file coupling state back into memory.
func (a *Aggregator) Collect() error {
	collected, err := a.files.CollectWith(mergeFileCouplings)
	if err != nil {
		return err
	}

	for k, v := range collected {
		a.files.Put(k, v)
	}

	return nil
}

// Memory bounds for filtered coupling collection.
const (
	// pruneInterval controls how often weak entries are pruned during merge.
	pruneInterval = 10

	// maxEntriesPerFile caps coupling entries per file during collection.
	// The store writer only uses top-K pairs globally (default 100), so keeping
	// 500 entries per file is sufficient for accurate top-K output while bounding
	// memory to 25K files × 500 entries × ~100 bytes ≈ 1.25 GB.
	maxEntriesPerFile = 500
)

// collectFilteredFiles merges spill files, keeping only entries for files in
// currentFiles. Inner coupling entries are also filtered through matcher.
// Bounds memory by periodically pruning weak entries AND capping per-file entries.
// This avoids materializing the full O(F²) coupling map for historical files
// that no longer exist, which can consume 50+ GB on large repos like kubernetes.
func (a *Aggregator) collectFilteredFiles(
	currentFiles map[string]bool,
	matcher fileMatchFunc,
	minWeight int,
) (map[string]map[string]int, error) {
	result := make(map[string]map[string]int, len(currentFiles))
	processed := 0

	mergeFn := func(chunk map[string]map[string]int) error {
		mergeChunkIntoResult(result, chunk, currentFiles, matcher)

		processed++

		if processed%pruneInterval == 0 {
			pruneAndCapEntries(result, minWeight, maxEntriesPerFile)
		}

		return nil
	}

	spillErr := a.files.ForEachSpill(mergeFn)
	if spillErr != nil {
		return nil, spillErr
	}

	// Also merge current in-memory buffer.
	mergeErr := mergeFn(a.files.Current())
	if mergeErr != nil {
		return nil, mergeErr
	}

	a.files.Cleanup()

	// Final prune + cap after all chunks merged.
	pruneAndCapEntries(result, minWeight, maxEntriesPerFile)

	return result, nil
}

// mergeChunkIntoResult merges a single spill chunk into the accumulated result,
// filtering to only current files and matched inner entries.
func mergeChunkIntoResult(
	result, chunk map[string]map[string]int,
	currentFiles map[string]bool,
	matcher fileMatchFunc,
) {
	for file1, inner := range chunk {
		if !currentFiles[file1] {
			continue
		}

		lane := result[file1]
		if lane == nil {
			lane = make(map[string]int)
			result[file1] = lane
		}

		for file2, count := range inner {
			if matcher(file2) {
				lane[file2] += count
			}
		}
	}
}

// pruneAndCapEntries removes weak coupling entries and caps per-file entries.
// First removes entries below minWeight, then caps each file's lane to maxEntries
// by evicting the lowest-count entries.
func pruneAndCapEntries(files map[string]map[string]int, minWeight, maxEntries int) {
	for file1, lane := range files {
		originalLen := len(lane)

		pruneWeakEntries(lane, file1, minWeight)

		// Cap per-file entries to maxEntries.
		if maxEntries > 0 && len(lane) > maxEntries {
			capLaneEntries(lane, file1, maxEntries)
		}

		// Remove files with only a self-reference (or empty).
		if len(lane) <= 1 {
			delete(files, file1)

			continue
		}

		compactLaneIfNeeded(files, file1, lane, originalLen)
	}
}

// pruneWeakEntries removes coupling entries below minWeight, preserving self-references.
func pruneWeakEntries(lane map[string]int, self string, minWeight int) {
	if minWeight <= 1 {
		return
	}

	for file2, count := range lane {
		if self != file2 && count < minWeight {
			delete(lane, file2)
		}
	}
}

// compactLaneIfNeeded replaces a lane map with a smaller copy if a significant
// portion of its entries were deleted. Go maps never shrink their underlying
// bucket arrays, so this prevents memory bloat during massive merges.
func compactLaneIfNeeded(files map[string]map[string]int, file1 string, lane map[string]int, originalLen int) {
	if len(lane)*2 < originalLen {
		compacted := make(map[string]int, len(lane))
		maps.Copy(compacted, lane)

		files[file1] = compacted
	}
}

// capLaneEntries reduces a coupling lane to at most maxEntries by evicting
// entries with the lowest counts. Preserves the self-entry (file1 -> file1).
func capLaneEntries(lane map[string]int, self string, maxEntries int) {
	if len(lane) <= maxEntries {
		return
	}

	threshold, ok := computeEvictionThreshold(lane, self, maxEntries)
	if !ok {
		return
	}

	// Delete entries below threshold.
	for k, c := range lane {
		if k != self && c < threshold {
			delete(lane, k)
		}
	}

	// If still over due to ties at threshold, delete some at threshold.
	evictTiedEntries(lane, self, threshold, maxEntries)
}

// computeEvictionThreshold determines the count threshold below which entries
// should be evicted to cap the lane at maxEntries. Returns false if no eviction needed.
func computeEvictionThreshold(lane map[string]int, self string, maxEntries int) (int, bool) {
	counts := make([]int, 0, len(lane))

	for k, c := range lane {
		if k != self {
			counts = append(counts, c)
		}
	}

	sort.Sort(sort.Reverse(sort.IntSlice(counts)))

	// Subtract 1 for the self-entry.
	keepN := maxEntries - 1
	if keepN >= len(counts) {
		return 0, false
	}

	return counts[keepN], true
}

// evictTiedEntries removes entries at the threshold count one at a time
// until the lane is within the maxEntries limit.
func evictTiedEntries(lane map[string]int, self string, threshold, maxEntries int) {
	for len(lane) > maxEntries {
		for k, c := range lane {
			if k != self && c == threshold {
				delete(lane, k)

				break
			}
		}
	}
}

// EstimatedStateSize returns the current in-memory footprint in bytes.
func (a *Aggregator) EstimatedStateSize() int64 {
	var size int64

	for _, lane := range a.files.Current() {
		size += int64(len(lane)) * fileCouplingEntryBytes
	}

	for _, files := range a.people {
		size += int64(len(files)) * personFileEntryBytes
	}

	size += int64(len(a.peopleCommits)) * personCommitBytes
	size += int64(len(a.renames)) * renameEntryBytes

	return size
}

// SpillState returns the current on-disk spill state for checkpoint persistence.
func (a *Aggregator) SpillState() analyze.AggregatorSpillInfo {
	return analyze.AggregatorSpillInfo{Dir: a.files.SpillDir(), Count: a.files.SpillCount()}
}

// RestoreSpillState points the aggregator at a previously-saved spill directory.
func (a *Aggregator) RestoreSpillState(info analyze.AggregatorSpillInfo) {
	a.files.RestoreFromDir(info.Dir, info.Count)
}

// DrainCommitStats implements analyze.CommitStatsDrainer.
// It extracts and clears per-commit data, returning the same shape as ExtractCommitTimeSeries.
func (a *Aggregator) DrainCommitStats() (stats map[string]any, tickHashes map[int][]gitlib.Hash) {
	if len(a.commitStats) == 0 {
		return nil, nil
	}

	result := make(map[string]any, len(a.commitStats))
	for hash, cs := range a.commitStats {
		result[hash] = map[string]any{
			"files_touched": cs.FilesTouched,
			"author_id":     cs.AuthorID,
		}
	}

	cbt := a.commitsByTick
	a.commitStats = make(map[string]*CommitSummary)
	a.commitsByTick = make(map[int][]gitlib.Hash)

	return result, cbt
}

// Close releases all resources. Idempotent.
func (a *Aggregator) Close() error {
	if a.closed {
		return nil
	}

	a.closed = true
	a.files.Cleanup()

	return nil
}

// ticksToReport converts aggregated TICKs into the analyze.Report format
// that existing ParseReportData()/ComputeAllMetrics() understand.
func ticksToReport(
	ctx context.Context,
	ticks []analyze.TICK,
	reversedNames []string,
	peopleNumber int,
	lastCommit analyze.CommitLike,
) analyze.Report {
	mergedFiles := make(map[string]map[string]int)

	// Determine the actual people count from tick data, which may exceed
	// the initial peopleNumber when authors are discovered incrementally.
	actualPeople := peopleNumber + 1

	for _, tick := range ticks {
		td, ok := tick.Data.(*TickData)
		if ok && td != nil && len(td.People) > actualPeople {
			actualPeople = len(td.People)
		}
	}

	mergedPeople := make([]map[string]int, actualPeople)
	for i := range mergedPeople {
		mergedPeople[i] = make(map[string]int)
	}

	var mergedRenames []RenamePair

	mergedCommitStats := make(map[string]*CommitSummary)
	commitsByTick := make(map[int][]gitlib.Hash)

	for _, tick := range ticks {
		td, ok := tick.Data.(*TickData)
		if !ok || td == nil {
			continue
		}

		mergeTickFiles(mergedFiles, td.Files)
		mergeTickPeople(mergedPeople, td.People)

		mergedRenames = append(mergedRenames, td.Renames...)

		for hash, stats := range td.CommitStats {
			mergedCommitStats[hash] = stats
			commitsByTick[tick.Tick] = append(commitsByTick[tick.Tick], gitlib.NewHash(hash))
		}
	}

	effectivePeopleNumber := actualPeople - 1

	report := buildReport(ctx, mergedFiles, mergedPeople, mergedRenames,
		reversedNames, effectivePeopleNumber, lastCommit)

	if len(mergedCommitStats) > 0 {
		report["commit_stats"] = mergedCommitStats
		report["commits_by_tick"] = commitsByTick
	}

	return report
}

// mergeTickFiles additively merges per-tick file couplings into the accumulator.
func mergeTickFiles(dst, src map[string]map[string]int) {
	for file, couplings := range src {
		lane, ok := dst[file]
		if !ok {
			lane = make(map[string]int)
			dst[file] = lane
		}

		for other, count := range couplings {
			lane[other] += count
		}
	}
}

// mergeTickPeople additively merges per-tick people data into the accumulator.
func mergeTickPeople(dst, src []map[string]int) {
	for i, srcFiles := range src {
		if i >= len(dst) {
			break
		}

		for file, count := range srcFiles {
			dst[i][file] += count
		}
	}
}

// buildReport constructs the final analyze.Report from merged data.
func buildReport(
	ctx context.Context,
	rawFiles map[string]map[string]int,
	people []map[string]int,
	renames []RenamePair,
	reversedNames []string,
	peopleNumber int,
	lastCommit analyze.CommitLike,
) analyze.Report {
	currentFiles := collectCurrentFiles(ctx, rawFiles, lastCommit)
	reducedFiles, reducedPeople := propagateRenamesForReport(rawFiles, people, renames, currentFiles)

	filesSequence, filesIndex := buildFilesIndex(reducedFiles)
	filesLines := computeFilesLinesFromCommit(ctx, filesSequence, lastCommit)

	effectivePeople := peopleNumber
	if len(people) > effectivePeople+1 {
		effectivePeople = len(people) - 1
	}

	peopleMatrix, peopleFiles := computePeopleMatrix(reducedPeople, filesIndex, effectivePeople)
	filesMatrix := computeFilesMatrix(reducedFiles, filesSequence, filesIndex)

	return analyze.Report{
		"PeopleMatrix":       peopleMatrix,
		"PeopleFiles":        peopleFiles,
		"Files":              filesSequence,
		"FilesLines":         filesLines,
		"FilesMatrix":        filesMatrix,
		"ReversedPeopleDict": reversedNames,
	}
}

// collectCurrentFilesFromTree builds the set of currently-existing files from the
// git tree at lastCommit, without requiring the raw coupling data.
// Returns nil if lastCommit is nil or the tree can't be read (caller should fall back).
func collectCurrentFilesFromTree(ctx context.Context, lastCommit analyze.CommitLike) map[string]bool {
	if lastCommit == nil {
		return nil
	}

	tree, err := lastCommit.Tree()
	if err != nil {
		return nil
	}

	files := map[string]bool{}
	processed := 0

	err = tree.FilesContext(ctx).ForEach(func(fobj *gitlib.File) error {
		files[fobj.Name] = true

		processed++
		if processed%1000 == 0 {
			gitlib.ReleaseNativeMemory()
		}

		return nil
	})
	if err != nil {
		return nil
	}

	return files
}

// collectCurrentFiles builds the set of currently-existing files.
func collectCurrentFiles(ctx context.Context, rawFiles map[string]map[string]int, lastCommit analyze.CommitLike) map[string]bool {
	files := map[string]bool{}

	if lastCommit == nil {
		for key := range rawFiles {
			files[key] = true
		}

		return files
	}

	tree, err := lastCommit.Tree()
	if err != nil {
		for key := range rawFiles {
			files[key] = true
		}

		return files
	}

	processed := 0

	err = tree.FilesContext(ctx).ForEach(func(fobj *gitlib.File) error {
		files[fobj.Name] = true

		processed++
		if processed%1000 == 0 {
			gitlib.ReleaseNativeMemory()
		}

		return nil
	})
	if err != nil {
		return files
	}

	return files
}

// fileMatchFunc tests whether a file is in the current set.
type fileMatchFunc func(file string) bool

// propagateRenamesForReport filters files and people to only currently-existing files.
//
// Uses a Bloom filter to pre-screen file existence before doing exact map lookups.
// This avoids the O(F²) nested iteration over currentFiles when the raw coupling
// map for a file doesn't contain most current files (sparse case).
func propagateRenamesForReport(
	rawFiles map[string]map[string]int,
	people []map[string]int,
	_ []RenamePair,
	currentFiles map[string]bool,
) (reducedFiles map[string]map[string]int, reducedPeople []map[string]int) {
	matcher := newFileMatchFunc(currentFiles)
	reducedFiles = reduceFiles(rawFiles, currentFiles, matcher)
	reducedPeople = reducePeople(people, matcher)

	return reducedFiles, reducedPeople
}

// newFileMatchFunc builds a Bloom-prefiltered match function, falling back to exact matching.
func newFileMatchFunc(currentFiles map[string]bool) fileMatchFunc {
	n := max(uint(len(currentFiles)), bloomFilesMinSize)

	currentFilter, bloomErr := bloom.NewWithEstimates(n, bloomFilesFPRate)
	if bloomErr != nil {
		return func(file string) bool { return currentFiles[file] }
	}

	for file := range currentFiles {
		currentFilter.Add([]byte(file))
	}

	return func(file string) bool {
		return currentFilter.Test([]byte(file)) && currentFiles[file]
	}
}

// reduceFiles filters the raw file coupling map to only currently-existing files.
func reduceFiles(
	rawFiles map[string]map[string]int,
	currentFiles map[string]bool,
	match fileMatchFunc,
) map[string]map[string]int {
	reduced := make(map[string]map[string]int)

	for file := range currentFiles {
		refmap := rawFiles[file]
		if len(refmap) == 0 {
			continue
		}

		fmap := make(map[string]int, min(len(refmap), len(currentFiles)))

		for other, refval := range refmap {
			if refval > 0 && match(other) {
				fmap[other] = refval
			}
		}

		if len(fmap) > 0 {
			reduced[file] = fmap
		}
	}

	return reduced
}

// reducePeople filters per-person file maps to only currently-existing files.
func reducePeople(people []map[string]int, match fileMatchFunc) []map[string]int {
	reduced := make([]map[string]int, len(people))

	for i, counts := range people {
		rp := make(map[string]int)
		reduced[i] = rp

		for file, count := range counts {
			if count > 0 && match(file) {
				rp[file] = count
			}
		}
	}

	return reduced
}

// computeFilesLinesFromCommit counts newlines in each file at the given commit.
func computeFilesLinesFromCommit(ctx context.Context, filesSequence []string, lastCommit analyze.CommitLike) []int {
	filesLines := make([]int, len(filesSequence))

	if lastCommit == nil {
		return filesLines
	}

	for i, name := range filesSequence {
		filesLines[i] = countFileLinesAt(ctx, name, lastCommit)

		// Bounding memory explicitly because `countFileLinesAt` loads
		// blob from libgit2, which may keep memory inside glibc arenas
		// leading to a spike.
		if i%100 == 0 && i > 0 {
			gitlib.ReleaseNativeMemory()
		}
	}

	return filesLines
}

// countFileLinesAt counts newlines in a single file at a commit.
func countFileLinesAt(ctx context.Context, name string, commit analyze.CommitLike) int {
	file, err := commit.File(name)
	if err != nil {
		return 0
	}

	blob, err := file.BlobContext(ctx)
	if err != nil {
		return 0
	}

	reader := blob.Reader()
	buf := make([]byte, readBufferSize)
	count := 0

	for {
		n, readErr := reader.Read(buf)
		count += countNewlines(buf[:n])

		if readErr != nil {
			break
		}
	}

	blob.Free()

	return count
}

// clonePeopleSlice deep-clones a slice of per-person file-touch maps using [mapx.Clone].
func clonePeopleSlice(src []map[string]int) []map[string]int {
	dst := make([]map[string]int, len(src))

	for i, m := range src {
		dst[i] = mapx.Clone(m)
	}

	return dst
}
