package analyze

// FRD: specs/frds/FRD-20260302-record-reader.md.

import (
	"encoding/gob"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func setupReaderWithRecords(t *testing.T, kind string, records []testRecord) ReportReader {
	t.Helper()

	gob.Register(testRecord{})

	dir := t.TempDir()
	store := NewFileReportStore(dir)

	meta := ReportMeta{AnalyzerID: "test-analyzer", Version: "v1"}

	w, err := store.Begin("test-analyzer", meta)
	require.NoError(t, err)

	for _, rec := range records {
		require.NoError(t, w.Write(kind, rec))
	}

	require.NoError(t, w.Close())

	r, err := store.Open("test-analyzer")
	require.NoError(t, err)

	t.Cleanup(func() { _ = r.Close() })

	return r
}

func TestReadRecordsIfPresent_KindAbsent(t *testing.T) {
	t.Parallel()

	r := setupReaderWithRecords(t, "other_kind", []testRecord{{Name: "a", Value: 1}})
	kinds := r.Kinds()

	result, err := ReadRecordsIfPresent[testRecord](r, kinds, "missing_kind")

	require.NoError(t, err)
	assert.Nil(t, result)
}

func TestReadRecordsIfPresent_EmptyRecords(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	store := NewFileReportStore(dir)

	meta := ReportMeta{AnalyzerID: "test-analyzer", Version: "v1"}

	w, err := store.Begin("test-analyzer", meta)
	require.NoError(t, err)
	require.NoError(t, w.Close())

	r, err := store.Open("test-analyzer")
	require.NoError(t, err)

	t.Cleanup(func() { _ = r.Close() })

	kinds := r.Kinds()

	result, err := ReadRecordsIfPresent[testRecord](r, kinds, "summary")

	require.NoError(t, err)
	assert.Nil(t, result)
}

func TestReadRecordsIfPresent_SingleRecord(t *testing.T) {
	t.Parallel()

	r := setupReaderWithRecords(t, "summary", []testRecord{{Name: "alpha", Value: 42}})
	kinds := r.Kinds()

	result, err := ReadRecordsIfPresent[testRecord](r, kinds, "summary")

	require.NoError(t, err)
	require.Len(t, result, 1)
	assert.Equal(t, "alpha", result[0].Name)
	assert.Equal(t, 42, result[0].Value)
}

func TestReadRecordsIfPresent_MultipleRecords(t *testing.T) {
	t.Parallel()

	records := []testRecord{
		{Name: "first", Value: 1},
		{Name: "second", Value: 2},
		{Name: "third", Value: 3},
	}

	r := setupReaderWithRecords(t, "entries", records)
	kinds := r.Kinds()

	result, err := ReadRecordsIfPresent[testRecord](r, kinds, "entries")

	require.NoError(t, err)
	require.Len(t, result, 3)
	assert.Equal(t, "first", result[0].Name)
	assert.Equal(t, "second", result[1].Name)
	assert.Equal(t, "third", result[2].Name)
}

func TestReadRecordsIfPresent_DecodeError(t *testing.T) {
	t.Parallel()

	gob.Register(testRecord{})

	dir := t.TempDir()
	store := NewFileReportStore(dir)

	meta := ReportMeta{AnalyzerID: "test-analyzer", Version: "v1"}

	w, err := store.Begin("test-analyzer", meta)
	require.NoError(t, err)

	// Write a testRecord but we'll try to decode as a different type.
	require.NoError(t, w.Write("summary", testRecord{Name: "alpha", Value: 1}))
	require.NoError(t, w.Close())

	r, err := store.Open("test-analyzer")
	require.NoError(t, err)

	t.Cleanup(func() { _ = r.Close() })

	kinds := r.Kinds()

	type incompatibleType struct {
		X float64
		Y float64
	}

	gob.Register(incompatibleType{})

	_, decErr := ReadRecordsIfPresent[incompatibleType](r, kinds, "summary")

	assert.Error(t, decErr)
}

func TestReadRecordIfPresent_KindAbsent(t *testing.T) {
	t.Parallel()

	r := setupReaderWithRecords(t, "other_kind", []testRecord{{Name: "a", Value: 1}})
	kinds := r.Kinds()

	result, err := ReadRecordIfPresent[testRecord](r, kinds, "missing_kind")

	require.NoError(t, err)
	assert.Equal(t, testRecord{}, result)
}

func TestReadRecordIfPresent_SingleRecord(t *testing.T) {
	t.Parallel()

	r := setupReaderWithRecords(t, "aggregate", []testRecord{{Name: "total", Value: 99}})
	kinds := r.Kinds()

	result, err := ReadRecordIfPresent[testRecord](r, kinds, "aggregate")

	require.NoError(t, err)
	assert.Equal(t, "total", result.Name)
	assert.Equal(t, 99, result.Value)
}

func TestReadRecordIfPresent_LastRecordWins(t *testing.T) {
	t.Parallel()

	records := []testRecord{
		{Name: "first", Value: 1},
		{Name: "last", Value: 2},
	}

	r := setupReaderWithRecords(t, "aggregate", records)
	kinds := r.Kinds()

	result, err := ReadRecordIfPresent[testRecord](r, kinds, "aggregate")

	require.NoError(t, err)
	assert.Equal(t, "last", result.Name)
	assert.Equal(t, 2, result.Value)
}
