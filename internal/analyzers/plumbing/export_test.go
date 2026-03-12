package plumbing

import "github.com/Sumatoshi-tech/codefang/pkg/gitlib"

// Repos returns the repos slice for testing.
func (b *BlobCacheAnalyzer) Repos() []*gitlib.Repository {
	return b.repos
}
