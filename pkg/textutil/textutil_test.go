package textutil

import (
	"io"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestIsBinary_EmptyData(t *testing.T) {
	t.Parallel()

	assert.False(t, IsBinary(nil))
	assert.False(t, IsBinary([]byte{}))
}

func TestIsBinary_PureText(t *testing.T) {
	t.Parallel()

	assert.False(t, IsBinary([]byte("hello world\n")))
}

func TestIsBinary_NullByte(t *testing.T) {
	t.Parallel()

	assert.True(t, IsBinary([]byte("hello\x00world")))
}

func TestIsBinary_NullAtStart(t *testing.T) {
	t.Parallel()

	assert.True(t, IsBinary([]byte("\x00start")))
}

func TestIsBinary_NullAtSniffBoundary(t *testing.T) {
	t.Parallel()

	// Null byte at exactly position BinarySniffLength-1 should be detected.
	data := make([]byte, BinarySniffLength)
	data[BinarySniffLength-1] = 0x00

	assert.True(t, IsBinary(data))
}

func TestIsBinary_NullBeyondSniffBoundary(t *testing.T) {
	t.Parallel()

	// Null byte beyond the sniff window should NOT be detected.
	data := make([]byte, BinarySniffLength+100)
	for i := range data {
		data[i] = 'a'
	}

	data[BinarySniffLength+50] = 0x00

	assert.False(t, IsBinary(data))
}

func TestIsBinary_ShortDataNoNull(t *testing.T) {
	t.Parallel()

	assert.False(t, IsBinary([]byte("short")))
}

func TestCountLines_EmptyData(t *testing.T) {
	t.Parallel()

	assert.Equal(t, 0, CountLines(nil))
	assert.Equal(t, 0, CountLines([]byte{}))
}

func TestCountLines_SingleLineNoNewline(t *testing.T) {
	t.Parallel()

	assert.Equal(t, 1, CountLines([]byte("hello")))
}

func TestCountLines_SingleLineWithNewline(t *testing.T) {
	t.Parallel()

	assert.Equal(t, 1, CountLines([]byte("hello\n")))
}

func TestCountLines_MultipleLines(t *testing.T) {
	t.Parallel()

	assert.Equal(t, 3, CountLines([]byte("a\nb\nc\n")))
}

func TestCountLines_MultipleLinesNoTrailingNewline(t *testing.T) {
	t.Parallel()

	assert.Equal(t, 3, CountLines([]byte("a\nb\nc")))
}

func TestCountLines_EmptyLines(t *testing.T) {
	t.Parallel()

	// "\n\n\n" = 3 empty lines.
	assert.Equal(t, 3, CountLines([]byte("\n\n\n")))
}

func TestCountLines_SingleNewline(t *testing.T) {
	t.Parallel()

	assert.Equal(t, 1, CountLines([]byte("\n")))
}

func TestCountLines_LargeFile(t *testing.T) {
	t.Parallel()

	lines := strings.Repeat("line\n", 10000)

	assert.Equal(t, 10000, CountLines([]byte(lines)))
}

func TestBytesReader_EmptyData(t *testing.T) {
	t.Parallel()

	rc := BytesReader(nil)
	defer rc.Close()

	data, err := io.ReadAll(rc)

	require.NoError(t, err)
	assert.Empty(t, data)
}

func TestBytesReader_RoundTrip(t *testing.T) {
	t.Parallel()

	input := []byte("hello world")
	rc := BytesReader(input)

	defer rc.Close()

	data, err := io.ReadAll(rc)

	require.NoError(t, err)
	assert.Equal(t, input, data)
}

func TestBytesReader_CloseIsIdempotent(t *testing.T) {
	t.Parallel()

	rc := BytesReader([]byte("test"))

	require.NoError(t, rc.Close())
	require.NoError(t, rc.Close())
}

func TestBinarySniffLength_Value(t *testing.T) {
	t.Parallel()

	// BinarySniffLength matches the well-known 8000-byte heuristic.
	assert.Equal(t, 8000, BinarySniffLength)
}
