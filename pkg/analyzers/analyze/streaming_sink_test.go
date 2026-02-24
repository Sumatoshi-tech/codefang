package analyze_test

import (
	"bytes"
	"encoding/json"
	"errors"
	"io"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestStreamingSink_WriteTC_SingleLine(t *testing.T) {
	t.Parallel()

	var buf bytes.Buffer

	sink := analyze.NewStreamingSink(&buf)

	tc := analyze.TC{
		CommitHash: gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
		Tick:       0,
		AuthorID:   1,
		Timestamp:  time.Date(2024, 1, 15, 10, 30, 0, 0, time.UTC),
		Data:       map[string]any{"score": 42},
	}

	err := sink.WriteTC(tc, "quality")
	require.NoError(t, err)

	var line map[string]any

	err = json.Unmarshal(buf.Bytes(), &line)
	require.NoError(t, err)

	assert.Equal(t, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", line["hash"])
	assert.InDelta(t, 0, line["tick"], 0)
	assert.InDelta(t, 1, line["author_id"], 0)
	assert.Equal(t, "2024-01-15T10:30:00Z", line["timestamp"])
	assert.Equal(t, "quality", line["analyzer"])
	assert.NotNil(t, line["data"])
}

func TestStreamingSink_WriteTC_NilData(t *testing.T) {
	t.Parallel()

	var buf bytes.Buffer

	sink := analyze.NewStreamingSink(&buf)

	tc := analyze.TC{
		CommitHash: gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
		Data:       nil,
	}

	err := sink.WriteTC(tc, "quality")
	require.NoError(t, err)
	assert.Empty(t, buf.String(), "nil Data should produce no output")
}

func TestStreamingSink_WriteTC_MultipleLines(t *testing.T) {
	t.Parallel()

	var buf bytes.Buffer

	sink := analyze.NewStreamingSink(&buf)

	for i := range 3 {
		tc := analyze.TC{
			CommitHash: gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
			Tick:       i,
			Timestamp:  time.Date(2024, 1, 1+i, 0, 0, 0, 0, time.UTC),
			Data:       map[string]any{"val": i},
		}

		err := sink.WriteTC(tc, "quality")
		require.NoError(t, err)
	}

	lines := strings.Split(strings.TrimSpace(buf.String()), "\n")
	assert.Len(t, lines, 3)

	// Each line must be valid JSON.
	for _, line := range lines {
		var obj map[string]any

		err := json.Unmarshal([]byte(line), &obj)
		require.NoError(t, err)
	}
}

func TestStreamingSink_WriteTC_ConcurrentWrites(t *testing.T) {
	t.Parallel()

	var buf bytes.Buffer

	sink := analyze.NewStreamingSink(&buf)

	const goroutines = 10

	var wg sync.WaitGroup

	wg.Add(goroutines)

	for g := range goroutines {
		go func(id int) {
			defer wg.Done()

			tc := analyze.TC{
				CommitHash: gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
				Tick:       id,
				Timestamp:  time.Date(2024, 1, 1, 0, 0, 0, 0, time.UTC),
				Data:       map[string]any{"id": id},
			}

			writeErr := sink.WriteTC(tc, "quality")
			assert.NoError(t, writeErr)
		}(g)
	}

	wg.Wait()

	lines := strings.Split(strings.TrimSpace(buf.String()), "\n")
	assert.Len(t, lines, goroutines)

	for _, line := range lines {
		var obj map[string]any

		err := json.Unmarshal([]byte(line), &obj)
		require.NoError(t, err, "each line must be valid JSON: %s", line)
	}
}

// errBrokenPipe is a sentinel error for testing writer failures.
var errBrokenPipe = errors.New("broken pipe")

// errWriter always returns an error on Write.
type errWriter struct{}

func (errWriter) Write(_ []byte) (int, error) {
	return 0, errBrokenPipe
}

func TestStreamingSink_WriteTC_WriterError(t *testing.T) {
	t.Parallel()

	sink := analyze.NewStreamingSink(errWriter{})

	tc := analyze.TC{
		CommitHash: gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
		Data:       map[string]any{"val": 1},
	}

	err := sink.WriteTC(tc, "quality")
	require.Error(t, err)
}

func TestStreamingSink_WriteTC_ZeroTimestamp(t *testing.T) {
	t.Parallel()

	var buf bytes.Buffer

	sink := analyze.NewStreamingSink(&buf)

	tc := analyze.TC{
		CommitHash: gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
		Data:       map[string]any{"val": 1},
	}

	err := sink.WriteTC(tc, "quality")
	require.NoError(t, err)

	var line map[string]any

	err = json.Unmarshal(buf.Bytes(), &line)
	require.NoError(t, err)

	assert.Empty(t, line["timestamp"], "zero timestamp should produce empty string")
}

func TestStreamingSink_ImplementsTCSink(t *testing.T) {
	t.Parallel()

	// Verify StreamingSink.WriteTC has the right signature for TCSink.
	sink := analyze.NewStreamingSink(io.Discard)

	var fn analyze.TCSink = sink.WriteTC

	assert.NotNil(t, fn)
}
