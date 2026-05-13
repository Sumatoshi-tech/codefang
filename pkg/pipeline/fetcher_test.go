package pipeline_test

import (
	"context"
	"errors"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

var (
	errUnknownRequest = errors.New("unknown request")
	errFetch          = errors.New("fetch failed")
)

func TestFetcherFunc_Success(t *testing.T) {
	t.Parallel()

	fetcher := pipeline.FetcherFunc[int, string](func(_ context.Context, req int) (string, error) {
		if req == 1 {
			return "one", nil
		}

		return "", errUnknownRequest
	})

	result, err := fetcher.Fetch(context.Background(), 1)

	require.NoError(t, err)
	assert.Equal(t, "one", result)
}

func TestFetcherFunc_Error(t *testing.T) {
	t.Parallel()

	fetcher := pipeline.FetcherFunc[string, int](func(_ context.Context, _ string) (int, error) {
		return 0, errFetch
	})

	_, err := fetcher.Fetch(context.Background(), "anything")
	require.ErrorIs(t, err, errFetch)
}

func TestFetcherFunc_SatisfiesInterface(t *testing.T) {
	t.Parallel()

	var f pipeline.Fetcher[int, int] = pipeline.FetcherFunc[int, int](
		func(_ context.Context, req int) (int, error) {
			return req * 2, nil
		},
	)

	result, err := f.Fetch(context.Background(), 5)

	require.NoError(t, err)
	assert.Equal(t, 10, result)
}
