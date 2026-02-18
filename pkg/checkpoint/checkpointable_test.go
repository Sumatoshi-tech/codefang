package checkpoint

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// mockCheckpointable implements Checkpointable for testing.
type mockCheckpointable struct {
	data string
}

func (m *mockCheckpointable) SaveCheckpoint(dir string) error {
	err := os.WriteFile(filepath.Join(dir, "mock.bin"), []byte(m.data), 0o600)
	if err != nil {
		return fmt.Errorf("writing mock checkpoint: %w", err)
	}

	return nil
}

func (m *mockCheckpointable) LoadCheckpoint(dir string) error {
	data, err := os.ReadFile(filepath.Join(dir, "mock.bin"))
	if err != nil {
		return fmt.Errorf("reading mock checkpoint: %w", err)
	}

	m.data = string(data)

	return nil
}

func (m *mockCheckpointable) CheckpointSize() int64 {
	return int64(len(m.data))
}

func TestCheckpointable_Interface(t *testing.T) {
	t.Parallel()

	// Verify mockCheckpointable implements Checkpointable.
	var _ Checkpointable = (*mockCheckpointable)(nil)
}

func TestCheckpointable_SaveLoad(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	original := &mockCheckpointable{data: "test state data"}
	err := original.SaveCheckpoint(dir)
	require.NoError(t, err)

	restored := &mockCheckpointable{}
	err = restored.LoadCheckpoint(dir)
	require.NoError(t, err)

	assert.Equal(t, original.data, restored.data)
}

func TestCheckpointable_Size(t *testing.T) {
	t.Parallel()

	m := &mockCheckpointable{data: "12345"}
	assert.Equal(t, int64(5), m.CheckpointSize())
}
