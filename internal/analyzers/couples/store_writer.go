package couples

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"sort"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// Store record kind constants.
const (
	KindFileCoupling = "file_coupling"
	KindDevMatrix    = "dev_matrix"
	KindOwnership    = "ownership"
	KindAggregate    = "aggregate"
)

// Default limits for bounded store output.
const (
	DefaultTopKPerFile   = 100
	DefaultMinEdgeWeight = 2
	DefaultMaxDevs       = 20
)

// StoreDevMatrix holds a bounded developer coupling matrix for store serialization.
type StoreDevMatrix struct {
	Names  []string        `json:"names"`
	Matrix []map[int]int64 `json:"matrix"`
}

// ErrUnexpectedAggregator indicates a type assertion failure for the aggregator.
var ErrUnexpectedAggregator = errors.New("unexpected aggregator type: expected *couples.Aggregator")

// WriteToStoreFromAggregator implements analyze.DirectStoreWriter.
// It handles Collect() internally with filtered merging: only coupling data
// for currently-existing files is loaded. This avoids materializing the full
// O(F²) coupling map for historical files (which can reach 50+ GB on kubernetes).
// Streams bounded pre-computed data:
//   - "file_coupling": top-K file coupling pairs.
//   - "dev_matrix": bounded developer coupling matrix (top-N devs).
//   - "ownership": per-file contributor counts.
//   - "aggregate": summary statistics.
func (c *HistoryAnalyzer) WriteToStoreFromAggregator(
	ctx context.Context,
	agg analyze.Aggregator,
	w analyze.ReportWriter,
) error {
	ca, ok := agg.(*Aggregator)
	if !ok {
		return ErrUnexpectedAggregator
	}

	minWeight := c.minEdgeWeight()

	reducedFiles, reducedPeople, err := c.collectAndReduce(ctx, ca, minWeight)
	if err != nil {
		return err
	}

	filesSequence, filesIndex := buildFilesIndex(reducedFiles)

	fcErr := writeFileCoupling(w, reducedFiles, filesSequence, filesIndex, c.topKPerFile(), minWeight)
	if fcErr != nil {
		return fmt.Errorf("write file_coupling: %w", fcErr)
	}

	effectivePeople := ca.peopleNumber
	if len(reducedPeople) > effectivePeople+1 {
		effectivePeople = len(reducedPeople) - 1
	}

	peopleMatrix, peopleFiles := computePeopleMatrix(reducedPeople, filesIndex, effectivePeople)

	dmErr := writeDevMatrix(w, peopleMatrix, ca.reversedNames, DefaultMaxDevs)
	if dmErr != nil {
		return fmt.Errorf("write dev_matrix: %w", dmErr)
	}

	owErr := writeOwnership(ctx, w, filesSequence, peopleFiles, ca.lastCommit)
	if owErr != nil {
		return fmt.Errorf("write ownership: %w", owErr)
	}

	aggErr := writeAggregate(w, reducedFiles, filesSequence, filesIndex, ca.reversedNames)
	if aggErr != nil {
		return fmt.Errorf("write aggregate: %w", aggErr)
	}

	return nil
}

// collectAndReduce gathers file coupling and people data, either through filtered
// spill merging (when lastCommit is available) or standard unfiltered collect.
func (c *HistoryAnalyzer) collectAndReduce(
	ctx context.Context,
	ca *Aggregator,
	minWeight int64,
) (files map[string]map[string]int, people []map[string]int, err error) {
	currentFiles := collectCurrentFilesFromTree(ctx, ca.lastCommit)

	if currentFiles != nil {
		return c.collectFiltered(ctx, ca, currentFiles, minWeight)
	}

	return c.collectUnfiltered(ctx, ca)
}

// collectFiltered merges spill files keeping only entries for currently-existing files.
func (c *HistoryAnalyzer) collectFiltered(
	ctx context.Context,
	ca *Aggregator,
	currentFiles map[string]bool,
	minWeight int64,
) (files map[string]map[string]int, people []map[string]int, err error) {
	matcher := newFileMatchFunc(currentFiles)

	reducedFiles, err := ca.collectFilteredFiles(currentFiles, matcher, int(minWeight))
	if err != nil {
		return nil, nil, fmt.Errorf("collect filtered: %w", err)
	}

	reducedPeople := reducePeople(ca.people, matcher)

	var totalEntries int64
	for _, lane := range reducedFiles {
		totalEntries += int64(len(lane))
	}

	slog.Default().InfoContext(ctx, "couples filtered collect complete",
		"files", len(reducedFiles),
		"total_entries", totalEntries,
		"spill_count", ca.files.SpillCount(),
		"people_count", len(ca.people),
	)

	return reducedFiles, reducedPeople, nil
}

// collectUnfiltered performs standard unfiltered collect when no lastCommit is available.
func (c *HistoryAnalyzer) collectUnfiltered(
	ctx context.Context,
	ca *Aggregator,
) (files map[string]map[string]int, people []map[string]int, err error) {
	collectErr := ca.Collect()
	if collectErr != nil {
		return nil, nil, fmt.Errorf("collect: %w", collectErr)
	}

	rawFiles := ca.files.Current()
	allFiles := collectCurrentFiles(ctx, rawFiles, ca.lastCommit)
	matcher := newFileMatchFunc(allFiles)

	reducedFiles := reduceFiles(rawFiles, allFiles, matcher)
	reducedPeople := reducePeople(ca.people, matcher)

	return reducedFiles, reducedPeople, nil
}

// topKPerFile returns the configured TopK or the default.
func (c *HistoryAnalyzer) topKPerFile() int {
	if c.TopKPerFile > 0 {
		return c.TopKPerFile
	}

	return DefaultTopKPerFile
}

// minEdgeWeight returns the configured MinEdgeWeight or the default.
func (c *HistoryAnalyzer) minEdgeWeight() int64 {
	if c.MinEdgeWeight > 0 {
		return c.MinEdgeWeight
	}

	return DefaultMinEdgeWeight
}

// writeFileCoupling computes top-K file coupling pairs from the sparse map
// and writes them as individual "file_coupling" records.
func writeFileCoupling(
	w analyze.ReportWriter,
	reducedFiles map[string]map[string]int,
	filesSequence []string,
	filesIndex map[string]int,
	topK int,
	minWeight int64,
) error {
	pairs := computeSparseCoupling(reducedFiles, filesSequence, filesIndex, minWeight)

	sort.Slice(pairs, func(i, j int) bool {
		return pairs[i].CoChanges > pairs[j].CoChanges
	})

	limit := min(len(pairs), topK)

	for i := range limit {
		writeErr := w.Write(KindFileCoupling, pairs[i])
		if writeErr != nil {
			return writeErr
		}
	}

	return nil
}

// computeSparseCoupling extracts file coupling pairs from the sparse coupling map
// without materializing a dense matrix. Only pairs with count >= minWeight
// are included.
func computeSparseCoupling(
	reducedFiles map[string]map[string]int,
	filesSequence []string,
	filesIndex map[string]int,
	minWeight int64,
) []FileCouplingData {
	var result []FileCouplingData

	for i, file1 := range filesSequence {
		lane := reducedFiles[file1]
		if len(lane) == 0 {
			continue
		}

		selfI := int64(lane[file1])

		for file2, coChanges := range lane {
			j, exists := filesIndex[file2]
			if !exists || j <= i {
				continue // Skip self and lower triangle.
			}

			coChanges64 := int64(coChanges)
			if coChanges64 < minWeight {
				continue
			}

			selfJ := int64(reducedFiles[file2][file2])

			avgRevs := float64(selfI+selfJ) / pairCount

			var strength float64
			if avgRevs > 0 {
				strength = min(float64(coChanges64)/avgRevs, 1.0)
			}

			result = append(result, FileCouplingData{
				File1:     file1,
				File2:     file2,
				CoChanges: coChanges64,
				Strength:  strength,
			})
		}
	}

	return result
}

// writeDevMatrix writes a bounded developer coupling matrix as a single "dev_matrix" record.
func writeDevMatrix(
	w analyze.ReportWriter,
	matrix []map[int]int64,
	reversedNames []string,
	maxDevs int,
) error {
	filteredMatrix, filteredNames := FilterTopDevs(matrix, reversedNames, maxDevs)

	record := StoreDevMatrix{
		Names:  filteredNames,
		Matrix: filteredMatrix,
	}

	return w.Write(KindDevMatrix, record)
}

// writeOwnership writes per-file ownership data as individual "ownership" records.
func writeOwnership(
	ctx context.Context,
	w analyze.ReportWriter,
	filesSequence []string,
	peopleFiles [][]int,
	lastCommit analyze.CommitLike,
) error {
	sketches := buildFileContributorSketches(len(filesSequence), peopleFiles)
	filesLines := computeFilesLinesFromCommit(ctx, filesSequence, lastCommit)

	for i, file := range filesSequence {
		lines := 0
		if i < len(filesLines) {
			lines = filesLines[i]
		}

		contributors := 0
		if i < len(sketches) && sketches[i] != nil {
			contributors = int(sketches[i].Count())
		}

		record := FileOwnershipData{
			File:         file,
			Lines:        lines,
			Contributors: contributors,
		}

		writeErr := w.Write(KindOwnership, record)
		if writeErr != nil {
			return writeErr
		}
	}

	return nil
}

// writeAggregate writes a single summary record as the "aggregate" kind.
// Computes aggregate stats directly from the sparse coupling map
// without materializing the dense O(N²) filesMatrix.
func writeAggregate(
	w analyze.ReportWriter,
	reducedFiles map[string]map[string]int,
	filesSequence []string,
	filesIndex map[string]int,
	reversedNames []string,
) error {
	aggregate := computeSparseAggregate(reducedFiles, filesSequence, filesIndex, reversedNames)

	return w.Write(KindAggregate, aggregate)
}

// computeSparseAggregate computes AggregateData directly from the sparse coupling map.
func computeSparseAggregate(
	reducedFiles map[string]map[string]int,
	filesSequence []string,
	filesIndex map[string]int,
	reversedNames []string,
) AggregateData {
	agg := AggregateData{
		TotalFiles:      len(filesSequence),
		TotalDevelopers: len(reversedNames),
	}

	var acc aggregateAccum

	for _, file1 := range filesSequence {
		i := filesIndex[file1]
		lane := reducedFiles[file1]

		selfI := int64(lane[file1])

		for file2, coChanges := range lane {
			j, exists := filesIndex[file2]
			if !exists || j <= i || coChanges <= 0 {
				continue
			}

			selfJ := int64(reducedFiles[file2][file2])
			acc.addPair(int64(coChanges), selfI, selfJ)
		}
	}

	agg.TotalCoChanges = acc.totalCoChanges
	agg.HighlyCoupledPairs = acc.highlyCoupled

	if acc.pairCount > 0 {
		agg.AvgCouplingStrength = acc.totalStrength / float64(acc.pairCount)
	}

	return agg
}
