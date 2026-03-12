package alg

// ForEachPair calls visit for every unique pair (i, j) where 0 <= i < j < n.
// The total number of calls is C(n, 2) = n*(n-1)/2. Does nothing when n < 2.
func ForEachPair(n int, visit func(i, j int)) {
	for i := range n {
		for j := i + 1; j < n; j++ {
			visit(i, j)
		}
	}
}
