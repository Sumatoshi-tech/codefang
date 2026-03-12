package analyze

import (
	"fmt"
	"io"
	"os"
	"sort"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/version"
)

// ReportKeyCommitMeta is the Report key that carries per-commit metadata
// (timestamp, author) for timeseries output enrichment.
const ReportKeyCommitMeta = "commit_meta"

// OutputHistoryResults outputs the results for all selected history leaves.
func OutputHistoryResults(
	leaves []HistoryAnalyzer,
	results map[HistoryAnalyzer]Report,
	format string,
	writer io.Writer,
) error {
	if writer == nil {
		writer = os.Stdout
	}

	// NDJSON output is written per-TC by StreamingSink during pipeline execution.
	if format == FormatNDJSON {
		return nil
	}

	if format == FormatTimeSeries || format == FormatTimeSeriesNDJSON {
		return outputMergedTimeSeries(leaves, results, format, writer)
	}

	rawOutput := format == FormatJSON || format == FormatBinary
	if !rawOutput {
		PrintHeader(writer)
	}

	for _, leaf := range leaves {
		res := results[leaf]
		if res == nil {
			continue
		}

		if !rawOutput {
			fmt.Fprintf(writer, "%s:\n", leaf.Name())
		}

		serializeErr := leaf.Serialize(res, format, writer)
		if serializeErr != nil {
			return fmt.Errorf("serialization error for %s: %w", leaf.Name(), serializeErr)
		}
	}

	return nil
}

// outputMergedTimeSeries builds and writes a unified time-series from all analyzer reports.
// Analyzers that implement CommitTimeSeriesProvider contribute per-commit data.
// Commit ordering comes from commits_by_tick + commit_meta injected by the Runner.
func outputMergedTimeSeries(
	leaves []HistoryAnalyzer,
	results map[HistoryAnalyzer]Report,
	format string,
	writer io.Writer,
) error {
	active := collectProviderData(leaves, results)
	commitMeta := buildOrderedCommitMeta(leaves, results)

	ts := BuildMergedTimeSeriesDirect(active, commitMeta, 0)

	if format == FormatTimeSeriesNDJSON {
		return WriteTimeSeriesNDJSON(ts, writer)
	}

	return WriteMergedTimeSeries(ts, writer)
}

// collectProviderData iterates leaves sorted by flag, type-asserts each to
// CommitTimeSeriesProvider, and collects non-empty per-commit data.
func collectProviderData(
	leaves []HistoryAnalyzer,
	results map[HistoryAnalyzer]Report,
) []AnalyzerData {
	// Sort leaves by flag for deterministic output ordering.
	sorted := make([]HistoryAnalyzer, len(leaves))
	copy(sorted, leaves)

	sort.Slice(sorted, func(i, j int) bool {
		return sorted[i].Flag() < sorted[j].Flag()
	})

	var active []AnalyzerData

	for _, leaf := range sorted {
		provider, ok := leaf.(CommitTimeSeriesProvider)
		if !ok {
			continue
		}

		report := results[leaf]
		if report == nil {
			continue
		}

		data := provider.ExtractCommitTimeSeries(report)
		if len(data) == 0 {
			continue
		}

		active = append(active, AnalyzerData{Flag: leaf.Flag(), Data: data})
	}

	return active
}

// buildOrderedCommitMeta builds an ordered slice of CommitMeta from the
// commits_by_tick and commit_meta data injected into Reports by the Runner.
func buildOrderedCommitMeta(
	leaves []HistoryAnalyzer,
	results map[HistoryAnalyzer]Report,
) []CommitMeta {
	var commitsByTick map[int][]gitlib.Hash

	var commitMetaMap map[string]CommitMeta

	for _, leaf := range leaves {
		report := results[leaf]
		if report == nil {
			continue
		}

		if cbt, ok := report["commits_by_tick"].(map[int][]gitlib.Hash); ok && len(cbt) > 0 {
			commitsByTick = cbt
		}

		if cm, ok := report[ReportKeyCommitMeta].(map[string]CommitMeta); ok && len(cm) > 0 {
			commitMetaMap = cm
		}

		if commitsByTick != nil {
			break
		}
	}

	return assembleOrderedCommitMeta(commitsByTick, commitMetaMap)
}

// buildOrderedCommitMetaFromReports builds an ordered slice of CommitMeta
// from reports keyed by analyzer flag. Used by the cross-format conversion
// path which doesn't have live analyzer instances.
func buildOrderedCommitMetaFromReports(reports map[string]Report) []CommitMeta {
	var commitsByTick map[int][]gitlib.Hash

	var commitMetaMap map[string]CommitMeta

	for _, report := range reports {
		if cbt, ok := report["commits_by_tick"].(map[int][]gitlib.Hash); ok && len(cbt) > 0 {
			commitsByTick = cbt
		}

		if cm, ok := report[ReportKeyCommitMeta].(map[string]CommitMeta); ok && len(cm) > 0 {
			commitMetaMap = cm
		}

		if commitsByTick != nil {
			break
		}
	}

	return assembleOrderedCommitMeta(commitsByTick, commitMetaMap)
}

// assembleOrderedCommitMeta builds an ordered CommitMeta slice from
// commits_by_tick (for ordering) and an optional commit_meta map (for
// timestamp/author enrichment).
func assembleOrderedCommitMeta(
	commitsByTick map[int][]gitlib.Hash,
	commitMetaMap map[string]CommitMeta,
) []CommitMeta {
	if len(commitsByTick) == 0 {
		return nil
	}

	ticks := make([]int, 0, len(commitsByTick))
	for tick := range commitsByTick {
		ticks = append(ticks, tick)
	}

	sort.Ints(ticks)

	var meta []CommitMeta

	for _, tick := range ticks {
		for _, hash := range commitsByTick[tick] {
			hashStr := hash.String()
			entry := CommitMeta{
				Hash: hashStr,
				Tick: tick,
			}

			if cm, ok := commitMetaMap[hashStr]; ok {
				entry.Timestamp = cm.Timestamp
				entry.Author = cm.Author
			}

			meta = append(meta, entry)
		}
	}

	return meta
}

// PrintHeader prints the codefang version header.
func PrintHeader(writer io.Writer) {
	fmt.Fprintln(writer, "codefang (v2):")
	fmt.Fprintf(writer, "  version: %d\n", version.Binary)
	fmt.Fprintln(writer, "  hash:", version.BinaryGitHash)
}
