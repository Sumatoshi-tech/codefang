// Package rbtree provides a red-black tree implementation with memory allocation
// and serialization support, including LZ4 compression and sharded allocators.
package rbtree

import (
	"bytes"
	"encoding/binary"

	"github.com/pierrec/lz4/v4"
)

// uint32ByteSize is the number of bytes in a uint32.
const uint32ByteSize = 4

// CompressUInt32Slice compresses a slice of uint32-s with LZ4.
func CompressUInt32Slice(data []uint32) []byte {
	buf := new(bytes.Buffer)

	writeErr := binary.Write(buf, binary.LittleEndian, data)
	if writeErr != nil {
		return nil
	}

	compressed := make([]byte, lz4.CompressBlockBound(buf.Len()))

	written, err := lz4.CompressBlock(buf.Bytes(), compressed, nil)
	if err != nil || written == 0 {
		return nil
	}

	return compressed[:written]
}

// DecompressUInt32Slice decompresses a slice of uint32-s previously compressed with LZ4.
// `result` must be preallocated.
func DecompressUInt32Slice(data []byte, result []uint32) {
	decompressed := make([]byte, len(result)*uint32ByteSize)

	_, err := lz4.UncompressBlock(data, decompressed)
	if err != nil {
		return
	}

	readErr := binary.Read(bytes.NewReader(decompressed), binary.LittleEndian, result)
	if readErr != nil {
		return
	}
}

// DeltaEncodeUInt32Slice replaces each element with the difference from its
// predecessor, in place. The first element is left unchanged. This transforms
// sorted sequences into small, repetitive values that compress better with LZ4.
func DeltaEncodeUInt32Slice(data []uint32) {
	for i := len(data) - 1; i > 0; i-- {
		data[i] -= data[i-1]
	}
}

// DeltaDecodeUInt32Slice performs a prefix-sum to restore original values from
// deltas produced by DeltaEncodeUInt32Slice. The operation is performed in place.
func DeltaDecodeUInt32Slice(data []uint32) {
	for i := 1; i < len(data); i++ {
		data[i] += data[i-1]
	}
}
