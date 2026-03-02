package shotness

import (
	"maps"
	"sort"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func newAggregator(opts analyze.AggregatorOptions) analyze.Aggregator {
	agg := analyze.NewGenericAggregator[*TickData, *TickData](
		opts,
		extractTC,
		mergeState,
		sizeState,
		buildTick,
	)
	agg.DrainCommitDataFn = drainShotnessCommitData

	return agg
}

func drainShotnessCommitData(state *TickData) (stats map[string]any, tickHashes map[int][]gitlib.Hash) {
	if state == nil || len(state.CommitStats) == 0 {
		return nil, nil
	}

	result := make(map[string]any, len(state.CommitStats))
	for hash, cs := range state.CommitStats {
		result[hash] = cs.toMap()
	}

	state.CommitStats = make(map[string]*CommitSummary)

	return result, nil
}

func extractTC(tc analyze.TC, byTick map[int]*TickData) error {
	cd, ok := tc.Data.(*CommitData)
	if !ok || cd == nil {
		return nil
	}

	acc := getOrCreateTickData(byTick, tc.Tick)
	accumulateNodes(acc, cd.NodesTouched)
	couplingPairs := computeCouplingPairs(acc, cd.NodesTouched)

	if !tc.CommitHash.IsZero() {
		acc.CommitStats[tc.CommitHash.String()] = &CommitSummary{
			NodesTouched:  len(cd.NodesTouched),
			CouplingPairs: couplingPairs,
		}
	}

	return nil
}

// getOrCreateTickData returns the TickData for the given tick, creating it if needed.
func getOrCreateTickData(byTick map[int]*TickData, tick int) *TickData {
	acc, ok := byTick[tick]
	if !ok {
		acc = &TickData{
			Nodes:       make(map[string]*nodeShotnessData),
			CommitStats: make(map[string]*CommitSummary),
		}
		byTick[tick] = acc
	}

	return acc
}

// accumulateNodes merges per-commit node deltas into the tick accumulator.
func accumulateNodes(acc *TickData, nodesTouched map[string]NodeDelta) {
	for key, delta := range nodesTouched {
		nd, exists := acc.Nodes[key]
		if !exists {
			nd = &nodeShotnessData{
				Summary: delta.Summary,
				Couples: make(map[string]int),
			}
			acc.Nodes[key] = nd
		}

		nd.Count += delta.CountDelta
	}
}

// computeCouplingPairs computes coupling pairs from the touched nodes and
// updates the accumulator's coupling maps. Skips the O(N^2) map updates
// when too many nodes are touched (mass refactor -- coupling signal is noise).
func computeCouplingPairs(acc *TickData, nodesTouched map[string]NodeDelta) int {
	n := len(nodesTouched)
	if n < minCouplingNodes {
		return 0
	}

	couplingPairs := n * (n - 1) / combinatorialPairDivisor

	if n > maxCouplingNodes {
		return couplingPairs
	}

	keys := make([]string, 0, n)
	for key := range nodesTouched {
		keys = append(keys, key)
	}

	sort.Strings(keys)

	for i, key1 := range keys {
		for _, key2 := range keys[i+1:] {
			if nd, exists := acc.Nodes[key1]; exists {
				nd.Couples[key2]++
			}

			if nd, exists := acc.Nodes[key2]; exists {
				nd.Couples[key1]++
			}
		}
	}

	return couplingPairs
}

func mergeState(existing, incoming *TickData) *TickData {
	if existing == nil {
		return incoming
	}

	if incoming == nil {
		return existing
	}

	if existing.Nodes == nil {
		existing.Nodes = make(map[string]*nodeShotnessData)
	}

	if existing.CommitStats == nil {
		existing.CommitStats = make(map[string]*CommitSummary)
	}

	maps.Copy(existing.CommitStats, incoming.CommitStats)
	mergeNodesInto(existing.Nodes, incoming.Nodes)

	return existing
}

func sizeState(state *TickData) int64 {
	if state == nil {
		return 0
	}

	const (
		overheadPerNode   int64 = 150
		overheadPerCouple int64 = 50
	)

	var size int64
	for _, nd := range state.Nodes {
		size += overheadPerNode
		size += int64(len(nd.Couples)) * overheadPerCouple
	}

	return size
}

func buildTick(tick int, state *TickData) (analyze.TICK, error) {
	if state == nil {
		return analyze.TICK{Tick: tick}, nil
	}

	return analyze.TICK{
		Tick: tick,
		Data: state,
	}, nil
}
