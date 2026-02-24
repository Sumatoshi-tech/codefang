package burndown

import (
	"context"
	"encoding/gob"
	"fmt"
	"maps"
	"os"
	"path/filepath"
	"slices"
	"time"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/identity"
)

// Memory estimation constants for aggregator state size.
const (
	sparseEntryBytes = 56 // overhead per sparseHistory entry (two map lookups + int64).
	matrixRowBytes   = 48 // overhead per matrix row.
)

// Aggregator implements analyze.Aggregator for the burndown analyzer.
// It accumulates sparse history deltas from the TC stream and produces
// dense history matrices for the final report.
type Aggregator struct {
	globalHistory   sparseHistory
	peopleHistories map[int]sparseHistory
	matrix          []map[int]int64
	fileHistories   map[PathID]sparseHistory

	// Configuration carried from the analyzer.
	opts               analyze.AggregatorOptions
	granularity        int
	sampling           int
	peopleNumber       int
	trackFiles         bool
	tickSize           time.Duration
	reversedPeopleDict []string
	pathInterner       *PathInterner

	// Tick tracking.
	lastTick int
	endTime  time.Time

	// Spill state.
	spillDir string
	spillN   int
	closed   bool
}

func newAggregator(
	opts analyze.AggregatorOptions,
	granularity, sampling, peopleNumber int,
	trackFiles bool,
	tickSize time.Duration,
	reversedPeopleDict []string,
	pathInterner *PathInterner,
) *Aggregator {
	return &Aggregator{
		globalHistory:      sparseHistory{},
		peopleHistories:    map[int]sparseHistory{},
		fileHistories:      map[PathID]sparseHistory{},
		opts:               opts,
		granularity:        granularity,
		sampling:           sampling,
		peopleNumber:       peopleNumber,
		trackFiles:         trackFiles,
		tickSize:           tickSize,
		reversedPeopleDict: reversedPeopleDict,
		pathInterner:       pathInterner,
	}
}

// Add ingests a single per-commit TC into the aggregator.
func (a *Aggregator) Add(tc analyze.TC) error {
	cr, ok := tc.Data.(*CommitResult)
	if !ok || cr == nil {
		return nil
	}

	mergeSparseHistory(a.globalHistory, cr.GlobalDeltas)
	mergePeopleHistories(a.peopleHistories, cr.PeopleDeltas)
	mergeMatrixInto(&a.matrix, cr.MatrixDeltas)

	if a.trackFiles {
		a.mergeFileDeltas(cr.FileDeltas)
	}

	if tc.Tick > a.lastTick {
		a.lastTick = tc.Tick
	}

	if !tc.Timestamp.IsZero() && tc.Timestamp.After(a.endTime) {
		a.endTime = tc.Timestamp
	}

	if a.opts.SpillBudget > 0 && a.EstimatedStateSize() > a.opts.SpillBudget {
		_, err := a.Spill()
		if err != nil {
			return err
		}
	}

	return nil
}

func (a *Aggregator) mergeFileDeltas(deltas map[PathID]sparseHistory) {
	for pathID, history := range deltas {
		if len(history) == 0 {
			continue
		}

		if a.fileHistories[pathID] == nil {
			a.fileHistories[pathID] = sparseHistory{}
		}

		mergeSparseHistory(a.fileHistories[pathID], history)
	}
}

// FlushTick returns the aggregated TICK for the given tick index.
// For burndown, we emit a single TICK at the end containing all accumulated state.
func (a *Aggregator) FlushTick(tick int) (analyze.TICK, error) {
	if len(a.globalHistory) == 0 {
		return analyze.TICK{Tick: tick}, nil
	}

	td := &TickResult{
		GlobalHistory:   cloneSparseHistory(a.globalHistory),
		PeopleHistories: a.clonePeopleHistories(),
		Matrix:          a.cloneMatrix(),
		FileHistories:   a.cloneFileHistories(),
	}

	return analyze.TICK{
		Tick:    tick,
		EndTime: a.endTime,
		Data:    td,
	}, nil
}

// FlushAllTicks returns a single TICK containing all accumulated burndown state.
// Burndown accumulates cumulatively; there is one TICK at lastTick.
func (a *Aggregator) FlushAllTicks() ([]analyze.TICK, error) {
	if len(a.globalHistory) == 0 {
		return nil, nil
	}

	t, err := a.FlushTick(a.lastTick)
	if err != nil {
		return nil, err
	}

	return []analyze.TICK{t}, nil
}

func (a *Aggregator) clonePeopleHistories() []sparseHistory {
	if a.peopleNumber == 0 {
		return nil
	}

	// Convert map[int]sparseHistory → []sparseHistory indexed by author.
	maxAuthor := 0

	for author := range a.peopleHistories {
		if author > maxAuthor {
			maxAuthor = author
		}
	}

	result := make([]sparseHistory, maxAuthor+1)

	for author, history := range a.peopleHistories {
		result[author] = cloneSparseHistory(history)
	}

	return result
}

func (a *Aggregator) cloneMatrix() []map[int]int64 {
	if len(a.matrix) == 0 {
		return nil
	}

	result := make([]map[int]int64, len(a.matrix))

	for i, row := range a.matrix {
		if row == nil {
			continue
		}

		clone := make(map[int]int64, len(row))
		maps.Copy(clone, row)

		result[i] = clone
	}

	return result
}

func (a *Aggregator) cloneFileHistories() map[PathID]sparseHistory {
	if !a.trackFiles || len(a.fileHistories) == 0 {
		return nil
	}

	result := make(map[PathID]sparseHistory, len(a.fileHistories))

	for pathID, history := range a.fileHistories {
		result[pathID] = cloneSparseHistory(history)
	}

	return result
}

// spillSnapshot holds all aggregator state for disk spilling.
type spillSnapshot struct {
	GlobalHistory   sparseHistory
	PeopleHistories map[int]sparseHistory
	Matrix          []map[int]int64
	FileHistories   map[PathID]sparseHistory
}

// Spill writes accumulated state to disk to free memory.
func (a *Aggregator) Spill() (int64, error) {
	if len(a.globalHistory) == 0 {
		return 0, nil
	}

	sizeBefore := a.EstimatedStateSize()

	err := a.ensureSpillDir()
	if err != nil {
		return 0, err
	}

	snap := &spillSnapshot{
		GlobalHistory:   a.globalHistory,
		PeopleHistories: a.peopleHistories,
		Matrix:          a.matrix,
		FileHistories:   a.fileHistories,
	}

	path := filepath.Join(a.spillDir, fmt.Sprintf("agg_%03d.gob", a.spillN))

	f, err := os.Create(path)
	if err != nil {
		return 0, fmt.Errorf("burndown aggregator: create spill: %w", err)
	}

	err = gob.NewEncoder(f).Encode(snap)

	closeErr := f.Close()

	if err != nil {
		return 0, fmt.Errorf("burndown aggregator: encode spill: %w", err)
	}

	if closeErr != nil {
		return 0, fmt.Errorf("burndown aggregator: close spill: %w", closeErr)
	}

	a.spillN++
	a.globalHistory = sparseHistory{}
	a.peopleHistories = map[int]sparseHistory{}
	a.matrix = nil
	a.fileHistories = map[PathID]sparseHistory{}

	return sizeBefore, nil
}

func (a *Aggregator) ensureSpillDir() error {
	if a.spillDir != "" {
		return nil
	}

	dir := a.opts.SpillDir
	if dir == "" {
		d, err := os.MkdirTemp("", "codefang-burndown-agg-*")
		if err != nil {
			return fmt.Errorf("burndown aggregator: create spill dir: %w", err)
		}

		dir = d
	}

	a.spillDir = dir

	return nil
}

// Collect reloads previously spilled state back into memory.
func (a *Aggregator) Collect() error {
	for i := range a.spillN {
		path := filepath.Join(a.spillDir, fmt.Sprintf("agg_%03d.gob", i))

		snap, err := readSpill(path)
		if err != nil {
			return err
		}

		mergeSparseHistory(a.globalHistory, snap.GlobalHistory)
		mergePeopleHistories(a.peopleHistories, snap.PeopleHistories)
		mergeMatrixInto(&a.matrix, snap.Matrix)

		for pathID, history := range snap.FileHistories {
			if a.fileHistories[pathID] == nil {
				a.fileHistories[pathID] = sparseHistory{}
			}

			mergeSparseHistory(a.fileHistories[pathID], history)
		}
	}

	a.cleanupSpillFiles()

	return nil
}

func readSpill(path string) (*spillSnapshot, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, fmt.Errorf("burndown aggregator: open spill: %w", err)
	}

	defer f.Close()

	var snap spillSnapshot

	err = gob.NewDecoder(f).Decode(&snap)
	if err != nil {
		return nil, fmt.Errorf("burndown aggregator: decode spill: %w", err)
	}

	return &snap, nil
}

func (a *Aggregator) cleanupSpillFiles() {
	if a.spillDir != "" && a.opts.SpillDir == "" {
		os.RemoveAll(a.spillDir)
	}

	a.spillDir = ""
	a.spillN = 0
}

// EstimatedStateSize returns the current in-memory footprint in bytes.
func (a *Aggregator) EstimatedStateSize() int64 {
	var size int64

	size += estimateSparseHistorySize(a.globalHistory)

	for _, history := range a.peopleHistories {
		size += estimateSparseHistorySize(history)
	}

	for _, row := range a.matrix {
		size += int64(len(row)) * matrixRowBytes
	}

	for _, history := range a.fileHistories {
		size += estimateSparseHistorySize(history)
	}

	return size
}

func estimateSparseHistorySize(history sparseHistory) int64 {
	var size int64

	for _, inner := range history {
		size += int64(len(inner)) * sparseEntryBytes
	}

	return size
}

// SpillState returns the current on-disk spill state for checkpoint persistence.
func (a *Aggregator) SpillState() analyze.AggregatorSpillInfo {
	return analyze.AggregatorSpillInfo{Dir: a.spillDir, Count: a.spillN}
}

// RestoreSpillState points the aggregator at a previously-saved spill directory.
func (a *Aggregator) RestoreSpillState(info analyze.AggregatorSpillInfo) {
	a.spillDir = info.Dir
	a.spillN = info.Count
}

// Close releases all resources. Idempotent.
func (a *Aggregator) Close() error {
	if a.closed {
		return nil
	}

	a.closed = true
	a.cleanupSpillFiles()

	return nil
}

// ticksToReport converts aggregated TICKs into the analyze.Report format
// that ParseReportData()/ComputeAllMetrics() understand.
func ticksToReport(
	_ context.Context,
	ticks []analyze.TICK,
	granularity, sampling, peopleNumber int,
	trackFiles bool,
	tickSize time.Duration,
	reversedPeopleDict []string,
	pathInterner *PathInterner,
) analyze.Report {
	// Merge all TICK data into one accumulated state.
	merged := mergeAllTicks(ticks)
	if merged == nil {
		return analyze.Report{}
	}

	// Use a temporary analyzer to convert sparse → dense via groupSparseHistory.
	converter := &HistoryAnalyzer{
		Granularity: granularity,
		Sampling:    sampling,
	}

	lastTick := findLastTick(ticks)
	endTime := findEndTime(ticks)

	// Convert global history.
	globalDense := converter.groupSparseHistory(merged.GlobalHistory, lastTick)

	report := analyze.Report{
		"GlobalHistory":      globalDense,
		"ReversedPeopleDict": reversedPeopleDict,
		"TickSize":           tickSize,
		"Sampling":           sampling,
		"Granularity":        granularity,
		"EndTime":            endTime,
	}

	// Convert people histories.
	if peopleNumber > 0 && len(merged.PeopleHistories) > 0 {
		addPeopleToReport(report, merged, converter, lastTick, peopleNumber)
	}

	// Convert file histories.
	if trackFiles && len(merged.FileHistories) > 0 {
		addFilesToReport(report, merged, converter, lastTick, pathInterner)
	}

	return report
}

func mergeAllTicks(ticks []analyze.TICK) *TickResult {
	var merged *TickResult

	for _, tick := range ticks {
		tr, ok := tick.Data.(*TickResult)
		if !ok || tr == nil {
			continue
		}

		if merged == nil {
			merged = &TickResult{
				GlobalHistory:   sparseHistory{},
				PeopleHistories: nil,
				FileHistories:   map[PathID]sparseHistory{},
			}
		}

		mergeSparseHistory(merged.GlobalHistory, tr.GlobalHistory)
		mergeTickPeopleHistories(merged, tr.PeopleHistories)
		mergeMatrixInto(&merged.Matrix, tr.Matrix)

		for pathID, history := range tr.FileHistories {
			if merged.FileHistories[pathID] == nil {
				merged.FileHistories[pathID] = sparseHistory{}
			}

			mergeSparseHistory(merged.FileHistories[pathID], history)
		}
	}

	return merged
}

func mergeTickPeopleHistories(merged *TickResult, src []sparseHistory) {
	for author, history := range src {
		if len(history) == 0 {
			continue
		}

		for len(merged.PeopleHistories) <= author {
			merged.PeopleHistories = append(merged.PeopleHistories, nil)
		}

		if merged.PeopleHistories[author] == nil {
			merged.PeopleHistories[author] = sparseHistory{}
		}

		mergeSparseHistory(merged.PeopleHistories[author], history)
	}
}

func findLastTick(ticks []analyze.TICK) int {
	lastTick := 0

	for _, tick := range ticks {
		tr, ok := tick.Data.(*TickResult)
		if !ok || tr == nil {
			continue
		}

		for t := range tr.GlobalHistory {
			if t > lastTick {
				lastTick = t
			}
		}
	}

	return lastTick
}

func findEndTime(ticks []analyze.TICK) time.Time {
	var endTime time.Time

	for _, tick := range ticks {
		if !tick.EndTime.IsZero() && tick.EndTime.After(endTime) {
			endTime = tick.EndTime
		}
	}

	return endTime
}

func addPeopleToReport(
	report analyze.Report,
	merged *TickResult,
	converter *HistoryAnalyzer,
	lastTick, peopleNumber int,
) {
	peopleHistories := make([]DenseHistory, len(merged.PeopleHistories))

	for author, history := range merged.PeopleHistories {
		if len(history) == 0 {
			continue
		}

		peopleHistories[author] = converter.groupSparseHistory(history, lastTick)
	}

	report["PeopleHistories"] = peopleHistories
	report["PeopleMatrix"] = buildDenseMatrix(merged.Matrix, peopleNumber)
}

// buildDenseMatrix converts sparse matrix []map[int]int64 to dense DenseHistory.
// The matrix rows are indexed by oldAuthor, columns by newAuthor.
// Column 0 = authorSelf, column 1+ = author IDs shifted by 2.
func buildDenseMatrix(sparse []map[int]int64, peopleNumber int) DenseHistory {
	if len(sparse) == 0 {
		return nil
	}

	// Determine column count: 2 + peopleNumber (self column + padding + one per author).
	cols := peopleNumber + 2 //nolint:mnd // self + padding + authors.

	dense := make(DenseHistory, len(sparse))

	for author, row := range sparse {
		if len(row) == 0 {
			continue
		}

		denseRow := make([]int64, cols)

		for other, count := range row {
			col := mapMatrixColumn(other)
			if col >= 0 && col < cols {
				denseRow[col] += count
			}
		}

		dense[author] = denseRow
	}

	return dense
}

// mapMatrixColumn converts a sparse matrix key to a dense column index.
// authorSelf → 0, regular author IDs → author + 2.
func mapMatrixColumn(key int) int {
	if key == authorSelf {
		return 0
	}

	return key + modifierIndexOffset
}

func addFilesToReport(
	report analyze.Report,
	merged *TickResult,
	converter *HistoryAnalyzer,
	lastTick int,
	pathInterner *PathInterner,
) {
	fileHistories := make(map[string]DenseHistory, len(merged.FileHistories))
	fileOwnership := make(map[string]map[int]int, len(merged.FileHistories))

	// Collect and sort PathIDs for deterministic output.
	pathIDs := make([]PathID, 0, len(merged.FileHistories))
	for pathID := range merged.FileHistories {
		pathIDs = append(pathIDs, pathID)
	}

	slices.Sort(pathIDs)

	for _, pathID := range pathIDs {
		history := merged.FileHistories[pathID]
		if len(history) == 0 {
			continue
		}

		name := pathInterner.Lookup(pathID)
		if name == "" {
			continue
		}

		fileHistories[name] = converter.groupSparseHistory(history, lastTick)

		ownership := computeFileOwnership(history)
		if len(ownership) > 0 {
			fileOwnership[name] = ownership
		}
	}

	report["FileHistories"] = fileHistories
	report["FileOwnership"] = fileOwnership
}

// computeFileOwnership extracts per-author line ownership from the last tick
// of a file's sparse history. The "last tick" is the highest curTick key.
func computeFileOwnership(history sparseHistory) map[int]int {
	if len(history) == 0 {
		return nil
	}

	// Find the last (highest) tick.
	lastTick := -1
	for tick := range history {
		if tick > lastTick {
			lastTick = tick
		}
	}

	inner := history[lastTick]
	if len(inner) == 0 {
		return nil
	}

	// Sum line counts per author from the inner map.
	// In global/file history, prevTick is used to track age bands,
	// not author identity. For ownership, we look at which author bands
	// have positive line counts.
	ownership := map[int]int{}

	for prevTick, count := range inner {
		if count > 0 {
			ownership[prevTick] += int(count)
		}
	}

	// Filter out non-author entries (AuthorMissing sentinel).
	delete(ownership, identity.AuthorMissing)

	return ownership
}
