package common_test

// FRD: specs/frds/FRD-20260302-filter-by-interface.md.

import (
	"testing"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common"
)

type stringer interface {
	String() string
}

type named struct {
	name string
}

func (n *named) String() string { return n.name }

type unnamed struct {
	value int
}

func TestFilterByInterface_EmptySlice(t *testing.T) {
	t.Parallel()

	result := common.FilterByInterface([]any{}, func(a any) (stringer, bool) {
		s, ok := a.(stringer)

		return s, ok
	})

	assert.Empty(t, result)
}

func TestFilterByInterface_NilSlice(t *testing.T) {
	t.Parallel()

	result := common.FilterByInterface(nil, func(a any) (stringer, bool) {
		s, ok := a.(stringer)

		return s, ok
	})

	assert.Nil(t, result)
}

func TestFilterByInterface_NoMatches(t *testing.T) {
	t.Parallel()

	items := []any{&unnamed{value: 1}, &unnamed{value: 2}}

	result := common.FilterByInterface(items, func(a any) (stringer, bool) {
		s, ok := a.(stringer)

		return s, ok
	})

	assert.Empty(t, result)
}

func TestFilterByInterface_AllMatch(t *testing.T) {
	t.Parallel()

	items := []any{&named{name: "alpha"}, &named{name: "beta"}}

	result := common.FilterByInterface(items, func(a any) (stringer, bool) {
		s, ok := a.(stringer)

		return s, ok
	})

	assert.Len(t, result, 2)
	assert.Equal(t, "alpha", result[0].String())
	assert.Equal(t, "beta", result[1].String())
}

func TestFilterByInterface_PartialMatch(t *testing.T) {
	t.Parallel()

	items := []any{
		&named{name: "first"},
		&unnamed{value: 42},
		&named{name: "third"},
	}

	result := common.FilterByInterface(items, func(a any) (stringer, bool) {
		s, ok := a.(stringer)

		return s, ok
	})

	assert.Len(t, result, 2)
	assert.Equal(t, "first", result[0].String())
	assert.Equal(t, "third", result[1].String())
}

func TestFilterByInterface_PreservesOrder(t *testing.T) {
	t.Parallel()

	items := []any{
		&unnamed{value: 0},
		&named{name: "c"},
		&unnamed{value: 0},
		&named{name: "a"},
		&unnamed{value: 0},
		&named{name: "b"},
	}

	result := common.FilterByInterface(items, func(a any) (stringer, bool) {
		s, ok := a.(stringer)

		return s, ok
	})

	assert.Len(t, result, 3)
	assert.Equal(t, "c", result[0].String())
	assert.Equal(t, "a", result[1].String())
	assert.Equal(t, "b", result[2].String())
}

type animal interface {
	Sound() string
}

type dog struct{}

func (dog) Sound() string { return "woof" }

type cat struct{}

func (cat) Sound() string { return "meow" }

func TestFilterByInterface_ConcreteSliceType(t *testing.T) {
	t.Parallel()

	items := []animal{dog{}, cat{}, dog{}}

	result := common.FilterByInterface(items, func(a animal) (dog, bool) {
		d, ok := a.(dog)

		return d, ok
	})

	assert.Len(t, result, 2)
}

func TestFilterByInterface_SingleElement_Match(t *testing.T) {
	t.Parallel()

	items := []any{&named{name: "only"}}

	result := common.FilterByInterface(items, func(a any) (stringer, bool) {
		s, ok := a.(stringer)

		return s, ok
	})

	assert.Len(t, result, 1)
	assert.Equal(t, "only", result[0].String())
}
