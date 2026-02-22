package spillstore_test

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/spillstore"
)

func TestSpillStore_NoSpill(t *testing.T) {
	t.Parallel()

	s := spillstore.New[int]()
	s.Put("a", 1)
	s.Put("b", 2)

	assert.Equal(t, 2, s.Len())

	v, ok := s.Get("a")
	assert.True(t, ok)
	assert.Equal(t, 1, v)

	collected, err := s.Collect()
	require.NoError(t, err)
	assert.Equal(t, map[string]int{"a": 1, "b": 2}, collected)
}

func TestSpillStore_SingleSpill(t *testing.T) {
	t.Parallel()

	s := spillstore.New[string]()
	s.Put("k1", "v1")
	s.Put("k2", "v2")

	require.NoError(t, s.Spill())
	assert.Equal(t, 0, s.Len())
	assert.Equal(t, 1, s.SpillCount())

	s.Put("k3", "v3")

	collected, err := s.Collect()
	require.NoError(t, err)
	assert.Equal(t, map[string]string{"k1": "v1", "k2": "v2", "k3": "v3"}, collected)
	assert.Empty(t, s.SpillDir()) // Cleaned up.
}

func TestSpillStore_MultipleSpills(t *testing.T) {
	t.Parallel()

	s := spillstore.New[int]()

	// Chunk 1.
	s.Put("a", 1)
	s.Put("b", 2)
	require.NoError(t, s.Spill())

	// Chunk 2.
	s.Put("c", 3)
	s.Put("d", 4)
	require.NoError(t, s.Spill())

	// Chunk 3 (in-memory).
	s.Put("e", 5)

	collected, err := s.Collect()
	require.NoError(t, err)
	assert.Equal(t, map[string]int{"a": 1, "b": 2, "c": 3, "d": 4, "e": 5}, collected)
}

func TestSpillStore_SpillEmpty(t *testing.T) {
	t.Parallel()

	s := spillstore.New[int]()

	require.NoError(t, s.Spill()) // No-op.
	assert.Equal(t, 0, s.SpillCount())
	assert.Empty(t, s.SpillDir())
}

func TestSpillStore_CollectWith(t *testing.T) {
	t.Parallel()

	s := spillstore.New[map[string]int]()

	// Chunk 1: file "a.go" couples with "b.go".
	s.Put("a.go", map[string]int{"b.go": 3})
	require.NoError(t, s.Spill())

	// Chunk 2: file "a.go" couples with "b.go" again + "c.go".
	s.Put("a.go", map[string]int{"b.go": 2, "c.go": 1})

	merge := func(existing, incoming map[string]int) map[string]int {
		for k, v := range incoming {
			existing[k] += v
		}

		return existing
	}

	collected, err := s.CollectWith(merge)
	require.NoError(t, err)
	assert.Equal(t, map[string]int{"b.go": 5, "c.go": 1}, collected["a.go"])
}

func TestSpillStore_Cleanup(t *testing.T) {
	t.Parallel()

	s := spillstore.New[int]()
	s.Put("x", 42)
	require.NoError(t, s.Spill())

	dir := s.SpillDir()
	assert.DirExists(t, dir)

	s.Cleanup()
	assert.NoDirExists(t, dir)

	// Double cleanup is safe.
	s.Cleanup()
}

func TestSpillStore_RestoreFromDir(t *testing.T) {
	t.Parallel()

	// Write spill files via one store.
	s1 := spillstore.New[int]()
	s1.Put("a", 1)
	require.NoError(t, s1.Spill())
	s1.Put("b", 2)
	require.NoError(t, s1.Spill())

	dir := s1.SpillDir()
	count := s1.SpillCount()

	// Restore into a new store.
	s2 := spillstore.New[int]()
	s2.RestoreFromDir(dir, count)
	s2.Put("c", 3)

	collected, err := s2.Collect()
	require.NoError(t, err)
	assert.Equal(t, map[string]int{"a": 1, "b": 2, "c": 3}, collected)
}

// SliceSpillStore tests.

func TestSliceSpillStore_NoSpill(t *testing.T) {
	t.Parallel()

	s := spillstore.NewSlice[string]()
	s.Append("a", "b")

	assert.Equal(t, 2, s.Len())

	collected, err := s.Collect()
	require.NoError(t, err)
	assert.Equal(t, []string{"a", "b"}, collected)
}

func TestSliceSpillStore_SingleSpill(t *testing.T) {
	t.Parallel()

	s := spillstore.NewSlice[int]()
	s.Append(1, 2, 3)
	require.NoError(t, s.Spill())

	s.Append(4, 5)

	collected, err := s.Collect()
	require.NoError(t, err)
	assert.Equal(t, []int{1, 2, 3, 4, 5}, collected)
}

func TestSliceSpillStore_MultipleSpills(t *testing.T) {
	t.Parallel()

	s := spillstore.NewSlice[int]()

	s.Append(1)
	require.NoError(t, s.Spill())

	s.Append(2)
	require.NoError(t, s.Spill())

	s.Append(3)

	collected, err := s.Collect()
	require.NoError(t, err)
	assert.Equal(t, []int{1, 2, 3}, collected)
}

func TestSliceSpillStore_SpillEmpty(t *testing.T) {
	t.Parallel()

	s := spillstore.NewSlice[int]()
	require.NoError(t, s.Spill()) // No-op.
	assert.Equal(t, 0, s.SpillCount())
}

func TestSliceSpillStore_CollectEmpty(t *testing.T) {
	t.Parallel()

	s := spillstore.NewSlice[int]()

	collected, err := s.Collect()
	require.NoError(t, err)
	assert.Empty(t, collected)
}

type testStruct struct {
	Name  string
	Value int
}

func TestSpillStore_StructValues(t *testing.T) {
	t.Parallel()

	s := spillstore.New[testStruct]()
	s.Put("x", testStruct{Name: "hello", Value: 42})
	require.NoError(t, s.Spill())

	s.Put("y", testStruct{Name: "world", Value: 99})

	collected, err := s.Collect()
	require.NoError(t, err)
	assert.Equal(t, testStruct{Name: "hello", Value: 42}, collected["x"])
	assert.Equal(t, testStruct{Name: "world", Value: 99}, collected["y"])
}

func TestSpillStore_PointerValues(t *testing.T) {
	t.Parallel()

	s := spillstore.New[*testStruct]()
	s.Put("x", &testStruct{Name: "hello", Value: 42})
	require.NoError(t, s.Spill())

	s.Put("y", &testStruct{Name: "world", Value: 99})

	collected, err := s.Collect()
	require.NoError(t, err)
	assert.Equal(t, &testStruct{Name: "hello", Value: 42}, collected["x"])
	assert.Equal(t, &testStruct{Name: "world", Value: 99}, collected["y"])
}
