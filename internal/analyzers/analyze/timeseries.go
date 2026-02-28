package analyze

import (
	"encoding/json"
	"fmt"
	"io"
	"maps"
)

// MergedCommitData holds merged analyzer data for a single commit.
type MergedCommitData struct {
	Hash      string         `json:"hash"`
	Timestamp string         `json:"timestamp"`
	Author    string         `json:"author"`
	Tick      int            `json:"tick"`
	Analyzers map[string]any `json:"-"`
}

// MarshalJSON flattens commit metadata and per-analyzer data into a single object:
// {"hash": "...", "timestamp": "...", "author": "...", "tick": N, "quality": {...}, ...}.
func (m MergedCommitData) MarshalJSON() ([]byte, error) {
	flat := make(map[string]any, len(m.Analyzers)+4) //nolint:mnd // 4 metadata fields
	flat["hash"] = m.Hash
	flat["timestamp"] = m.Timestamp
	flat["author"] = m.Author
	flat["tick"] = m.Tick

	maps.Copy(flat, m.Analyzers)

	data, err := json.Marshal(flat)
	if err != nil {
		return nil, fmt.Errorf("marshal merged commit data: %w", err)
	}

	return data, nil
}

// MergedTimeSeries is the top-level unified time-series output structure.
type MergedTimeSeries struct {
	Version       string             `json:"version"`
	TickSizeHours float64            `json:"tick_size_hours"`
	Analyzers     []string           `json:"analyzers"`
	Commits       []MergedCommitData `json:"commits"`
}

// TimeSeriesModelVersion is the schema version for unified time-series output.
const TimeSeriesModelVersion = "codefang.timeseries.v1"

// defaultTickSizeHours is the fallback tick duration when not specified.
const defaultTickSizeHours = 24

// CommitMeta carries per-commit metadata for time-series construction.
// Analyzers populate this during Consume() from the analyze.Context.
type CommitMeta struct {
	Hash      string `json:"hash"`
	Timestamp string `json:"timestamp"`
	Author    string `json:"author"`
	Tick      int    `json:"tick"`
}

// CommitTimeSeriesProvider is implemented by analyzers that contribute
// per-commit data to the unified time-series output (--format timeseries).
// Replaces the global TickExtractor registry with compile-time interface dispatch.
type CommitTimeSeriesProvider interface {
	// ExtractCommitTimeSeries extracts per-commit data from a finalized report.
	// Returns a map of commit hash (hex string) to a JSON-serializable value.
	// Returns nil if no per-commit data is available.
	ExtractCommitTimeSeries(report Report) map[string]any
}

// AnalyzerData pairs an analyzer flag with its per-commit extracted data.
type AnalyzerData struct {
	Flag string
	Data map[string]any
}

// BuildMergedTimeSeriesDirect builds a unified time-series from pre-extracted
// per-analyzer commit data. Callers collect AnalyzerData via
// CommitTimeSeriesProvider.ExtractCommitTimeSeries on each leaf analyzer.
func BuildMergedTimeSeriesDirect(
	active []AnalyzerData,
	commitMeta []CommitMeta,
	tickSizeHours float64,
) *MergedTimeSeries {
	if tickSizeHours <= 0 {
		tickSizeHours = defaultTickSizeHours
	}

	analyzerNames := make([]string, len(active))

	for i, a := range active {
		analyzerNames[i] = a.Flag
	}

	commits := assembleCommits(active, commitMeta)

	return &MergedTimeSeries{
		Version:       TimeSeriesModelVersion,
		TickSizeHours: tickSizeHours,
		Analyzers:     analyzerNames,
		Commits:       commits,
	}
}

// assembleCommits merges per-analyzer data into ordered MergedCommitData entries.
func assembleCommits(active []AnalyzerData, commitMeta []CommitMeta) []MergedCommitData {
	metaByHash := make(map[string]CommitMeta, len(commitMeta))
	for _, m := range commitMeta {
		metaByHash[m.Hash] = m
	}

	commitSet := make(map[string]struct{})

	for _, a := range active {
		for hash := range a.Data {
			commitSet[hash] = struct{}{}
		}
	}

	ordered := orderCommitsByMeta(commitMeta, commitSet)
	commits := make([]MergedCommitData, len(ordered))

	for i, hash := range ordered {
		meta := metaByHash[hash]
		analyzerMap := make(map[string]any, len(active))

		for _, a := range active {
			if v, ok := a.Data[hash]; ok {
				analyzerMap[a.Flag] = v
			}
		}

		commits[i] = MergedCommitData{
			Hash:      meta.Hash,
			Timestamp: meta.Timestamp,
			Author:    meta.Author,
			Tick:      meta.Tick,
			Analyzers: analyzerMap,
		}
	}

	return commits
}

// WriteMergedTimeSeries encodes a MergedTimeSeries as indented JSON to the writer.
func WriteMergedTimeSeries(ts *MergedTimeSeries, writer io.Writer) error {
	encoder := json.NewEncoder(writer)
	encoder.SetIndent("", "  ")

	err := encoder.Encode(ts)
	if err != nil {
		return fmt.Errorf("encode merged timeseries: %w", err)
	}

	return nil
}

// WriteTimeSeriesNDJSON writes a MergedTimeSeries as NDJSON — one JSON line per commit.
func WriteTimeSeriesNDJSON(ts *MergedTimeSeries, writer io.Writer) error {
	encoder := json.NewEncoder(writer)

	for i := range ts.Commits {
		err := encoder.Encode(ts.Commits[i])
		if err != nil {
			return fmt.Errorf("encode timeseries ndjson line: %w", err)
		}
	}

	return nil
}

// orderCommitsByMeta returns commit hashes in the order they appear in
// commitMeta, filtering to only those present in commitSet.
func orderCommitsByMeta(meta []CommitMeta, commitSet map[string]struct{}) []string {
	ordered := make([]string, 0, len(commitSet))

	for _, m := range meta {
		if _, ok := commitSet[m.Hash]; ok {
			ordered = append(ordered, m.Hash)
		}
	}

	return ordered
}
