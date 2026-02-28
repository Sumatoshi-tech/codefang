package analyze

import (
	"fmt"
	"maps"
	"sort"
	"strconv"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/spillstore"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// GenericAggregator manages per-tick state accumulation, spilling, and collection.
// S is the tick accumulator state (e.g., *TickAccumulator).
// T is the final tick data representation (e.g., *TickData).
type GenericAggregator[S any, T any] struct {
	Opts       AggregatorOptions
	ByTick     map[int]S
	SpillStore *spillstore.SpillStore[S]

	// Delegate Hooks.
	ExtractTCFn  func(TC, map[int]S) error
	MergeStateFn func(S, S) S
	SizeStateFn  func(S) int64
	BuildTickFn  func(int, S) (TICK, error)

	// DrainCommitDataFn extracts and clears per-commit data from a tick
	// accumulator state. Returns summarized per-commit data and commit ordering.
	// When nil, DrainCommitStats returns nil (CommitStatsDrainer not satisfied).
	DrainCommitDataFn func(S) (map[string]any, map[int][]gitlib.Hash)
}

// NewGenericAggregator is a helper to create and initialize a GenericAggregator.
func NewGenericAggregator[S any, T any](
	opts AggregatorOptions,
	extractFn func(TC, map[int]S) error,
	mergeFn func(S, S) S,
	sizeFn func(S) int64,
	buildFn func(int, S) (TICK, error),
) *GenericAggregator[S, T] {
	return &GenericAggregator[S, T]{
		Opts:         opts,
		ByTick:       make(map[int]S),
		SpillStore:   spillstore.New[S](),
		ExtractTCFn:  extractFn,
		MergeStateFn: mergeFn,
		SizeStateFn:  sizeFn,
		BuildTickFn:  buildFn,
	}
}

// Add ingests a single per-commit result.
// If the internal state size exceeds SpillBudget, it triggers a Spill.
func (a *GenericAggregator[S, T]) Add(tc TC) error {
	errExtract := a.ExtractTCFn(tc, a.ByTick)
	if errExtract != nil {
		return errExtract
	}

	if a.Opts.SpillBudget > 0 && a.EstimatedStateSize() > a.Opts.SpillBudget {
		_, errSpill := a.Spill()
		if errSpill != nil {
			return errSpill
		}
	}

	return nil
}

// FlushTick finalizes and returns the aggregated result for the given tick.
func (a *GenericAggregator[S, T]) FlushTick(tick int) (TICK, error) {
	state, ok := a.ByTick[tick]
	if !ok {
		// Use a zero value for state if not found. BuildTickFn must handle it.
		var zero S

		return a.BuildTickFn(tick, zero)
	}

	return a.BuildTickFn(tick, state)
}

// FlushAllTicks returns TICKs for all ticks that have accumulated data, sorted ascending.
func (a *GenericAggregator[S, T]) FlushAllTicks() ([]TICK, error) {
	if len(a.ByTick) == 0 {
		return nil, nil
	}

	ticks := make([]int, 0, len(a.ByTick))
	for tick := range a.ByTick {
		ticks = append(ticks, tick)
	}

	sort.Ints(ticks)

	var result []TICK

	for _, tick := range ticks {
		t, err := a.FlushTick(tick)
		if err != nil {
			return nil, err
		}

		result = append(result, t)
	}

	return result, nil
}

// Spill writes accumulated state to disk to free memory.
func (a *GenericAggregator[S, T]) Spill() (int64, error) {
	if len(a.ByTick) == 0 {
		return 0, nil
	}

	size := a.EstimatedStateSize()

	for tick, state := range a.ByTick {
		a.SpillStore.Put(formatTickKey(tick), state)
	}

	errSpill := a.SpillStore.Spill()
	if errSpill != nil {
		return 0, errSpill
	}

	a.ByTick = make(map[int]S)

	return size, nil
}

// Collect reloads previously spilled state back into memory.
func (a *GenericAggregator[S, T]) Collect() error {
	if a.SpillStore.SpillCount() == 0 {
		return nil
	}

	collected, err := a.SpillStore.CollectWith(a.MergeStateFn)
	if err != nil {
		return err
	}

	for k, v := range collected {
		tick, errParse := parseTickKey(k)
		if errParse != nil {
			continue // skip invalid keys silently or log them.
		}

		if existing, ok := a.ByTick[tick]; ok {
			a.ByTick[tick] = a.MergeStateFn(existing, v)
		} else {
			a.ByTick[tick] = v
		}
	}

	return nil
}

// EstimatedStateSize returns the current in-memory footprint of the accumulated state.
func (a *GenericAggregator[S, T]) EstimatedStateSize() int64 {
	var total int64
	for _, v := range a.ByTick {
		total += a.SizeStateFn(v)
	}

	return total
}

// SpillState returns the current on-disk spill state for checkpoint persistence.
func (a *GenericAggregator[S, T]) SpillState() AggregatorSpillInfo {
	return AggregatorSpillInfo{
		Dir:   a.SpillStore.SpillDir(),
		Count: a.SpillStore.SpillCount(),
	}
}

// RestoreSpillState points the aggregator at a previously-saved spill directory.
func (a *GenericAggregator[S, T]) RestoreSpillState(info AggregatorSpillInfo) {
	if info.Count > 0 && info.Dir != "" {
		a.SpillStore.RestoreFromDir(info.Dir, info.Count)
	}
}

// DiscardState clears all in-memory cumulative state without serialization.
func (a *GenericAggregator[S, T]) DiscardState() {
	a.ByTick = make(map[int]S)
}

// Close releases all resources. Idempotent.
func (a *GenericAggregator[S, T]) Close() error {
	if a.SpillStore != nil {
		a.SpillStore.Cleanup()
	}

	a.ByTick = nil

	return nil
}

// DrainCommitStats implements CommitStatsDrainer when DrainCommitDataFn is set.
// Iterates all tick accumulators, extracts per-commit data, and merges the results.
func (a *GenericAggregator[S, T]) DrainCommitStats() (stats map[string]any, tickHashes map[int][]gitlib.Hash) {
	if a.DrainCommitDataFn == nil {
		return nil, nil
	}

	var allCommitData map[string]any

	var allCommitsByTick map[int][]gitlib.Hash

	for _, state := range a.ByTick {
		cd, cbt := a.DrainCommitDataFn(state)

		if len(cd) > 0 {
			if allCommitData == nil {
				allCommitData = make(map[string]any, len(cd))
			}

			maps.Copy(allCommitData, cd)
		}

		for tick, hashes := range cbt {
			if allCommitsByTick == nil {
				allCommitsByTick = make(map[int][]gitlib.Hash, len(cbt))
			}

			allCommitsByTick[tick] = append(allCommitsByTick[tick], hashes...)
		}
	}

	return allCommitData, allCommitsByTick
}

// Internal helpers for keys.
func formatTickKey(tick int) string {
	return strconv.Itoa(tick)
}

func parseTickKey(key string) (int, error) {
	val, err := strconv.Atoi(key)
	if err != nil {
		return 0, fmt.Errorf("parse tick key: %w", err)
	}

	return val, nil
}
