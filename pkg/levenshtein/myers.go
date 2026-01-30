package levenshtein

// distanceMyers64 calculates Levenshtein distance using Myers' bit-vector algorithm.
// This is optimized for strings where the first string is <= 64 characters.
// Reference: Hyyrö, H. (2001). "Explaining and extending the bit-parallel approximate string matching algorithm of Myers".
//
//nolint:gocognit // complexity is inherent to the bit-parallel algorithm logic.
func (ctx *Context) distanceMyers64(s1, s2 []rune) int {
	const asciiMax = 256

	len1 := len(s1)

	// Ensure s1 is the shorter string if possible/necessary, but the alg relies on s1 fitting in a word.
	// Since we dispatch based on len(s1) <= 64, this is fine.

	// Initialize Pattern Match bit-vectors (PEq) for ASCII.
	// We use the pre-allocated array in Context to avoid allocs.
	// To avoid clearing the entire 256-entry array (which is slow for short strings),
	// we only clear the entries we modify.
	// We rely on the invariant that ctx.peq is always all-zero upon entry.

	// Precompute PEq for s1.
	for i, r := range s1 {
		if r < asciiMax {
			ctx.peq[r] |= 1 << i
		}
	}

	// Clean up peq at the end (defer is too slow in hot path).
	// We must ensure this runs before any return.
	defer func() {
		for _, r := range s1 {
			if r < asciiMax {
				ctx.peq[r] = 0
			}
		}
	}()

	// VP and VN: Vertical Positive and Vertical Negative deltas.
	// Initial VP is all 1s (D[i,j] = i initially, so diff is +1).
	vp := ^uint64(0)
	vn := uint64(0)

	// Score is currently len1 (distance of s1 prefix to empty s2 prefix).
	score := len1

	// Mask has high bit set at len1-1.
	mask := uint64(1) << (len1 - 1)

	for _, char := range s2 {
		// PM (Pattern Match): 1 where s1[i] == char.
		var pm uint64
		if char < asciiMax {
			pm = ctx.peq[char]
		} else {
			// Fallback for non-ASCII: scan s1.
			for i, r := range s1 {
				if r == char {
					pm |= 1 << i
				}
			}
		}

		// Myers' step update logic:
		// D0 = (((PM & VP) + VP) ^ VP) | PM | VN
		// HP = VN | ~(D0 | VP)
		// HN = VP & D0.

		xVal := pm | vn
		d0 := ((vp + (xVal & vp)) ^ vp) | xVal
		hn := vp & d0
		hp := vn | ^(d0 | vp)

		xVal = (hp << 1) | 1
		vn = xVal & d0
		vp = (hn << 1) | ^(xVal | d0)

		if (hp & mask) != 0 {
			score++
		}

		if (hn & mask) != 0 {
			score--
		}
	}

	return score
}
