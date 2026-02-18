package levenshtein

const asciiMax = 256

// distanceMyers64 calculates Levenshtein distance using Myers' bit-vector algorithm.
// This is optimized for strings where the first string is <= 64 characters.
// Reference: Hyyrö, H. (2001). "Explaining and extending the bit-parallel
// approximate string matching algorithm of Myers".
func (ctx *Context) distanceMyers64(s1, s2 []rune) int {
	len1 := len(s1)

	ctx.initPeq(s1)
	defer ctx.clearPeq(s1)

	// VP and VN: Vertical Positive and Vertical Negative deltas.
	// Initial VP is all 1s (D[i,j] = i initially, so diff is +1).
	vp := ^uint64(0)
	vn := uint64(0)

	// Score is currently len1 (distance of s1 prefix to empty s2 prefix).
	score := len1

	// Mask has high bit set at len1-1.
	mask := uint64(1) << (len1 - 1)

	for _, char := range s2 {
		pm := ctx.patternMatch(s1, char)

		// Myers' step update:
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

// initPeq initializes the Pattern Match bit-vectors for s1.
func (ctx *Context) initPeq(s1 []rune) {
	for i, r := range s1 {
		if r < asciiMax {
			ctx.peq[r] |= 1 << i
		}
	}
}

// clearPeq resets the Pattern Match bit-vectors modified by s1.
func (ctx *Context) clearPeq(s1 []rune) {
	for _, r := range s1 {
		if r < asciiMax {
			ctx.peq[r] = 0
		}
	}
}

// patternMatch returns a bit-vector with 1 at positions where s1[i] == char.
func (ctx *Context) patternMatch(s1 []rune, char rune) uint64 {
	if char < asciiMax {
		return ctx.peq[char]
	}

	// Fallback for non-ASCII: scan s1.
	var pm uint64

	for i, r := range s1 {
		if r == char {
			pm |= 1 << i
		}
	}

	return pm
}
