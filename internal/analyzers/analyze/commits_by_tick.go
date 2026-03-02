package analyze

import (
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// BuildCommitsByTick converts ticks into a map from tick index to commit
// hashes. The extract callback should type-assert tick.Data and return the
// commit-keyed map from the analyzer's TickData type, or nil/false if the
// data is not the expected type or has no commits.
func BuildCommitsByTick[V any](ticks []TICK, extract func(any) (map[string]V, bool)) map[int][]gitlib.Hash {
	ct := make(map[int][]gitlib.Hash)

	for _, tick := range ticks {
		m, ok := extract(tick.Data)
		if !ok || len(m) == 0 {
			continue
		}

		hashes := make([]gitlib.Hash, 0, len(m))

		for h := range m {
			hashes = append(hashes, gitlib.NewHash(h))
		}

		ct[tick.Tick] = append(ct[tick.Tick], hashes...)
	}

	return ct
}
