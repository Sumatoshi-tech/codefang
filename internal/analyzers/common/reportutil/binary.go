package reportutil

import (
	"bytes"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math"

	"github.com/Sumatoshi-tech/codefang/pkg/safeconv"
)

const (
	// BinaryMagic marks Codefang binary envelopes.
	BinaryMagic = "CFB1"
	// binaryHeaderSize is magic bytes + payload length bytes.
	binaryHeaderSize = 8
)

var (
	// ErrInvalidBinaryEnvelope indicates malformed or truncated binary payload.
	ErrInvalidBinaryEnvelope = errors.New("invalid binary envelope")
	// ErrBinaryPayloadTooLarge indicates payload exceeds binary envelope limit.
	ErrBinaryPayloadTooLarge = errors.New("binary payload too large")
)

// EncodeBinaryEnvelope serializes any JSON-serializable value into a binary envelope.
func EncodeBinaryEnvelope(value any, writer io.Writer) error {
	payload, err := json.Marshal(value)
	if err != nil {
		return fmt.Errorf("marshal binary payload: %w", err)
	}

	if len(payload) > math.MaxUint32 {
		return fmt.Errorf("%w: %d bytes", ErrBinaryPayloadTooLarge, len(payload))
	}

	header := make([]byte, binaryHeaderSize)
	copy(header[:4], BinaryMagic)
	binary.LittleEndian.PutUint32(header[4:], safeconv.MustIntToUint32(len(payload)))

	_, err = writer.Write(header)
	if err != nil {
		return fmt.Errorf("write binary header: %w", err)
	}

	_, err = writer.Write(payload)
	if err != nil {
		return fmt.Errorf("write binary payload: %w", err)
	}

	return nil
}

// DecodeBinaryEnvelope extracts the JSON payload from a binary envelope.
func DecodeBinaryEnvelope(reader io.Reader) ([]byte, error) {
	header := make([]byte, binaryHeaderSize)

	_, err := io.ReadFull(reader, header)
	if err != nil {
		return nil, errors.Join(ErrInvalidBinaryEnvelope, err)
	}

	if !bytes.Equal(header[:4], []byte(BinaryMagic)) {
		return nil, fmt.Errorf("%w: bad magic", ErrInvalidBinaryEnvelope)
	}

	payloadLen := binary.LittleEndian.Uint32(header[4:])
	payload := make([]byte, payloadLen)

	_, err = io.ReadFull(reader, payload)
	if err != nil {
		return nil, errors.Join(ErrInvalidBinaryEnvelope, err)
	}

	return payload, nil
}

// DecodeBinaryEnvelopes decodes all concatenated binary envelopes from bytes.
func DecodeBinaryEnvelopes(data []byte) ([][]byte, error) {
	reader := bytes.NewReader(data)
	payloads := make([][]byte, 0)

	for reader.Len() > 0 {
		payload, err := DecodeBinaryEnvelope(reader)
		if err != nil {
			return nil, err
		}

		payloads = append(payloads, payload)
	}

	return payloads, nil
}
