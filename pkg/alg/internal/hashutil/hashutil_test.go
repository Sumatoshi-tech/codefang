package hashutil

import (
	"testing"
)

func TestMix64_Deterministic(t *testing.T) {
	t.Parallel()

	// Same input must always produce same output.
	input := uint64(0x12345678)
	result1 := Mix64(input)
	result2 := Mix64(input)

	if result1 != result2 {
		t.Errorf("Mix64 not deterministic: %x != %x", result1, result2)
	}
}

func TestMix64_Avalanche(t *testing.T) {
	t.Parallel()

	// Adjacent inputs should produce very different outputs.
	a := Mix64(0)
	b := Mix64(1)

	if a == b {
		t.Error("Mix64(0) == Mix64(1); expected avalanche")
	}
}

func TestMix64_Zero(t *testing.T) {
	t.Parallel()

	// Mix64(0) = 0 is expected: the finalizer is multiplicative,
	// so 0 is a fixed point. This documents the known behavior.
	result := Mix64(0)
	if result != 0 {
		t.Errorf("Mix64(0) = %x; expected 0 (fixed point)", result)
	}
}

func TestSplitmix64_Deterministic(t *testing.T) {
	t.Parallel()

	input := uint64(0xAAAABBBBCCCCDDDD)
	result1 := Splitmix64(input)
	result2 := Splitmix64(input)

	if result1 != result2 {
		t.Errorf("Splitmix64 not deterministic: %x != %x", result1, result2)
	}
}

func TestSplitmix64_DiffersFromMix64(t *testing.T) {
	t.Parallel()

	// Splitmix64 adds an increment before mixing, so results should differ.
	input := uint64(42)
	mix := Mix64(input)
	split := Splitmix64(input)

	if mix == split {
		t.Errorf("Splitmix64 and Mix64 produced same result for input %d", input)
	}
}

func TestSplitmix64_Sequence(t *testing.T) {
	t.Parallel()

	// Calling Splitmix64 with its own output should produce unique values.
	seen := make(map[uint64]bool)
	state := uint64(BaseSeed)
	iterations := 100

	for range iterations {
		state = Splitmix64(state)
		if seen[state] {
			t.Fatalf("Splitmix64 cycle detected at value %x", state)
		}

		seen[state] = true
	}
}

func TestMixHash_Deterministic(t *testing.T) {
	t.Parallel()

	base := uint64(0x1234)
	seed := uint64(0x5678)

	result1 := MixHash(base, seed)
	result2 := MixHash(base, seed)

	if result1 != result2 {
		t.Errorf("MixHash not deterministic: %x != %x", result1, result2)
	}
}

func TestMixHash_SeedVariation(t *testing.T) {
	t.Parallel()

	base := uint64(0xDEADBEEF)
	r1 := MixHash(base, 1)
	r2 := MixHash(base, 2)

	if r1 == r2 {
		t.Error("MixHash produced same result for different seeds")
	}
}

func TestMixHash_Symmetric(t *testing.T) {
	t.Parallel()

	// XOR is commutative, so MixHash(a, b) == MixHash(b, a).
	a := uint64(0x1111)
	b := uint64(0x2222)

	if MixHash(a, b) != MixHash(b, a) {
		t.Error("MixHash should be symmetric (XOR-based)")
	}
}

func TestFNV64a_Deterministic(t *testing.T) {
	t.Parallel()

	data := []byte("hello world")
	r1 := FNV64a(data)
	r2 := FNV64a(data)

	if r1 != r2 {
		t.Errorf("FNV64a not deterministic: %x != %x", r1, r2)
	}
}

func TestFNV64a_DifferentInputs(t *testing.T) {
	t.Parallel()

	r1 := FNV64a([]byte("hello"))
	r2 := FNV64a([]byte("world"))

	if r1 == r2 {
		t.Error("FNV64a produced same hash for different inputs")
	}
}

func TestFNV64a_Empty(t *testing.T) {
	t.Parallel()

	// FNV-1a of empty data should be the offset basis.
	result := FNV64a([]byte{})
	if result == 0 {
		t.Error("FNV64a of empty data should not be 0")
	}
}

func TestGenerateSeeds_Count(t *testing.T) {
	t.Parallel()

	seeds := GenerateSeeds(5, Mix64)
	if len(seeds) != 5 {
		t.Errorf("expected 5 seeds, got %d", len(seeds))
	}
}

func TestGenerateSeeds_Uniqueness(t *testing.T) {
	t.Parallel()

	seeds := GenerateSeeds(100, Mix64)
	seen := make(map[uint64]bool, len(seeds))

	for i, s := range seeds {
		if seen[s] {
			t.Fatalf("duplicate seed at index %d: %x", i, s)
		}

		seen[s] = true
	}
}

func TestGenerateSeeds_DeterministicWithMix64(t *testing.T) {
	t.Parallel()

	s1 := GenerateSeeds(10, Mix64)
	s2 := GenerateSeeds(10, Mix64)

	for i := range s1 {
		if s1[i] != s2[i] {
			t.Errorf("GenerateSeeds(Mix64) not deterministic at index %d", i)
		}
	}
}

func TestGenerateSeeds_DeterministicWithSplitmix64(t *testing.T) {
	t.Parallel()

	s1 := GenerateSeeds(10, Splitmix64)
	s2 := GenerateSeeds(10, Splitmix64)

	for i := range s1 {
		if s1[i] != s2[i] {
			t.Errorf("GenerateSeeds(Splitmix64) not deterministic at index %d", i)
		}
	}
}

func TestGenerateSeeds_DifferentAdvanceFunctions(t *testing.T) {
	t.Parallel()

	// Mix64 and Splitmix64 should produce different seed sequences.
	sMix := GenerateSeeds(5, Mix64)
	sSplit := GenerateSeeds(5, Splitmix64)

	allSame := true

	for i := range sMix {
		if sMix[i] != sSplit[i] {
			allSame = false

			break
		}
	}

	if allSame {
		t.Error("GenerateSeeds with Mix64 and Splitmix64 produced identical sequences")
	}
}

func TestGenerateSeeds_Zero(t *testing.T) {
	t.Parallel()

	seeds := GenerateSeeds(0, Mix64)
	if len(seeds) != 0 {
		t.Errorf("expected 0 seeds, got %d", len(seeds))
	}
}

func TestConstants(t *testing.T) {
	t.Parallel()

	// Verify the constants match the well-known splitmix64 values.
	if BaseSeed != 0x517cc1b727220a95 {
		t.Errorf("BaseSeed mismatch: %x", BaseSeed)
	}

	if MixShift1 != 30 {
		t.Errorf("MixShift1 mismatch: %d", MixShift1)
	}

	if MixMul1 != 0xbf58476d1ce4e5b9 {
		t.Error("MixMul1 mismatch")
	}

	if MixShift2 != 27 {
		t.Errorf("MixShift2 mismatch: %d", MixShift2)
	}

	if MixMul2 != 0x94d049bb133111eb {
		t.Error("MixMul2 mismatch")
	}

	if MixShift3 != 31 {
		t.Errorf("MixShift3 mismatch: %d", MixShift3)
	}
}

func BenchmarkMix64(b *testing.B) {
	var v uint64 = 0xDEADBEEFCAFEBABE

	for range b.N {
		v = Mix64(v)
	}
}

func BenchmarkSplitmix64(b *testing.B) {
	var v uint64 = 0xDEADBEEFCAFEBABE

	for range b.N {
		v = Splitmix64(v)
	}
}

func BenchmarkMixHash(b *testing.B) {
	base := uint64(0xDEADBEEFCAFEBABE)
	seed := uint64(0x1234567890ABCDEF)

	for range b.N {
		_ = MixHash(base, seed)
	}
}

func BenchmarkFNV64a(b *testing.B) {
	data := []byte("benchmark test data for FNV-1a hashing")

	for range b.N {
		_ = FNV64a(data)
	}
}

func BenchmarkGenerateSeeds(b *testing.B) {
	for range b.N {
		_ = GenerateSeeds(128, Mix64)
	}
}
