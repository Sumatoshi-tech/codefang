package main

type Stack[T any] struct{ items []T }

func (s *Stack[T]) Push(v T) {
	if v != nil {
		s.items = append(s.items, v)
	}
}

func Map[T any, U any](xs []T, f func(T) U) []U {
	out := make([]U, 0, len(xs))
	for _, x := range xs {
		out = append(out, f(x))
	}
	return out
}

var pkgClosure = func(x int) int {
	if x > 0 {
		return 1
	}
	return 0
}

func outer() {
	defer func() {
		if r := recover(); r != nil {
			panic(r)
		}
	}()
	go func() {
		for {
		}
	}()
}
