package rbtree

import (
	"bytes"
	"encoding/binary"

	"github.com/pierrec/lz4/v4"
)

// CompressUInt32Slice compresses a slice of uint32-s with LZ4.
func CompressUInt32Slice(data []uint32) []byte {
	buf := new(bytes.Buffer)
	_ = binary.Write(buf, binary.LittleEndian, data)

	compressed := make([]byte, lz4.CompressBlockBound(buf.Len()))
	n, err := lz4.CompressBlock(buf.Bytes(), compressed, nil)
	if err != nil || n == 0 {
		return nil
	}

	return compressed[:n]
}

// DecompressUInt32Slice decompresses a slice of uint32-s previously compressed with LZ4.
// `result` must be preallocated.
func DecompressUInt32Slice(data []byte, result []uint32) {
	decompressed := make([]byte, len(result)*4)
	_, err := lz4.UncompressBlock(data, decompressed)
	if err != nil {
		return
	}
	_ = binary.Read(bytes.NewReader(decompressed), binary.LittleEndian, result)
}
