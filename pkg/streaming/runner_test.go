package streaming

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// mockHibernatable tracks hibernation calls for testing.
type mockHibernatable struct {
	hibernateCount int
	bootCount      int
}

func (m *mockHibernatable) Hibernate() error {
	m.hibernateCount++

	return nil
}

func (m *mockHibernatable) Boot() error {
	m.bootCount++

	return nil
}

func TestHibernateAnalyzers_SingleChunk_NoHibernation(t *testing.T) {
	t.Parallel()

	mock := &mockHibernatable{}
	analyzers := []Hibernatable{mock}

	chunks := []ChunkBounds{{Start: 0, End: 10}}

	// Process single chunk - no hibernation between chunks.
	for i := range chunks {
		if i > 0 {
			err := hibernateAll(analyzers)
			require.NoError(t, err)
			err = bootAll(analyzers)
			require.NoError(t, err)
		}
	}

	assert.Equal(t, 0, mock.hibernateCount)
	assert.Equal(t, 0, mock.bootCount)
}

func TestHibernateAnalyzers_MultipleChunks_Hibernates(t *testing.T) {
	t.Parallel()

	mock := &mockHibernatable{}
	analyzers := []Hibernatable{mock}

	chunks := []ChunkBounds{
		{Start: 0, End: 10},
		{Start: 10, End: 20},
		{Start: 20, End: 30},
	}

	// Process multiple chunks - hibernate between each.
	for i := range chunks {
		if i > 0 {
			err := hibernateAll(analyzers)
			require.NoError(t, err)
			err = bootAll(analyzers)
			require.NoError(t, err)
		}
	}

	// 3 chunks means 2 transitions.
	assert.Equal(t, 2, mock.hibernateCount)
	assert.Equal(t, 2, mock.bootCount)
}

func TestCollectHibernatables_MixedAnalyzers(t *testing.T) {
	t.Parallel()

	// Test that only hibernatable analyzers are collected.
	h1 := &mockHibernatable{}
	h2 := &mockHibernatable{}

	hibernatables := []Hibernatable{h1, h2}

	err := hibernateAll(hibernatables)
	require.NoError(t, err)

	assert.Equal(t, 1, h1.hibernateCount)
	assert.Equal(t, 1, h2.hibernateCount)
}
