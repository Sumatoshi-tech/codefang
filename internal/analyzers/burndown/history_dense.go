package burndown

import "sort"

// groupSparseHistory converts a sparse tick-based history into a dense 2D matrix
// suitable for rendering burndown charts.
func (b *HistoryAnalyzer) groupSparseHistory(
	history sparseHistory, lastTick int,
) DenseHistory {
	if len(history) == 0 {
		return DenseHistory{}
	}

	ticks, lastTick := b.normalizeTicks(history, lastTick)

	samples := lastTick/b.Sampling + 1
	bands := lastTick/b.Granularity + 1

	result := make(DenseHistory, samples)
	for i := range samples {
		result[i] = make([]int64, bands)
	}

	b.fillDenseHistory(result, ticks, history)

	return result
}

// normalizeTicks extracts sorted tick keys from a sparse history and resolves lastTick.
func (b *HistoryAnalyzer) normalizeTicks(history sparseHistory, lastTick int) (ticks []int, resolvedLastTick int) {
	ticks = make([]int, 0, len(history))
	for tick := range history {
		ticks = append(ticks, tick)
	}

	sort.Ints(ticks)

	if len(ticks) == 0 {
		return ticks, max(lastTick, 0)
	}

	if lastTick >= 0 {
		if ticks[len(ticks)-1] < lastTick {
			ticks = append(ticks, lastTick)
		}
	} else {
		lastTick = ticks[len(ticks)-1]
	}

	return ticks, lastTick
}

// fillDenseHistory populates a pre-allocated dense history from sparse tick data.
func (b *HistoryAnalyzer) fillDenseHistory(result DenseHistory, ticks []int, history sparseHistory) {
	prevsi := 0

	for _, tick := range ticks {
		si := tick / b.Sampling
		if si > prevsi {
			state := result[prevsi]
			for i := prevsi + 1; i <= si; i++ {
				copy(result[i], state)
			}

			prevsi = si
		}

		sample := result[si]
		for t, value := range history[tick] {
			if t/b.Granularity < len(sample) {
				sample[t/b.Granularity] += value
			}
		}
	}
}
