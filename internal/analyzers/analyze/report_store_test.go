package analyze

import (
	"encoding/gob"
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// Compile-time interface assertions.
var (
	_ ReportStore  = (*FileReportStore)(nil)
	_ ReportWriter = (*fileReportWriter)(nil)
	_ ReportReader = (*fileReportReader)(nil)
)

// testRecord is a simple struct used in round-trip tests.
type testRecord struct {
	Name  string
	Value int
}

func TestReportMeta_Fields(t *testing.T) {
	t.Parallel()

	gob.Register(testRecord{})

	meta := ReportMeta{
		AnalyzerID: "history/burndown",
		Version:    "v1.0.0",
		SchemaHash: "abc123",
	}

	assert.Equal(t, "history/burndown", meta.AnalyzerID)
	assert.Equal(t, "v1.0.0", meta.Version)
	assert.Equal(t, "abc123", meta.SchemaHash)
}

func TestFileReportStore_RoundTrip_SingleKind(t *testing.T) {
	t.Parallel()

	gob.Register(testRecord{})

	dir := t.TempDir()
	store := NewFileReportStore(dir)

	meta := ReportMeta{
		AnalyzerID: "history/burndown",
		Version:    "v1.0.0",
	}

	w, err := store.Begin("history/burndown", meta)
	require.NoError(t, err)
	require.NoError(t, w.Write("summary", testRecord{Name: "alpha", Value: 1}))
	require.NoError(t, w.Write("summary", testRecord{Name: "beta", Value: 2}))
	require.NoError(t, w.Close())

	r, err := store.Open("history/burndown")
	require.NoError(t, err)

	assert.Equal(t, meta, r.Meta())

	var got []testRecord

	err = r.Iter("summary", func(raw []byte) error {
		var rec testRecord

		decErr := GobDecode(raw, &rec)
		if decErr != nil {
			return decErr
		}

		got = append(got, rec)

		return nil
	})
	require.NoError(t, err)
	require.NoError(t, r.Close())

	require.Len(t, got, 2)
	assert.Equal(t, testRecord{Name: "alpha", Value: 1}, got[0])
	assert.Equal(t, testRecord{Name: "beta", Value: 2}, got[1])

	ids := store.AnalyzerIDs()
	assert.Equal(t, []string{"history/burndown"}, ids)
	require.NoError(t, store.Close())
}

func TestFileReportStore_RoundTrip_MultipleKinds(t *testing.T) {
	t.Parallel()

	gob.Register(testRecord{})

	dir := t.TempDir()
	store := NewFileReportStore(dir)

	const recordsPerKind = 100

	meta := ReportMeta{AnalyzerID: "history/couples", Version: "v2"}

	w, err := store.Begin("history/couples", meta)
	require.NoError(t, err)

	kinds := []string{"edges", "summary", "top_edges"}

	for _, kind := range kinds {
		for i := range recordsPerKind {
			require.NoError(t, w.Write(kind, testRecord{Name: kind, Value: i}))
		}
	}

	require.NoError(t, w.Close())

	r, err := store.Open("history/couples")
	require.NoError(t, err)

	assert.Equal(t, kinds, r.Kinds())

	for _, kind := range kinds {
		var got []testRecord

		err = r.Iter(kind, func(raw []byte) error {
			var rec testRecord

			decErr := GobDecode(raw, &rec)
			if decErr != nil {
				return decErr
			}

			got = append(got, rec)

			return nil
		})
		require.NoError(t, err)
		require.Len(t, got, recordsPerKind)

		for i, rec := range got {
			assert.Equal(t, kind, rec.Name)
			assert.Equal(t, i, rec.Value)
		}
	}

	require.NoError(t, r.Close())
}

func TestFileReportStore_MultipleAnalyzers(t *testing.T) {
	t.Parallel()

	gob.Register(testRecord{})

	dir := t.TempDir()
	store := NewFileReportStore(dir)

	analyzerIDs := []string{"history/burndown", "history/couples", "history/devs"}

	for _, id := range analyzerIDs {
		meta := ReportMeta{AnalyzerID: id, Version: "v1"}

		w, err := store.Begin(id, meta)
		require.NoError(t, err)
		require.NoError(t, w.Write("summary", testRecord{Name: id, Value: 1}))
		require.NoError(t, w.Close())
	}

	ids := store.AnalyzerIDs()
	assert.Equal(t, analyzerIDs, ids)
}

func TestFileReportStore_Open_NonExistent(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	store := NewFileReportStore(dir)

	_, err := store.Open("nonexistent")
	require.Error(t, err)
	assert.ErrorIs(t, err, ErrAnalyzerNotFound)
}

func TestFileReportStore_TornWrite(t *testing.T) {
	t.Parallel()

	gob.Register(testRecord{})

	dir := t.TempDir()
	store := NewFileReportStore(dir)

	meta := ReportMeta{AnalyzerID: "history/burndown", Version: "v1"}

	w, err := store.Begin("history/burndown", meta)
	require.NoError(t, err)
	require.NoError(t, w.Write("summary", testRecord{Name: "orphan", Value: 1}))

	// Manually create a .tmp file to simulate partial/torn write.
	fw, ok := w.(*fileReportWriter)
	require.True(t, ok, "expected *fileReportWriter")

	for kind, gw := range fw.writers {
		tmpPath := filepath.Join(fw.dir, kind+tmpExtension)
		require.NoError(t, os.WriteFile(tmpPath, gw.buf.Bytes(), filePerm))
	}

	_, openErr := store.Open("history/burndown")
	require.Error(t, openErr)
	assert.ErrorIs(t, openErr, ErrTornWrite)
}

func TestFileReportStore_WriterClose_Idempotent(t *testing.T) {
	t.Parallel()

	gob.Register(testRecord{})

	dir := t.TempDir()
	store := NewFileReportStore(dir)

	meta := ReportMeta{AnalyzerID: "history/burndown", Version: "v1"}

	w, err := store.Begin("history/burndown", meta)
	require.NoError(t, err)
	require.NoError(t, w.Write("summary", testRecord{Name: "a", Value: 1}))
	require.NoError(t, w.Close())
	require.NoError(t, w.Close())
}

func TestFileReportStore_WriteAfterClose(t *testing.T) {
	t.Parallel()

	gob.Register(testRecord{})

	dir := t.TempDir()
	store := NewFileReportStore(dir)

	meta := ReportMeta{AnalyzerID: "history/burndown", Version: "v1"}

	w, err := store.Begin("history/burndown", meta)
	require.NoError(t, err)
	require.NoError(t, w.Close())

	err = w.Write("summary", testRecord{Name: "late", Value: 1})
	require.Error(t, err)
	assert.ErrorIs(t, err, ErrWriterClosed)
}

func TestFileReportStore_MemoryBounded_LargeIteration(t *testing.T) {
	t.Parallel()

	gob.Register(testRecord{})

	dir := t.TempDir()
	store := NewFileReportStore(dir)

	const recordCount = 10_000

	meta := ReportMeta{AnalyzerID: "history/burndown", Version: "v1"}

	w, err := store.Begin("history/burndown", meta)
	require.NoError(t, err)

	for i := range recordCount {
		require.NoError(t, w.Write("timeseries", testRecord{Name: "row", Value: i}))
	}

	require.NoError(t, w.Close())

	// Iterate all records — memory should stay bounded
	// because only one decoded record exists at a time.
	r, err := store.Open("history/burndown")
	require.NoError(t, err)

	count := 0

	err = r.Iter("timeseries", func(raw []byte) error {
		var rec testRecord

		decErr := GobDecode(raw, &rec)
		if decErr != nil {
			return decErr
		}

		count++

		return nil
	})
	require.NoError(t, err)
	assert.Equal(t, recordCount, count)
	require.NoError(t, r.Close())
}
