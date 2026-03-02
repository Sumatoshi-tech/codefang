package common_test

// FRD: specs/frds/FRD-20260302-context-stack.md.

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common"
)

func TestContextStack_NewStack_IsEmpty(t *testing.T) {
	t.Parallel()

	s := common.NewContextStack[int]()

	assert.Equal(t, 0, s.Depth())
}

func TestContextStack_Pop_Empty_ReturnsFalse(t *testing.T) {
	t.Parallel()

	s := common.NewContextStack[string]()

	val, ok := s.Pop()
	assert.False(t, ok)
	assert.Empty(t, val)
}

func TestContextStack_Current_Empty_ReturnsFalse(t *testing.T) {
	t.Parallel()

	s := common.NewContextStack[int]()

	val, ok := s.Current()
	assert.False(t, ok)
	assert.Zero(t, val)
}

func TestContextStack_Push_IncreasesDepth(t *testing.T) {
	t.Parallel()

	s := common.NewContextStack[int]()

	s.Push(42)

	assert.Equal(t, 1, s.Depth())
}

func TestContextStack_Push_Current_ReturnsPushed(t *testing.T) {
	t.Parallel()

	s := common.NewContextStack[string]()

	s.Push("first")

	val, ok := s.Current()
	require.True(t, ok)
	assert.Equal(t, "first", val)
}

func TestContextStack_Current_DoesNotRemove(t *testing.T) {
	t.Parallel()

	s := common.NewContextStack[int]()

	s.Push(10)

	_, _ = s.Current()
	assert.Equal(t, 1, s.Depth())
}

func TestContextStack_Pop_ReturnsTop(t *testing.T) {
	t.Parallel()

	s := common.NewContextStack[int]()

	s.Push(1)
	s.Push(2)

	val, ok := s.Pop()
	require.True(t, ok)
	assert.Equal(t, 2, val)
	assert.Equal(t, 1, s.Depth())
}

func TestContextStack_LIFO_Ordering(t *testing.T) {
	t.Parallel()

	s := common.NewContextStack[int]()

	s.Push(1)
	s.Push(2)
	s.Push(3)

	val3, ok3 := s.Pop()
	val2, ok2 := s.Pop()
	val1, ok1 := s.Pop()

	require.True(t, ok3)
	require.True(t, ok2)
	require.True(t, ok1)
	assert.Equal(t, 3, val3)
	assert.Equal(t, 2, val2)
	assert.Equal(t, 1, val1)
	assert.Equal(t, 0, s.Depth())
}

func TestContextStack_PopAll_ThenPopReturnsFalse(t *testing.T) {
	t.Parallel()

	s := common.NewContextStack[int]()

	s.Push(99)

	_, _ = s.Pop()

	val, ok := s.Pop()
	assert.False(t, ok)
	assert.Zero(t, val)
}

func TestContextStack_PointerElements(t *testing.T) {
	t.Parallel()

	type ctx struct {
		name string
	}

	s := common.NewContextStack[*ctx]()

	s.Push(&ctx{name: "alpha"})
	s.Push(&ctx{name: "beta"})

	val, ok := s.Current()
	require.True(t, ok)
	assert.Equal(t, "beta", val.name)
}
