package checkpoint

import (
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
	return os.WriteFile(filepath.Join(dir, "mock.bin"), []byte(m.data), 0o600)
}

func (m *mockCheckpointable) LoadCheckpoint(dir string) error {
	data, err := os.ReadFile(filepath.Join(dir, "mock.bin"))
	if err != nil {
		return err
	}
	m.data = string(data)
	return nil
}

func (m *mockCheckpointable) CheckpointSize() int64 {
	return int64(len(m.data))
}

func TestCheckpointable_Interface(_ *testing.T) {
	// Verify mockCheckpointable implements Checkpointable.
	var _ Checkpointable = (*mockCheckpointable)(nil)
}

func TestCheckpointable_SaveLoad(t *testing.T) {
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
	m := &mockCheckpointable{data: "12345"}
	assert.Equal(t, int64(5), m.CheckpointSize())
}
