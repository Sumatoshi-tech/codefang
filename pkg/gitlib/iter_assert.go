package gitlib

import "github.com/Sumatoshi-tech/codefang/pkg/alg"

// Compile-time interface assertions for [alg.Iterator].
var (
	_ alg.Iterator[*Commit] = (*CommitIter)(nil)
	_ alg.Iterator[*File]   = (*FileIter)(nil)
	_ alg.Iterator[Hash]    = (*RevWalk)(nil)
)
