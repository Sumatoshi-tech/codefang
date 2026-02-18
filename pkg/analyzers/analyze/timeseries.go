package analyze

import (
	"encoding/json"
	"fmt"
	"io"
	"maps"
	"sort"
)

// TickExtractor extracts per-commit data from a finalized analyzer report.
// Returns a map of commit hash (hex string) to a JSON-serializable value
// representing that analyzer's data for the commit. Returns nil if no
// per-commit data is available.
type TickExtractor func(report Report) map[string]any

// tickExtractors maps analyzer flag names to their tick extractors.
var tickExtractors = make(map[string]TickExtractor)

// RegisterTickExtractor registers a tick extractor for the given analyzer flag.
// Analyzers call this during initialization to opt into unified time-series output.
func RegisterTickExtractor(analyzerFlag string, fn TickExtractor) {
	tickExtractors[analyzerFlag] = fn
}

// TickExtractorFor returns the registered tick extractor for the given flag, or nil.
func TickExtractorFor(analyzerFlag string) TickExtractor {
	return tickExtractors[analyzerFlag]
}

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

// analyzerData pairs an analyzer flag with its per-commit extracted data.
type analyzerData struct {
	flag string
	data map[string]any
}

// BuildMergedTimeSeries builds a unified time-series from multiple analyzer
// reports. It iterates registered TickExtractors, collects per-commit data
// from each analyzer, and merges them into MergedCommitData entries ordered
// by the commit metadata sequence.
func BuildMergedTimeSeries(
	reports map[string]Report,
	commitMeta []CommitMeta,
	tickSizeHours float64,
) *MergedTimeSeries {
	if tickSizeHours <= 0 {
		tickSizeHours = defaultTickSizeHours
	}

	active := collectActiveExtractors(reports)

	analyzerNames := make([]string, len(active))

	for i, a := range active {
		analyzerNames[i] = a.flag
	}

	commits := assembleCommits(active, commitMeta)

	return &MergedTimeSeries{
		Version:       TimeSeriesModelVersion,
		TickSizeHours: tickSizeHours,
		Analyzers:     analyzerNames,
		Commits:       commits,
	}
}

// collectActiveExtractors runs registered TickExtractors against the provided
// reports and returns only those that produced non-empty per-commit data.
func collectActiveExtractors(reports map[string]Report) []analyzerData {
	var active []analyzerData

	flags := sortedReportKeys(reports)

	for _, flag := range flags {
		extractor := TickExtractorFor(flag)
		if extractor == nil {
			continue
		}

		commitData := extractor(reports[flag])
		if len(commitData) == 0 {
			continue
		}

		active = append(active, analyzerData{flag: flag, data: commitData})
	}

	return active
}

// assembleCommits merges per-analyzer data into ordered MergedCommitData entries.
func assembleCommits(active []analyzerData, commitMeta []CommitMeta) []MergedCommitData {
	metaByHash := make(map[string]CommitMeta, len(commitMeta))
	for _, m := range commitMeta {
		metaByHash[m.Hash] = m
	}

	commitSet := make(map[string]struct{})

	for _, a := range active {
		for hash := range a.data {
			commitSet[hash] = struct{}{}
		}
	}

	ordered := orderCommitsByMeta(commitMeta, commitSet)
	commits := make([]MergedCommitData, len(ordered))

	for i, hash := range ordered {
		meta := metaByHash[hash]
		analyzerMap := make(map[string]any, len(active))

		for _, a := range active {
			if v, ok := a.data[hash]; ok {
				analyzerMap[a.flag] = v
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

func sortedReportKeys(m map[string]Report) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}

	sort.Strings(keys)

	return keys
}
