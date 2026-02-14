package reportutil

import (
	"bytes"
	"encoding/json"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestEncodeDecodeBinaryEnvelope_RoundTrip(t *testing.T) {
	t.Parallel()

	input := map[string]any{
		"name":  "test",
		"value": 42,
	}

	var buf bytes.Buffer

	err := EncodeBinaryEnvelope(input, &buf)
	require.NoError(t, err)

	payload, err := DecodeBinaryEnvelope(&buf)
	require.NoError(t, err)

	decoded := make(map[string]any)
	err = json.Unmarshal(payload, &decoded)
	require.NoError(t, err)
	require.Equal(t, "test", decoded["name"])
	require.InDelta(t, float64(42), decoded["value"], 0)
}

func TestDecodeBinaryEnvelope_InvalidMagic(t *testing.T) {
	t.Parallel()

	buf := bytes.NewBufferString("BAD!\x00\x00\x00\x00")
	_, err := DecodeBinaryEnvelope(buf)
	require.ErrorIs(t, err, ErrInvalidBinaryEnvelope)
}

func TestDecodeBinaryEnvelope_Truncated(t *testing.T) {
	t.Parallel()

	buf := bytes.NewBuffer([]byte{'C', 'F', 'B', '1', 0x05, 0x00, 0x00, 0x00, 'a'})
	_, err := DecodeBinaryEnvelope(buf)
	require.ErrorIs(t, err, ErrInvalidBinaryEnvelope)
}

func TestDecodeBinaryEnvelopes(t *testing.T) {
	t.Parallel()

	var buf bytes.Buffer
	require.NoError(t, EncodeBinaryEnvelope(map[string]any{"id": "first"}, &buf))
	require.NoError(t, EncodeBinaryEnvelope(map[string]any{"id": "second"}, &buf))

	payloads, err := DecodeBinaryEnvelopes(buf.Bytes())
	require.NoError(t, err)
	require.Len(t, payloads, 2)

	first := map[string]any{}
	require.NoError(t, json.Unmarshal(payloads[0], &first))
	require.Equal(t, "first", first["id"])

	second := map[string]any{}
	require.NoError(t, json.Unmarshal(payloads[1], &second))
	require.Equal(t, "second", second["id"])
}

func TestDecodeBinaryEnvelopes_InvalidPayload(t *testing.T) {
	t.Parallel()

	_, err := DecodeBinaryEnvelopes([]byte("bad"))
	require.ErrorIs(t, err, ErrInvalidBinaryEnvelope)
}
