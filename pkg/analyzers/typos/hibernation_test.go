package typos

import (
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/levenshtein"
)

func TestHibernate_ClearsContext(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{
		lcontext: &levenshtein.Context{},
		typos:    []Typo{{Wrong: "test", Correct: "tset"}},
	}

	err := h.Hibernate()
	require.NoError(t, err)

	// lcontext should be cleared.
	require.Nil(t, h.lcontext)

	// typos should be preserved.
	require.Len(t, h.typos, 1)
}

func TestBoot_RecreatesContext(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{
		lcontext: nil,
		typos:    []Typo{{Wrong: "test", Correct: "tset"}},
	}

	err := h.Boot()
	require.NoError(t, err)

	// lcontext should be recreated.
	require.NotNil(t, h.lcontext)

	// typos should be preserved.
	require.Len(t, h.typos, 1)
}

func TestHibernateBootCycle_PreservesTypos(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{
		lcontext: &levenshtein.Context{},
		typos: []Typo{
			{Wrong: "tset", Correct: "test", File: "main.go", Line: 10},
			{Wrong: "functon", Correct: "function", File: "util.go", Line: 20},
		},
	}

	// Hibernate.
	err := h.Hibernate()
	require.NoError(t, err)
	require.Nil(t, h.lcontext)

	// Boot.
	err = h.Boot()
	require.NoError(t, err)
	require.NotNil(t, h.lcontext)

	// Typos should be preserved.
	require.Len(t, h.typos, 2)
	require.Equal(t, "tset", h.typos[0].Wrong)
	require.Equal(t, "functon", h.typos[1].Wrong)
}

func TestBoot_IdempotentIfAlreadyBooted(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{
		lcontext: &levenshtein.Context{},
	}

	originalContext := h.lcontext

	// Boot should not replace existing context.
	err := h.Boot()
	require.NoError(t, err)

	// Should still be the same context.
	require.Same(t, originalContext, h.lcontext)
}

func TestHibernate_IdempotentIfAlreadyHibernated(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{
		lcontext: nil,
		typos:    []Typo{{Wrong: "test", Correct: "tset"}},
	}

	// Hibernate when already hibernated.
	err := h.Hibernate()
	require.NoError(t, err)

	// Should not panic, typos preserved.
	require.Len(t, h.typos, 1)
}

func TestLcontextFunctionality_AfterBoot(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}

	// Boot to get lcontext.
	err := h.Boot()
	require.NoError(t, err)

	// Verify lcontext is functional.
	dist := h.lcontext.Distance("test", "tset")
	require.Equal(t, 2, dist)
}
