// FRD: specs/frds/FRD-20260419-pathspec-builder.md.
package langpath_test

import (
	"slices"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/plumbing/langpath"
)

func TestGlobs_AllToken_YieldsWantsAll(t *testing.T) {
	t.Parallel()

	globs, wantsAll, err := langpath.Globs([]string{"all"})

	require.NoError(t, err)
	assert.True(t, wantsAll, "all token must set wantsAll")
	assert.Nil(t, globs, "wantsAll must return nil globs")
}

func TestGlobs_ReturnsFreshSlicePerCall(t *testing.T) {
	t.Parallel()

	a, _, errA := langpath.Globs([]string{"go"})
	require.NoError(t, errA)
	require.NotEmpty(t, a)

	b, _, errB := langpath.Globs([]string{"go"})
	require.NoError(t, errB)
	require.NotEmpty(t, b)

	const tampered = "tampered"

	a[0] = tampered
	assert.NotEqual(t, tampered, b[0],
		"mutating one call's result must not affect a subsequent call's result")
}

func TestGlobs_Dockerfile_IncludesBasenameGlob(t *testing.T) {
	t.Parallel()

	globs, wantsAll, err := langpath.Globs([]string{"dockerfile"})

	require.NoError(t, err)
	assert.False(t, wantsAll)
	assert.Contains(t, globs, "Dockerfile",
		"filename-only languages must emit a literal-filename glob")
}

func TestGlobs_MultipleLanguages_SortedAndDeduplicated(t *testing.T) {
	t.Parallel()

	globs, wantsAll, err := langpath.Globs([]string{"python", "go", "python"})

	require.NoError(t, err)
	assert.False(t, wantsAll)
	assert.NotEmpty(t, globs)
	assert.True(t, slices.IsSorted(globs), "globs must be sorted")
	assert.Contains(t, globs, "*.go", "go extension must be present")
	assert.Contains(t, globs, "*.py", "python extension must be present")
	assert.Len(t, mapset(globs), len(globs), "globs must be deduplicated")
}

func mapset(xs []string) map[string]struct{} {
	m := make(map[string]struct{}, len(xs))
	for _, x := range xs {
		m[x] = struct{}{}
	}

	return m
}

func TestGlobs_UnknownToken_ReturnsErrUnknownLanguage(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		in   []string
	}{
		{"solo", []string{"notalang"}},
		{"after known", []string{"go", "notalang"}},
		{"before known", []string{"notalang", "go"}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			globs, wantsAll, err := langpath.Globs(tt.in)

			require.ErrorIs(t, err, langpath.ErrUnknownLanguage)
			assert.False(t, wantsAll)
			assert.Nil(t, globs)
			assert.Contains(t, err.Error(), "notalang")
		})
	}
}

func TestGlobs_GoToken_YieldsStarDotGo(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		in   string
	}{
		{"lowercase", "go"},
		{"titlecase", "Go"},
		{"uppercase", "GO"},
		{"padded", "  go  "},
		{"alias golang", "golang"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			globs, wantsAll, err := langpath.Globs([]string{tt.in})

			require.NoError(t, err)
			assert.False(t, wantsAll)
			assert.Equal(t, []string{"*.go"}, globs)
		})
	}
}

func TestGlobs_EmptyInput_YieldsWantsAll(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		in   []string
	}{
		{"nil slice", nil},
		{"empty slice", []string{}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			globs, wantsAll, err := langpath.Globs(tt.in)

			require.NoError(t, err)
			assert.True(t, wantsAll)
			assert.Nil(t, globs)
		})
	}
}
