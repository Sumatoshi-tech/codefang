// Copyright (c) 2015, Arbo von Monkiewitsch All rights reserved.
// Use of this source code is governed by a BSD-style
// license.

// Package levenshtein calculates the Levenshtein edit distance between strings.
package levenshtein

// Context is the object which allows to calculate the Levenshtein distance
// with Distance() method. It is needed to ensure 0 memory allocations.
type Context struct {
	intSlice []int
}

func (ctx *Context) getIntSlice(length int) []int {
	if cap(ctx.intSlice) < length {
		ctx.intSlice = make([]int, length)
	}

	return ctx.intSlice[:length]
}

// Distance calculates the Levenshtein distance between two strings which
// is defined as the minimum number of edits needed to transform one string
// into the other, with the allowable edit operations being insertion, deletion,
// or substitution of a single character.
// http://en.wikipedia.org/wiki/Levenshtein_distance
//
// This implementation is optimized to use O(min(m,n)) space.
// It is based on the optimized C version found here:
// http://en.wikibooks.org/wiki/Algorithm_implementation/Strings/Levenshtein_distance#C
func (ctx *Context) Distance(str1, str2 string) int {
	s1 := []rune(str1)
	s2 := []rune(str2)

	lenS1 := len(s1)
	lenS2 := len(s2)

	if lenS2 == 0 {
		return lenS1
	}

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
