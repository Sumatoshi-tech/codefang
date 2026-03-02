package shotness

import (
	"context"
	"maps"
	"sort"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func ticksToReport(_ context.Context, ticks []analyze.TICK) analyze.Report {
	merged := make(map[string]*nodeShotnessData)
	commitStats := make(map[string]*CommitSummary)
	commitsByTick := make(map[int][]gitlib.Hash)

	for _, tick := range ticks {
		td, ok := tick.Data.(*TickData)
		if !ok || td == nil {
			continue
		}

		mergeNodesInto(merged, td.Nodes)

		for hash, cs := range td.CommitStats {
			commitStats[hash] = cs
			commitsByTick[tick.Tick] = append(commitsByTick[tick.Tick], gitlib.NewHash(hash))
		}
	}

	report := buildReportFromMerged(merged)

	if len(commitStats) > 0 {
		report["commit_stats"] = commitStats
		report["commits_by_tick"] = commitsByTick
	}

	return report
}

// mergeNodesInto merges source node data into the destination map,
// accumulating counts and coupling pairs.
func mergeNodesInto(dst, src map[string]*nodeShotnessData) {
	for key, nd := range src {
		existing, found := dst[key]
		if !found {
			dst[key] = &nodeShotnessData{
				Summary: nd.Summary,
				Count:   nd.Count,
				Couples: copyIntMap(nd.Couples),
			}

			continue
		}

		existing.Count += nd.Count

		for ck, cv := range nd.Couples {
			existing.Couples[ck] += cv
		}
	}
}

// buildReportFromMerged builds the Nodes/Counters report from merged node data.
func buildReportFromMerged(merged map[string]*nodeShotnessData) analyze.Report {
	nodes := make([]NodeSummary, len(merged))
	counters := make([]map[int]int, len(merged))

	keys := make([]string, 0, len(merged))
	for key := range merged {
		keys = append(keys, key)
	}

	sort.Strings(keys)

	reverseKeys := make(map[string]int, len(keys))
	for i, key := range keys {
		reverseKeys[key] = i
	}

	for i, key := range keys {
		nd := merged[key]
		nodes[i] = nd.Summary
		counter := map[int]int{}
		counters[i] = counter

		counter[i] = nd.Count

		for ck, val := range nd.Couples {
			if idx, ok := reverseKeys[ck]; ok {
				counter[idx] = val
			}
		}
	}

	return analyze.Report{
		"Nodes":    nodes,
		"Counters": counters,
	}
}

// copyIntMap creates a copy of a map[string]int.
func copyIntMap(src map[string]int) map[string]int {
	dst := make(map[string]int, len(src))
	maps.Copy(dst, src)

	return dst
}
