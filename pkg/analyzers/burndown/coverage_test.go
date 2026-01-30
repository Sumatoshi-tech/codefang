package burndown //nolint:testpackage // testing internal implementation.

import (
	"testing"
)

func TestDenseHistory(t *testing.T) {
	t.Parallel()
	// DenseHistory type test.
	d := make(DenseHistory, 2)
	d[0] = []int64{1, 2, 3}
	d[1] = []int64{4, 5, 6}

	if len(d) != 2 {
		t.Error("expected 2 rows")
	}
}

func TestShard(t *testing.T) {
	t.Parallel()
	// Shard type basic test - verify zero value is usable.
	s := &Shard{}

	// Verify the shard is initialized with zero values.
	if s.files != nil {
		t.Error("expected nil files in zero value Shard")
	}
}
