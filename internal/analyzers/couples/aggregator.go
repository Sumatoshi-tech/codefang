package couples

import (
	"context"
	"maps"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/spillstore"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/bloom"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// Bloom filter configuration for file membership pre-filtering.
const (
	bloomFilesFPRate  = 0.01 // 1% false-positive rate for file membership.
	bloomFilesMinSize = 64   // Minimum bloom filter size.
)

// Memory estimation constants for aggregator state size.
const (
	fileCouplingEntryBytes = 80 // map entry overhead per file coupling pair.
	personFileEntryBytes   = 60 // map entry overhead per person-file entry.
	renameEntryBytes       = 100
	personCommitBytes      = 8
)

// Aggregator implements analyze.Aggregator for the couples analyzer.
// It accumulates the file co-occurrence matrix, per-person file touches,
// per-person commit counts, and rename tracking from the TC stream.
type Aggregator struct {
	files         *spillstore.SpillStore[map[string]int]
	people        []map[string]int
	peopleCommits []int
	renames       []RenamePair
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
		files:         spillstore.New[map[string]int](),
		people:        people,
		peopleCommits: make([]int, peopleNumber+1),
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
		Files:         copyFilesMap(a.files.Current()),
		People:        copyPeopleSlice(a.people),
		PeopleCommits: copyIntSlice(a.peopleCommits),
		Renames:       a.renames,
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

	for _, tick := range ticks {
		td, ok := tick.Data.(*TickData)
		if !ok || td == nil {
			continue
		}

		mergeTickFiles(mergedFiles, td.Files)
		mergeTickPeople(mergedPeople, td.People)

		mergedRenames = append(mergedRenames, td.Renames...)
	}

	effectivePeopleNumber := actualPeople - 1

	return buildReport(ctx, mergedFiles, mergedPeople, mergedRenames,
		reversedNames, effectivePeopleNumber, lastCommit)
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

	err = tree.FilesContext(ctx).ForEach(func(fobj *gitlib.File) error {
		files[fobj.Name] = true

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

// copyFilesMap creates a deep copy of the file coupling map.
func copyFilesMap(src map[string]map[string]int) map[string]map[string]int {
	dst := make(map[string]map[string]int, len(src))

	for file, lane := range src {
		dstLane := make(map[string]int, len(lane))
		maps.Copy(dstLane, lane)
		dst[file] = dstLane
	}

	return dst
}

// copyPeopleSlice creates a deep copy of the people slice.
func copyPeopleSlice(src []map[string]int) []map[string]int {
	dst := make([]map[string]int, len(src))

	for i, m := range src {
		dstMap := make(map[string]int, len(m))
		maps.Copy(dstMap, m)
		dst[i] = dstMap
	}

	return dst
}

// copyIntSlice creates a copy of an int slice.
func copyIntSlice(src []int) []int {
	dst := make([]int, len(src))
	copy(dst, src)

	return dst
}
