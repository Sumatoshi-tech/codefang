package plumbing

import (
	"testing"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
)

func TestSnapshot_Clone(t *testing.T) {
	t.Parallel()

	h1 := gitlib.Hash{}
	b1 := &gitlib.CachedBlob{}
	f1 := pkgplumbing.FileDiffData{}
	l1 := pkgplumbing.LineStats{}
	c1 := gitlib.ChangeEntry{}
	u1 := uast.Change{}

	s := Snapshot{
		Tick:     42,
		AuthorID: 100,
		Changes:  gitlib.Changes{&gitlib.Change{}},
		BlobCache: map[gitlib.Hash]*gitlib.CachedBlob{
			h1: b1,
		},
		FileDiffs: map[string]pkgplumbing.FileDiffData{
			"a": f1,
		},
		LineStats: map[gitlib.ChangeEntry]pkgplumbing.LineStats{
			c1: l1,
		},
		Languages: map[gitlib.Hash]string{
			h1: "Go",
		},
		UASTChanges: []uast.Change{u1},
	}

	clone := s.Clone()

	assert.Equal(t, s.Tick, clone.Tick)
	assert.Equal(t, s.AuthorID, clone.AuthorID)

	assert.NotSame(t, &s.Changes, &clone.Changes)
	assert.Equal(t, s.Changes, clone.Changes)

	clone.Changes[0] = nil
	assert.NotNil(t, s.Changes[0])

	assert.Equal(t, s.BlobCache, clone.BlobCache)
	clone.BlobCache[h1] = nil
	assert.NotNil(t, s.BlobCache[h1])

	assert.Equal(t, s.FileDiffs, clone.FileDiffs)
	clone.FileDiffs["b"] = pkgplumbing.FileDiffData{}
	assert.Len(t, s.FileDiffs, 1)

	assert.Equal(t, s.LineStats, clone.LineStats)
	clone.LineStats[c1] = pkgplumbing.LineStats{Added: 1}
	assert.Equal(t, 0, s.LineStats[c1].Added)

	assert.Equal(t, s.Languages, clone.Languages)
	clone.Languages[h1] = "Python"
	assert.Equal(t, "Go", s.Languages[h1])

	assert.NotSame(t, &s.UASTChanges, &clone.UASTChanges)
	assert.Equal(t, s.UASTChanges, clone.UASTChanges)
	clone.UASTChanges[0].Change = &gitlib.Change{}
	assert.Nil(t, s.UASTChanges[0].Change)
}
