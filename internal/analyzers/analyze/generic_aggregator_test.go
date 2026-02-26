package analyze_test

import (
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

type DummyState struct {
	Count int
}

type DummyTickData struct {
	Total int
}

func extractTC(tc analyze.TC, byTick map[int]*DummyState) error {
	state, ok := byTick[tc.Tick]
	if !ok {
		state = &DummyState{}
		byTick[tc.Tick] = state
	}

	val, ok := tc.Data.(int)
	if ok {
		state.Count += val
	}

	return nil
}

func mergeState(existing, incoming *DummyState) *DummyState {
	if existing == nil {
		return incoming
	}

	if incoming == nil {
		return existing
	}

	existing.Count += incoming.Count

	return existing
}

func sizeState(state *DummyState) int64 {
	if state == nil {
		return 0
	}

	return 8 // arbitrary size.
}

func buildTick(tick int, state *DummyState) (analyze.TICK, error) {
	if state == nil {
		return analyze.TICK{Tick: tick}, nil
	}

	return analyze.TICK{
		Tick: tick,
		Data: &DummyTickData{Total: state.Count},
	}, nil
}

func setupAggregator(budget int64) *analyze.GenericAggregator[*DummyState, *DummyTickData] {
	return analyze.NewGenericAggregator[*DummyState, *DummyTickData](
		analyze.AggregatorOptions{
			SpillBudget: budget,
			SpillDir:    "", // use default temp.
		},
		extractTC,
		mergeState,
		sizeState,
		buildTick,
	)
}

func TestGenericAggregator_AddAndFlush(t *testing.T) {
	t.Parallel()

	agg := setupAggregator(0) // no limit.

	defer func() { require.NoError(t, agg.Close()) }()

	err := agg.Add(analyze.TC{Tick: 1, Data: 10})
	require.NoError(t, err)

	err = agg.Add(analyze.TC{Tick: 1, Data: 20})
	require.NoError(t, err)

	tick, err := agg.FlushTick(1)
	require.NoError(t, err)
	require.Equal(t, 1, tick.Tick)

	data, ok := tick.Data.(*DummyTickData)
	require.True(t, ok)
	require.Equal(t, 30, data.Total)

	// Flush non-existent tick.
	emptyTick, err := agg.FlushTick(99)
	require.NoError(t, err)
	require.Equal(t, 99, emptyTick.Tick)
	require.Nil(t, emptyTick.Data)
}

func TestGenericAggregator_FlushAllTicks(t *testing.T) {
	t.Parallel()

	agg := setupAggregator(0)

	defer func() { require.NoError(t, agg.Close()) }()

	require.NoError(t, agg.Add(analyze.TC{Tick: 5, Data: 50}))
	require.NoError(t, agg.Add(analyze.TC{Tick: 2, Data: 20}))
	require.NoError(t, agg.Add(analyze.TC{Tick: 8, Data: 80}))

	ticks, err := agg.FlushAllTicks()
	require.NoError(t, err)
	require.Len(t, ticks, 3)

	// Should be sorted ascending by tick.
	require.Equal(t, 2, ticks[0].Tick)
	require.Equal(t, 5, ticks[1].Tick)
	require.Equal(t, 8, ticks[2].Tick)

	data0, ok0 := ticks[0].Data.(*DummyTickData)
	require.True(t, ok0)
	require.Equal(t, 20, data0.Total)

	data1, ok1 := ticks[1].Data.(*DummyTickData)
	require.True(t, ok1)
	require.Equal(t, 50, data1.Total)

	data2, ok2 := ticks[2].Data.(*DummyTickData)
	require.True(t, ok2)
	require.Equal(t, 80, data2.Total)
}

func TestGenericAggregator_SpillAndCollect(t *testing.T) {
	t.Parallel()

	// Budget of 10 bytes. Each state is 8 bytes.
	// 2 distinct ticks will exceed budget (16 > 10) and trigger spill.
	agg := setupAggregator(10)

	defer func() { require.NoError(t, agg.Close()) }()

	// Add to tick 1 (8 bytes).
	require.NoError(t, agg.Add(analyze.TC{Tick: 1, Data: 5}))
	require.Equal(t, int64(8), agg.EstimatedStateSize())
	require.Equal(t, 0, agg.SpillState().Count) // no spills yet.

	// Add to tick 2 (8 bytes). Total = 16 bytes. Exceeds budget, triggers spill.
	require.NoError(t, agg.Add(analyze.TC{Tick: 2, Data: 10}))

	require.Equal(t, int64(0), agg.EstimatedStateSize())
	require.Equal(t, 1, agg.SpillState().Count)

	// Add to tick 1 again (in-memory now).
	require.NoError(t, agg.Add(analyze.TC{Tick: 1, Data: 2}))

	// Collect.
	require.NoError(t, agg.Collect())

	// Data should be merged.
	ticks, err := agg.FlushAllTicks()
	require.NoError(t, err)
	require.Len(t, ticks, 2)

	require.Equal(t, 1, ticks[0].Tick)

	data0, ok0 := ticks[0].Data.(*DummyTickData)
	require.True(t, ok0)
	require.Equal(t, 7, data0.Total) // 5 + 2.

	require.Equal(t, 2, ticks[1].Tick)

	data1, ok1 := ticks[1].Data.(*DummyTickData)
	require.True(t, ok1)
	require.Equal(t, 10, data1.Total)
}

func TestGenericAggregator_RestoreSpillState(t *testing.T) {
	t.Parallel()

	agg1 := setupAggregator(10)
	require.NoError(t, agg1.Add(analyze.TC{Tick: 1, Data: 10}))
	require.NoError(t, agg1.Add(analyze.TC{Tick: 2, Data: 20})) // triggers spill.

	info := agg1.SpillState()
	require.NotEmpty(t, info.Dir)
	require.Equal(t, 1, info.Count)

	agg2 := setupAggregator(0)

	defer func() { require.NoError(t, agg2.Close()) }()

	agg2.RestoreSpillState(info)

	require.NoError(t, agg2.Collect())

	ticks, err := agg2.FlushAllTicks()
	require.NoError(t, err)
	require.Len(t, ticks, 2)

	data0, ok0 := ticks[0].Data.(*DummyTickData)
	require.True(t, ok0)
	require.Equal(t, 10, data0.Total)

	data1, ok1 := ticks[1].Data.(*DummyTickData)
	require.True(t, ok1)
	require.Equal(t, 20, data1.Total)

	require.NoError(t, agg1.Close()) // cleans up dir.
}
