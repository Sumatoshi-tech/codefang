// Package hll provides a HyperLogLog cardinality estimator.
//
// HyperLogLog estimates the number of distinct elements in a multiset with
// approximately 2% standard error using only 2^p bytes of memory (e.g., 16 KB
// for precision 14). It is useful for counting unique items (developers,
// files, tokens) without maintaining a full set.
//
// This implementation uses the LogLog-Beta bias correction from Qin et al.
// (2016), which provides accurate estimates across all cardinality ranges
// without the piecewise linear interpolation tables of HLL++.
package hll

import (
	"errors"
	"hash/fnv"
	"math"
	"math/bits"
	"sync"
)

const (
	// minPrecision is the minimum allowed precision (2^4 = 16 registers).
	minPrecision = 4

	// maxPrecision is the maximum allowed precision (2^18 = 262144 registers).
	maxPrecision = 18

	// hashBits is the total number of bits in the hash output.
	hashBits = 64

	// precisionP5 is precision 5 for alpha constant lookup.
	precisionP5 = 5

	// precisionP6 is precision 6 for alpha constant lookup.
	precisionP6 = 6

	// alphaP4 is the alpha constant for 2^4 = 16 registers.
	alphaP4 = 0.673

	// alphaP5 is the alpha constant for 2^5 = 32 registers.
	alphaP5 = 0.697

	// alphaP6 is the alpha constant for 2^6 = 64 registers.
	alphaP6 = 0.709

	// alphaGenericNumerator is the numerator in the generic alpha formula.
	alphaGenericNumerator = 0.7213

	// alphaGenericDenominatorCoeff is the coefficient in the generic alpha denominator.
	alphaGenericDenominatorCoeff = 1.079

	// LogLog-Beta polynomial coefficients from Qin et al. (2016).
	betaC0 = -0.370393911
	betaC1 = 0.070471823
	betaC2 = 0.17393686
	betaC3 = 0.16339839
	betaC4 = -0.09237745
	betaC5 = 0.03738027
	betaC6 = -0.005384159
	betaC7 = 0.00042419

	// mix64 constants from splitmix64 finalizer by Vigna (2014).
	mixShift1 = 30
	mixMul1   = 0xbf58476d1ce4e5b9
	mixShift2 = 27
	mixMul2   = 0x94d049bb133111eb
	mixShift3 = 31
)

var (
	// ErrPrecisionOutOfRange is returned when precision is not in [4, 18].
	ErrPrecisionOutOfRange = errors.New("hll: precision must be in [4, 18]")

	// ErrPrecisionMismatch is returned when merging sketches with different precisions.
	ErrPrecisionMismatch = errors.New("hll: cannot merge sketches with different precisions")
)

// Sketch is a thread-safe HyperLogLog cardinality estimator.
type Sketch struct {
	mu        sync.RWMutex
	registers []uint8
	precision uint8
}

// New creates a HyperLogLog sketch with the given precision p.
// Precision must be in [4, 18]. The sketch allocates 2^p registers (bytes).
func New(precision uint8) (*Sketch, error) {
	if precision < minPrecision || precision > maxPrecision {
		return nil, ErrPrecisionOutOfRange
	}

	regCount := uint(1) << precision

	return &Sketch{
		registers: make([]uint8, regCount),
		precision: precision,
	}, nil
}

// Add inserts data into the sketch by hashing it and updating the
// appropriate register with the observed number of leading zeros.
func (s *Sketch) Add(data []byte) {
	hashVal := hash64(data)
	idx := hashVal >> (hashBits - s.precision)

	// Mask out the upper p bits to get the remaining (64-p) bits.
	// Count the position of the leftmost 1-bit (rho = leading zeros + 1).
	// When all remaining bits are zero, rho = 64-p+1 (maximum).
	remaining := hashBits - uint(s.precision)
	mask := (uint64(1) << remaining) - 1
	w := hashVal & mask

	rho := uint8(remaining-uint(bits.Len64(w))) + 1

	s.mu.Lock()

	if rho > s.registers[idx] {
		s.registers[idx] = rho
	}

	s.mu.Unlock()
}

// Count returns the estimated number of distinct elements that have been
// added to the sketch. Uses LogLog-Beta bias correction for accuracy across
// all cardinality ranges.
func (s *Sketch) Count() uint64 {
	s.mu.RLock()
	defer s.mu.RUnlock()

	return s.countLocked()
}

// countLocked computes the cardinality estimate. Caller must hold at least a read lock.
// Uses the LogLog-Beta formula: alpha * m * (m - ez) / (beta(ez) + sum).
func (s *Sketch) countLocked() uint64 {
	regCount := float64(uint(1) << s.precision)
	zeros := float64(countZeroRegisters(s.registers))

	if zeros == regCount {
		return 0
	}

	alphaM := alpha(s.precision)
	harmonicSum := computeHarmonicSum(s.registers)
	betaVal := betaCorrection(zeros)
	estimate := alphaM * regCount * (regCount - zeros) / (betaVal + harmonicSum)

	return uint64(math.Round(estimate))
}

// countZeroRegisters counts registers that are still at zero.
func countZeroRegisters(registers []uint8) int {
	count := 0

	for _, val := range registers {
		if val == 0 {
			count++
		}
	}

	return count
}

// computeHarmonicSum computes the sum of 2^(-M[j]) for all registers.
func computeHarmonicSum(registers []uint8) float64 {
	sum := 0.0

	for _, val := range registers {
		sum += math.Exp2(-float64(val))
	}

	return sum
}

// alpha returns the alpha_m constant used in the HLL estimate formula.
// For m >= 128, alpha_m = 0.7213 / (1 + 1.079/m).
func alpha(precision uint8) float64 {
	regCount := float64(uint(1) << precision)

	switch precision {
	case minPrecision:
		return alphaP4
	case precisionP5:
		return alphaP5
	case precisionP6:
		return alphaP6
	default:
		return alphaGenericNumerator / (1 + alphaGenericDenominatorCoeff/regCount)
	}
}

// betaCorrection computes the LogLog-Beta bias correction term from Qin et al. (2016).
// This polynomial approximation corrects for bias across all cardinality ranges.
func betaCorrection(zeroCount float64) float64 {
	zl := math.Log(zeroCount + 1)
	zl2 := zl * zl
	zl3 := zl2 * zl
	zl4 := zl3 * zl
	zl5 := zl4 * zl
	zl6 := zl5 * zl
	zl7 := zl6 * zl

	return betaC0*zeroCount +
		betaC1*zl +
		betaC2*zl2 +
		betaC3*zl3 +
		betaC4*zl4 +
		betaC5*zl5 +
		betaC6*zl6 +
		betaC7*zl7
}

// hash64 computes a 64-bit hash of data using FNV-1a followed by a
// bit-mixing finalizer. The finalizer ensures good avalanche properties
// across all bit positions, which is critical for HyperLogLog where
// both high bits (register index) and low bits (leading zeros) must be
// well-distributed.
func hash64(data []byte) uint64 {
	h := fnv.New64a()
	_, _ = h.Write(data)

	return mix64(h.Sum64())
}

// mix64 applies a splitmix64-style finalizer for full-avalanche mixing.
func mix64(v uint64) uint64 {
	v ^= v >> mixShift1
	v *= mixMul1
	v ^= v >> mixShift2
	v *= mixMul2
	v ^= v >> mixShift3

	return v
}
