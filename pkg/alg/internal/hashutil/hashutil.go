// Package hashutil provides shared hash mixing constants and functions
// for probabilistic data structures (Count-Min Sketch, HyperLogLog, MinHash).
//
// All functions use the splitmix64 finalizer by Vigna (2014) which provides
// full-avalanche mixing across all 64 bits.
package hashutil

import "hash/fnv"

// Splitmix64 constants from the splitmix64 finalizer by Vigna (2014).
const (
	// BaseSeed is the starting seed for deterministic seed generation.
	BaseSeed = 0x517cc1b727220a95

	// MixShift1 is the first right-shift in the splitmix64 finalizer.
	MixShift1 = 30

	// MixMul1 is the first multiplier in the splitmix64 finalizer.
	MixMul1 = 0xbf58476d1ce4e5b9

	// MixShift2 is the second right-shift in the splitmix64 finalizer.
	MixShift2 = 27

	// MixMul2 is the second multiplier in the splitmix64 finalizer.
	MixMul2 = 0x94d049bb133111eb

	// MixShift3 is the third right-shift in the splitmix64 finalizer.
	MixShift3 = 31

	// splitmix64Increment is the golden-ratio-derived increment
	// used in the Splitmix64 state-advance function.
	splitmix64Increment = 0x9e3779b97f4a7c15
)

// Mix64 applies the splitmix64 finalizer for full-avalanche mixing.
// This is a pure output function — it does NOT advance any state.
func Mix64(v uint64) uint64 {
	v ^= v >> MixShift1
	v *= MixMul1
	v ^= v >> MixShift2
	v *= MixMul2
	v ^= v >> MixShift3

	return v
}

// Splitmix64 advances the state by the golden-ratio increment and applies
// the mix64 finalizer. This is a full PRNG step that both advances state
// and produces output.
func Splitmix64(state uint64) uint64 {
	state += splitmix64Increment
	z := state
	z = (z ^ (z >> MixShift1)) * MixMul1
	z = (z ^ (z >> MixShift2)) * MixMul2
	z ^= z >> MixShift3

	return z
}

// MixHash combines a base hash with a seed using XOR and the splitmix64 finalizer.
// This produces a deterministic hash variation for a given (base, seed) pair.
func MixHash(base, seed uint64) uint64 {
	x := base ^ seed
	x = (x ^ (x >> MixShift1)) * MixMul1
	x = (x ^ (x >> MixShift2)) * MixMul2
	x ^= x >> MixShift3

	return x
}

// FNV64a computes a 64-bit FNV-1a hash of the given data.
func FNV64a(data []byte) uint64 {
	h := fnv.New64a()
	_, _ = h.Write(data)

	return h.Sum64()
}

// GenerateSeeds creates n deterministic seeds using the given advance function.
// Use Mix64 for CMS-style seeds or Splitmix64 for MinHash-style seeds.
func GenerateSeeds(n int, advance func(uint64) uint64) []uint64 {
	seeds := make([]uint64, n)
	state := uint64(BaseSeed)

	for i := range n {
		state = advance(state)
		seeds[i] = state
	}

	return seeds
}
