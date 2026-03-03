package analyze

// FRD: specs/frds/FRD-20260303-write-slice-kind.md.

import (
	"encoding/gob"
	"errors"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestWriteSliceKind_NilSlice(t *testing.T) {
	t.Parallel()

	var records []testRecord

	err := WriteSliceKind[testRecord](nil, "summary", records)

	assert.NoError(t, err)
}

func TestWriteSliceKind_EmptySlice(t *testing.T) {
	t.Parallel()

	records := []testRecord{}

	err := WriteSliceKind[testRecord](nil, "summary", records)

	assert.NoError(t, err)
}

func TestWriteSliceKind_SingleRecord(t *testing.T) {
	t.Parallel()

	gob.Register(testRecord{})

	dir := t.TempDir()
	store := NewFileReportStore(dir)
	meta := ReportMeta{AnalyzerID: "test", Version: "v1"}

	w, err := store.Begin("test", meta)
	require.NoError(t, err)

	records := []testRecord{{Name: "alpha", Value: 42}}

	writeErr := WriteSliceKind(w, "items", records)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())

	r, err := store.Open("test")
	require.NoError(t, err)

	t.Cleanup(func() { _ = r.Close() })

	result, readErr := ReadRecordsIfPresent[testRecord](r, r.Kinds(), "items")
	require.NoError(t, readErr)
	require.Len(t, result, 1)
	assert.Equal(t, "alpha", result[0].Name)
	assert.Equal(t, 42, result[0].Value)
}

func TestWriteSliceKind_MultipleRecords(t *testing.T) {
	t.Parallel()

	gob.Register(testRecord{})

	dir := t.TempDir()
	store := NewFileReportStore(dir)
	meta := ReportMeta{AnalyzerID: "test", Version: "v1"}

	w, err := store.Begin("test", meta)
	require.NoError(t, err)

	records := []testRecord{
		{Name: "first", Value: 1},
		{Name: "second", Value: 2},
		{Name: "third", Value: 3},
	}

	writeErr := WriteSliceKind(w, "entries", records)
	require.NoError(t, writeErr)
	require.NoError(t, w.Close())

	r, err := store.Open("test")
	require.NoError(t, err)

	t.Cleanup(func() { _ = r.Close() })

	result, readErr := ReadRecordsIfPresent[testRecord](r, r.Kinds(), "entries")
	require.NoError(t, readErr)
	require.Len(t, result, 3)
	assert.Equal(t, "first", result[0].Name)
	assert.Equal(t, "second", result[1].Name)
	assert.Equal(t, "third", result[2].Name)
}

// errWriter is a ReportWriter that returns an error on the Nth Write call.
type errWriter struct {
	calls   int
	failOnN int
}

var errForcedWrite = errors.New("forced write error")

func (ew *errWriter) Write(_ string, _ any) error {
	ew.calls++
	if ew.calls >= ew.failOnN {
		return errForcedWrite
	}

	return nil
}

func (ew *errWriter) Close() error { return nil }

func TestWriteSliceKind_WriteError(t *testing.T) {
	t.Parallel()

	records := []testRecord{
		{Name: "ok", Value: 1},
		{Name: "fail", Value: 2},
	}

	w := &errWriter{failOnN: 2}

	err := WriteSliceKind(w, "items", records)

	require.Error(t, err)
	require.ErrorIs(t, err, errForcedWrite)
	assert.Contains(t, err.Error(), "write items")
}
