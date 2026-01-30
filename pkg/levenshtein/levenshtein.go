// Copyright (c) 2015, Arbo von Monkiewitsch All rights reserved.
// Use of this source code is governed by a BSD-style
// license.

// Package levenshtein calculates the Levenshtein edit distance between strings.
package levenshtein

import (
	"unicode/utf8"
)

// Context is the object which allows to calculate the Levenshtein distance
// with Distance() method. It is needed to ensure 0 memory allocations.
type Context struct {
	intSlice []int
	runeBuf1 []rune
	runeBuf2 []rune
	peq      [256]uint64
}

// maxMyersLen is the maximum length of the first string for using the
// bit-parallel Myers algorithm (limited by 64-bit word size).
const maxMyersLen = 64

func (ctx *Context) getIntSlice(length int) []int {
	if cap(ctx.intSlice) < length {
		ctx.intSlice = make([]int, length)
	}

	return ctx.intSlice[:length]
}

func (ctx *Context) chars1(s string) []rune {
	ctx.runeBuf1 = ctx.runeBuf1[:0]
	for _, r := range s {
		ctx.runeBuf1 = append(ctx.runeBuf1, r)
	}

	return ctx.runeBuf1
}

func (ctx *Context) chars2(s string) []rune {
	ctx.runeBuf2 = ctx.runeBuf2[:0]
	for _, r := range s {
		ctx.runeBuf2 = append(ctx.runeBuf2, r)
	}

	return ctx.runeBuf2
}

// Distance calculates the Levenshtein distance between two strings.
// It uses a bit-parallel algorithm (Myers) for strings up to 64 runes,
// providing significant performance speedup (SIMD-within-a-register).
// For longer strings, it falls back to the standard optimized DP algorithm.
func (ctx *Context) Distance(str1, str2 string) int {
	// Optimization: check simple equality/empty cases first?
	if str1 == str2 {
		return 0
	}

	if str1 == "" {
		return utf8.RuneCountInString(str2)
	}

	if str2 == "" {
		return utf8.RuneCountInString(str1)
	}

	// We need runes for the calculation.
	// Note: Avoiding full allocation by using Context buffers is still relevant
	// for the fallback, but Myers also needs rune access.
	// Since Myers is 64-bit, we can just read runes on the fly if we want 0 allocations?
	// But `distanceMyers64` as written takes `[]rune`.
	// Let's use the context buffers to get `[]rune` cheaply.

	s1 := ctx.chars1(str1)
	s2 := ctx.chars2(str2)

	// If s1 fits in 64 bits, use Myers.
	if len(s1) <= maxMyersLen {
		return ctx.distanceMyers64(s1, s2)
	}

	// Myers algorithm is asymmetric. If s2 fits, we can swap.
	// Distance(s1, s2) == Distance(s2, s1).
	if len(s2) <= maxMyersLen {
		return ctx.distanceMyers64(s2, s1)
	}

	// Fallback to standard DP logic (which we already optimized).
	lenS1 := len(s1)
	lenS2 := len(s2)

	column := ctx.getIntSlice(lenS1 + 1)
	// Column[0] will be initialized at the start of the first loop before it
	// is read, unless lenS2 is zero, which we deal with above.
	for idx := 1; idx <= lenS1; idx++ {
		column[idx] = idx
	}

	for col := range lenS2 {
		s2Rune := s2[col]
		column[0] = col + 1
		lastdiag := col

		for row := range lenS1 {
			olddiag := column[row+1]

			cost := 0
			if s1[row] != s2Rune {
				cost = 1
			}

			column[row+1] = min(
				column[row+1]+1,
				column[row]+1,
				lastdiag+cost,
			)
			lastdiag = olddiag
		}
	}

	return column[lenS1]
}
