package toposort_test

import (
	"testing"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/pkg/toposort"
)

func TestSymbolTable_Intern(t *testing.T) {
	t.Parallel()

	st := toposort.NewSymbolTable()

	id1 := st.Intern("foo")
	id2 := st.Intern("bar")
	id3 := st.Intern("foo")

	assert.NotEqual(t, id1, id2)
	assert.Equal(t, id1, id3)
	assert.Equal(t, 2, st.Len())
}

func TestSymbolTable_Resolve(t *testing.T) {
	t.Parallel()

	st := toposort.NewSymbolTable()

	id := st.Intern("hello")
	val := st.Resolve(id)

	assert.Equal(t, "hello", val)
	assert.Empty(t, st.Resolve(999))
}

func TestSymbolTable_Concurrent(t *testing.T) {
	t.Parallel()

	st := toposort.NewSymbolTable()

	// Just a simple concurrency smoke test.
	done := make(chan bool)

	for range 10 {
		go func() {
			st.Intern("concurrent")

			done <- true
		}()
	}

	for range 10 {
		<-done
	}

	assert.Equal(t, 1, st.Len())
	assert.Equal(t, "concurrent", st.Resolve(0))
}
