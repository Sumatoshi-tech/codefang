package filehistory

import (
	"context"
	"errors"
	"fmt"
	"sort"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// Store record kind constants.
const (
	KindFileChurn = "file_churn"
	KindSummary   = "summary"
)

// ErrUnexpectedAggregator indicates a type assertion failure for the aggregator.
var ErrUnexpectedAggregator = errors.New("unexpected aggregator type: expected *file_history.Aggregator")

// WriteToStoreFromAggregator implements analyze.DirectStoreWriter.
// It handles Collect() and accesses the aggregator's SpillStore directly,
// avoiding the [maps.Copy] in FlushTick. Computes per-file churn data
// and streams it as individual records:
//   - "file_churn": per-file FileChurnData records (sorted by churn score).
//   - "summary": single AggregateData record.
func (h *HistoryAnalyzer) WriteToStoreFromAggregator(
	ctx context.Context,
	agg analyze.Aggregator,
	w analyze.ReportWriter,
) error {
	collectErr := agg.Collect()
	if collectErr != nil {
		return fmt.Errorf("collect: %w", collectErr)
	}

	fa, ok := agg.(*Aggregator)
	if !ok {
		return ErrUnexpectedAggregator
	}

	files := fa.files.Current()

	// Filter by last commit tree if repo is available.
	if !fa.lastCommitHash.IsZero() && h.repo != nil {
		files = filterFilesByLastCommit(ctx, h.repo, fa.lastCommitHash, files)
	}

	churnData := computeFileChurnFromFiles(files)

	churnErr := writeFileChurn(w, churnData)
	if churnErr != nil {
		return fmt.Errorf("write %s: %w", KindFileChurn, churnErr)
	}

	aggregate := computeAggregateFromFiles(files)

	summaryErr := w.Write(KindSummary, aggregate)
	if summaryErr != nil {
		return fmt.Errorf("write %s: %w", KindSummary, summaryErr)
	}

	return nil
}

// computeFileChurnFromFiles computes FileChurnData for each file,
// sorted by churn score descending.
func computeFileChurnFromFiles(files map[string]FileHistory) []FileChurnData {
	result := make([]FileChurnData, 0, len(files))

	for path, fh := range files {
		var totalAdded, totalRemoved, totalChanged int

		for _, stats := range fh.People {
			totalAdded += stats.Added
			totalRemoved += stats.Removed
			totalChanged += stats.Changed
		}

		commitCount := len(fh.Hashes)
		contributorCount := len(fh.People)
		churnScore := float64(commitCount) + float64(totalAdded+totalRemoved+totalChanged)/churnScoreDivisor

		result = append(result, FileChurnData{
			Path:             path,
			CommitCount:      commitCount,
			ContributorCount: contributorCount,
			TotalAdded:       totalAdded,
			TotalRemoved:     totalRemoved,
			TotalChanged:     totalChanged,
			ChurnScore:       churnScore,
		})
	}

	sort.Slice(result, func(i, j int) bool {
		return result[i].ChurnScore > result[j].ChurnScore
	})

	return result
}

// computeAggregateFromFiles computes summary statistics from the files map.
func computeAggregateFromFiles(files map[string]FileHistory) AggregateData {
	agg := AggregateData{
		TotalFiles: len(files),
	}

	contributors := make(map[int]struct{})
	totalCommits := 0

	for _, fh := range files {
		totalCommits += len(fh.Hashes)

		for devID := range fh.People {
			contributors[devID] = struct{}{}
		}

		if len(fh.Hashes) >= HotspotThresholdMedium {
			agg.HighChurnFiles++
		}
	}

	agg.TotalCommits = totalCommits
	agg.TotalContributors = len(contributors)

	if agg.TotalFiles > 0 {
		agg.AvgCommitsPerFile = float64(totalCommits) / float64(agg.TotalFiles)
		agg.AvgContributorsPerFile = float64(agg.TotalContributors) / float64(agg.TotalFiles)
	}

	return agg
}

// writeFileChurn writes FileChurnData records to the store.
func writeFileChurn(w analyze.ReportWriter, churn []FileChurnData) error {
	for i := range churn {
		writeErr := w.Write(KindFileChurn, churn[i])
		if writeErr != nil {
			return writeErr
		}
	}

	return nil
}
