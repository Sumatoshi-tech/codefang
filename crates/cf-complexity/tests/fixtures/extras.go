package main

type Doer interface {
	Do(x int) int
}

type Fn func(int) int

func labeled(xs []int) int {
	r := 0
outer:
	for _, x := range xs {
		for y := 0; y < x; y++ {
			if y == 3 {
				continue outer
			}
			if y == 5 {
				break outer
			}
			if y == 7 {
				break
			}
		}
	}
	return r
}

func sel(a, b chan int) int {
	select {
	case v := <-a:
		return v
	case v := <-b:
		return v
	default:
		return 0
	}
}

func typeSwitch(v interface{}) int {
	switch t := v.(type) {
	case int:
		return t
	case string:
		return len(t)
	}
	return 0
}

func nestedClosure() func() int {
	return func() int {
		f := func() int {
			if true {
				return 1
			}
			return 0
		}
		return f()
	}
}

func paren(a, b, c bool) bool {
	return (a || b) && c
}

func chain(a, b, c int) int {
	if a > 0 {
		return 1
	} else if b > 0 {
		return 2
	} else if c > 0 {
		return 3
	} else {
		return 4
	}
}

func rec(n int) int {
	if n > 0 {
		return rec(n - 1)
	}
	return 0
}

type S struct{}

func (s *S) Rec(n int) int {
	if n > 0 {
		return s.Rec(n - 1)
	}
	return 0
}

// commented has a comment line and a blank line inside: loc counts neither.
func commented(a int) int {
	// explain

	return a /* trailing */
}

func asmStub(a, b int) int
