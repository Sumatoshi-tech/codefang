// Package gitlib provides a unified interface for git operations using libgit2.
// This replaces go-git with the faster libgit2 C library.
package gitlib

import (
	git2go "github.com/libgit2/git2go/v34"
)

// Constants for hash operations.
const (
	// HashSize is the size of a SHA-1 hash in bytes.
	HashSize = 20
	// HashHexSize is the size of a hex-encoded SHA-1 hash.
	HashHexSize = 40
	// HexBase is the base for hexadecimal digits a-f.
	hexBase = 10
	// HexShift is the bit shift for the high nibble.
	hexShift = 4
)

// Hash represents a git object hash (SHA-1).
type Hash [HashSize]byte

// ZeroHash returns the zero value hash.
func ZeroHash() Hash {
	return Hash{}
}

// NewHash creates a Hash from a hex string.
// Used for testing and initialization.
func NewHash(hexStr string) Hash {
	var hash Hash

	for i := 0; i < HashSize && i*2+1 < len(hexStr); i++ {
		c1, c2 := hexStr[i*2], hexStr[i*2+1]
		hash[i] = hexCharToNibble(c1)<<hexShift | hexCharToNibble(c2)
	}

	return hash
}

// hexCharToNibble converts a hex character to its 4-bit value.
func hexCharToNibble(char byte) byte {
	switch {
	case char >= '0' && char <= '9':
		return char - '0'
	case char >= 'a' && char <= 'f':
		return char - 'a' + hexBase
	case char >= 'A' && char <= 'F':
		return char - 'A' + hexBase
	default:
		return 0
	}
}

// HashFromOid converts a libgit2 Oid to Hash.
func HashFromOid(oid *git2go.Oid) Hash {
	var h Hash
	copy(h[:], oid[:])

	return h
}

// String returns the hex representation of the hash.
func (h Hash) String() string {
	const hexChars = "0123456789abcdef"

	buf := make([]byte, HashHexSize)

	for i, byteVal := range h {
		buf[i*2] = hexChars[byteVal>>hexShift]
		buf[i*2+1] = hexChars[byteVal&0x0f]
	}

	return string(buf)
}

// IsZero returns true if the hash is all zeros.
func (h Hash) IsZero() bool {
	for _, b := range h {
		if b != 0 {
			return false
		}
	}

	return true
}

// ToOid converts Hash back to libgit2 Oid.
func (h Hash) ToOid() *git2go.Oid {
	oid := new(git2go.Oid)
	copy(oid[:], h[:])

	return oid
}
