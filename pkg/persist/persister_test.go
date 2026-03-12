package persist

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// persisterState is a struct for persister round-trip testing.
type persisterState struct {
	Label string `json:"label"`
	Value int    `json:"value"`
}

func TestPersister_SaveLoad_JSON(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	p := NewPersister[persisterState]("mystate", NewJSONCodec())

	original := persisterState{Label: "hello", Value: 42}

	err := p.Save(dir, func() *persisterState { return &original })

	require.NoError(t, err)

	var restored persisterState

	err = p.Load(dir, func(s *persisterState) { restored = *s })

	require.NoError(t, err)

	assert.Equal(t, original.Label, restored.Label)
	assert.Equal(t, original.Value, restored.Value)
}

func TestPersister_SaveLoad_Gob(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	p := NewPersister[persisterState]("gobstate", NewGobCodec())

	original := persisterState{Label: "gob", Value: 99}

	err := p.Save(dir, func() *persisterState { return &original })

	require.NoError(t, err)

	var restored persisterState

	err = p.Load(dir, func(s *persisterState) { restored = *s })

	require.NoError(t, err)

	assert.Equal(t, original.Label, restored.Label)
	assert.Equal(t, original.Value, restored.Value)
}

func TestPersister_LoadMissingFile(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	p := NewPersister[persisterState]("missing", NewJSONCodec())

	err := p.Load(dir, func(_ *persisterState) {})

	assert.Error(t, err)
}

func TestPersister_SaveInvalidDir(t *testing.T) {
	t.Parallel()

	p := NewPersister[persisterState]("state", NewJSONCodec())

	err := p.Save("/nonexistent/path", func() *persisterState {
		return &persisterState{Label: "x"}
	})

	assert.Error(t, err)
}
