package storage

// FRD: specs/frds/FRD-20260310-atomic-file-write.md.

import (
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

const (
	testPerm      = 0o600
	testContent   = "hello atomic"
	testTmpSuffix = ".tmp"
)

// errWriteCallback is a sentinel used to simulate a write callback failure.
var errWriteCallback = errors.New("write callback failed")

func writeString(w io.Writer, s string) error {
	_, err := io.WriteString(w, s)
	if err != nil {
		return fmt.Errorf("test write: %w", err)
	}

	return nil
}

func TestWriteAtomic_SuccessPath(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	target := filepath.Join(dir, "output.dat")

	err := WriteAtomic(target, testPerm, func(w io.Writer) error {
		return writeString(w, testContent)
	})

	require.NoError(t, err)

	got, readErr := os.ReadFile(target)
	require.NoError(t, readErr)
	assert.Equal(t, testContent, string(got))

	// Verify no tmp file remains.
	tmpPath := target + testTmpSuffix
	_, statErr := os.Stat(tmpPath)
	assert.True(t, os.IsNotExist(statErr), "tmp file should not exist after success")
}

func TestWriteAtomic_OverwritesExistingFile(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	target := filepath.Join(dir, "output.dat")

	// Write initial content.
	require.NoError(t, os.WriteFile(target, []byte("old"), testPerm))

	err := WriteAtomic(target, testPerm, func(w io.Writer) error {
		return writeString(w, "new")
	})

	require.NoError(t, err)

	got, readErr := os.ReadFile(target)
	require.NoError(t, readErr)
	assert.Equal(t, "new", string(got))
}

func TestWriteAtomic_WriteCallbackError_CleansUp(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	target := filepath.Join(dir, "output.dat")

	err := WriteAtomic(target, testPerm, func(_ io.Writer) error {
		return errWriteCallback
	})

	require.ErrorIs(t, err, errWriteCallback)

	// Target should not exist.
	_, statErr := os.Stat(target)
	assert.True(t, os.IsNotExist(statErr), "target file should not exist after write error")

	// Tmp file should be cleaned up.
	tmpPath := target + testTmpSuffix
	_, tmpStatErr := os.Stat(tmpPath)
	assert.True(t, os.IsNotExist(tmpStatErr), "tmp file should be cleaned up after write error")
}

func TestWriteAtomic_CreateError_InvalidDir(t *testing.T) {
	t.Parallel()

	target := filepath.Join(t.TempDir(), "nonexistent", "subdir", "file.dat")

	err := WriteAtomic(target, testPerm, func(w io.Writer) error {
		return writeString(w, testContent)
	})

	require.Error(t, err)
	assert.Contains(t, err.Error(), "atomic create")
}

func TestWriteAtomic_EmptyWrite(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	target := filepath.Join(dir, "empty.dat")

	err := WriteAtomic(target, testPerm, func(_ io.Writer) error {
		return nil
	})

	require.NoError(t, err)

	got, readErr := os.ReadFile(target)
	require.NoError(t, readErr)
	assert.Empty(t, got)
}
