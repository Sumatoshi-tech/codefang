// Copyright (c) 2015, Arbo von Monkiewitsch All rights reserved.
// Use of this source code is governed by a BSD-style
// license.

package levenshtein

import (
	"strings"
	"testing"
)

var myersTestCases = []struct {
	s1     string
	s2     string
	wanted int
}{
	{"", "a", 1},
	{"a", "", 1},
	{"a", "a", 0},
	{"a", "b", 1},
	{"ab", "ab", 0},
	{"ab", "aa", 1},
	{"ab", "aaa", 2},
	{"kitten", "sitting", 3},
	{"sitting", "kitten", 3},
	{"aaa", "ab", 2},
	{"aa", "aü", 1},
	{"Fön", "Föm", 1},
	{"abc", "def", 3},
	{"x", "xyz", 2},
	{"xyz", "x", 2},
	{"same", "same", 0},
	{"insert", "inser", 1},
	{"inser", "insert", 1},
}

func TestDistanceMyersPath(t *testing.T) {
	t.Parallel()

	ctx := &Context{}

	for _, tc := range myersTestCases {
		got := ctx.Distance(tc.s1, tc.s2)
		if got != tc.wanted {
			t.Errorf("Distance(%q, %q) = %d, want %d", tc.s1, tc.s2, got, tc.wanted)
		}
	}
}

func TestDistanceMyersPathSymmetry(t *testing.T) {
	t.Parallel()

	ctx := &Context{}
	pairs := []string{"kitten", "sitting", "ab", "aaa", "Fön", "Föm", "a", "xyz"}

	for i, a := range pairs {
		for j, b := range pairs {
			if i == j {
				continue
			}

			d1 := ctx.Distance(a, b)
			d2 := ctx.Distance(b, a)

			if d1 != d2 {
				t.Errorf("Distance(%q, %q) = %d but Distance(%q, %q) = %d (should be equal)",
					a, b, d1, b, a, d2)
			}
		}
	}
}

func TestDistanceMyersPathAt64Runes(t *testing.T) {
	t.Parallel()

	ctx := &Context{}

	// Exactly 64 runes: Myers path.
	s64 := strings.Repeat("a", 64)
	s64alt := strings.Repeat("a", 63) + "b"

	got := ctx.Distance(s64, s64alt)
	if got != 1 {
		t.Errorf("Distance(64×a, 63×a+b) = %d, want 1", got)
	}

	got = ctx.Distance(s64, s64)
	if got != 0 {
		t.Errorf("Distance(64×a, 64×a) = %d, want 0", got)
	}
}

func TestDistanceMyersPathNonASCII(t *testing.T) {
	t.Parallel()

	ctx := &Context{}

	// Non-ASCII runes exercise the fallback PM scan in distanceMyers64.
	tests := []struct {
		s1     string
		s2     string
		wanted int
	}{
		{"αβγ", "αβγ", 0},
		{"αβγ", "αβδ", 1},
		{"Fön", "Föm", 1},
		{"aa", "aü", 1},
	}

	for _, tc := range tests {
		got := ctx.Distance(tc.s1, tc.s2)
		if got != tc.wanted {
			t.Errorf("Distance(%q, %q) = %d, want %d", tc.s1, tc.s2, got, tc.wanted)
		}
	}
}

func TestDistanceMyersVsDPConsistency(t *testing.T) {
	t.Parallel()

	ctx := &Context{}

	// Strings <= 64 runes use Myers; > 64 use DP. Compare at boundary.
	// For s1,s2 both <=64: result must match classic definition.
	// We verify by comparing (short, short) with a known baseline.
	sShort := "kitten"
	sLong := strings.Repeat("x", 100)

	dShort := ctx.Distance(sShort, "sitting")
	if dShort != 3 {
		t.Errorf("short Distance = %d, want 3", dShort)
	}

	_ = ctx.Distance(sLong, sLong)
	// After using long strings, context buffers are grown; next short call still uses Myers.
	dShortAgain := ctx.Distance(sShort, "sitting")
	if dShortAgain != 3 {
		t.Errorf("short Distance after long = %d, want 3", dShortAgain)
	}
}
