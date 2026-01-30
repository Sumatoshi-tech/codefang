package gitlib_test

import (
	"testing"

	git2go "github.com/libgit2/git2go/v34"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestZeroHash(t *testing.T) {
	hash := gitlib.ZeroHash()

	assert.Equal(t, gitlib.Hash{}, hash)
	assert.True(t, hash.IsZero())
}

func TestNewHash(t *testing.T) {
	tests := []struct {
		name     string
		input    string
		expected gitlib.Hash
	}{
		{
			name:  "full lowercase hex",
			input: "0123456789abcdef0123456789abcdef01234567",
			expected: gitlib.Hash{
				0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef,
				0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef,
				0x01, 0x23, 0x45, 0x67,
			},
		},
		{
			name:  "full uppercase hex",
			input: "0123456789ABCDEF0123456789ABCDEF01234567",
			expected: gitlib.Hash{
				0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef,
				0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef,
				0x01, 0x23, 0x45, 0x67,
			},
		},
		{
			name:  "all zeros",
			input: "0000000000000000000000000000000000000000",
			expected: gitlib.Hash{
				0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
				0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
				0x00, 0x00, 0x00, 0x00,
			},
		},
		{
			name:  "all f's",
			input: "ffffffffffffffffffffffffffffffffffffffff",
			expected: gitlib.Hash{
				0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
				0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
				0xff, 0xff, 0xff, 0xff,
			},
		},
		{
			name:     "short string",
			input:    "abcd",
			expected: gitlib.Hash{0xab, 0xcd},
		},
		{
			name:     "empty string",
			input:    "",
			expected: gitlib.Hash{},
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			result := gitlib.NewHash(tc.input)
			assert.Equal(t, tc.expected, result)
		})
	}
}

func TestHashString(t *testing.T) {
	tests := []struct {
		name     string
		hash     gitlib.Hash
		expected string
	}{
		{
			name:     "zero hash",
			hash:     gitlib.Hash{},
			expected: "0000000000000000000000000000000000000000",
		},
		{
			name: "all f's",
			hash: gitlib.Hash{
				0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
				0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
				0xff, 0xff, 0xff, 0xff,
			},
			expected: "ffffffffffffffffffffffffffffffffffffffff",
		},
		{
			name: "mixed",
			hash: gitlib.Hash{
				0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef,
				0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef,
				0x01, 0x23, 0x45, 0x67,
			},
			expected: "0123456789abcdef0123456789abcdef01234567",
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			result := tc.hash.String()
			assert.Equal(t, tc.expected, result)
		})
	}
}

func TestHashIsZero(t *testing.T) {
	tests := []struct {
		name     string
		hash     gitlib.Hash
		expected bool
	}{
		{
			name:     "zero hash",
			hash:     gitlib.Hash{},
			expected: true,
		},
		{
			name:     "non-zero first byte",
			hash:     gitlib.Hash{0x01},
			expected: false,
		},
		{
			name: "non-zero last byte",
			hash: gitlib.Hash{
				0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
				0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
				0x00, 0x00, 0x00, 0x01,
			},
			expected: false,
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			result := tc.hash.IsZero()
			assert.Equal(t, tc.expected, result)
		})
	}
}

func TestHashRoundTrip(t *testing.T) {
	original := "0123456789abcdef0123456789abcdef01234567"

	hash := gitlib.NewHash(original)
	result := hash.String()

	assert.Equal(t, original, result)
}

func TestHashFromOid(t *testing.T) {
	oid := new(git2go.Oid)
	copy(oid[:], []byte{
		0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef,
		0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef,
		0x01, 0x23, 0x45, 0x67,
	})

	hash := gitlib.HashFromOid(oid)

	expected := gitlib.Hash{
		0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef,
		0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef,
		0x01, 0x23, 0x45, 0x67,
	}
	assert.Equal(t, expected, hash)
}

func TestHashToOid(t *testing.T) {
	hash := gitlib.Hash{
		0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef,
		0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef,
		0x01, 0x23, 0x45, 0x67,
	}

	oid := hash.ToOid()
	require.NotNil(t, oid)

	// Convert back to verify.
	roundTrip := gitlib.HashFromOid(oid)
	assert.Equal(t, hash, roundTrip)
}

func TestHashOidRoundTrip(t *testing.T) {
	original := new(git2go.Oid)
	copy(original[:], []byte{
		0xde, 0xad, 0xbe, 0xef, 0xca, 0xfe, 0xba, 0xbe,
		0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0,
		0xfe, 0xdc, 0xba, 0x98,
	})

	hash := gitlib.HashFromOid(original)
	result := hash.ToOid()

	assert.Equal(t, original[:], result[:])
}
