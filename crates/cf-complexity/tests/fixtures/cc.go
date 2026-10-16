package main

// cyclomatic 8, cognitive 7, nesting 1
func decisions(a, b int, xs []int) int {
	r := 0
	if a > 0 && b > 0 {
		r++
	} else if a < 0 || b < 0 {
		r--
	} else {
		r = 2
	}
	for _, x := range xs {
		r += x
	}
	switch r {
	case 1:
		r = 10
	case 2:
		r = 20
	default:
		r = 0
	}
	return r
}

// cyclomatic 4, cognitive 6, nesting 3
func nested(a, b, c bool) int {
	if a {
		if b {
			if c {
				return 1
			}
		}
	}
	return 0
}

// cyclomatic 3, cognitive 4 (if+1, else+1, inner if nested => +2), nesting 2
func elseNest(a, b bool) int {
	if a {
		return 1
	} else {
		if b {
			return 2
		}
	}
	return 0
}

// cyclomatic 4 (for, &&, ||), cognitive 3 (for +1, && seq +1, || change +1)
func loopCond(a, b bool, n int) int {
	r := 0
	for i := 0; i < n && a || b; i++ {
		r++
	}
	return r
}

// cyclomatic 2, cognitive 2 (if +1, recursion +1)
func fact(n int) int {
	if n <= 1 {
		return 1
	}
	return n * fact(n-1)
}

// cyclomatic 3 (if in closure + for), cognitive 3 (if in lambda @nesting1 => +2, for +1)
func withClosure(xs []int) int {
	total := 0
	f := func(x int) {
		if x > 0 {
			total += x
		}
	}
	for _, x := range xs {
		f(x)
	}
	return total
}

// cyclomatic 4 (&&,&&,||), cognitive 2 (&& seq +1, || change +1)
func boolSeq(a, b, c, d bool) bool {
	return a && b && c || d
}

// cyclomatic 5, cognitive 1, nesting 1
func manyCases(n int) string {
	switch n {
	case 1:
		return "a"
	case 2:
		return "b"
	case 3:
		return "c"
	case 4:
		return "d"
	default:
		return "z"
	}
}

// cyclomatic 1, cognitive 0, nesting 0
func simple() int {
	return 1
}

type T struct{}

// method: cyclomatic 2, cognitive 1
func (t *T) Method(a bool) int {
	if a {
		return 1
	}
	return 0
}
