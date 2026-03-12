package pipeline

import "context"

// Fetcher retrieves a response for a given request. It serves as the
// base interface for the cache decorator pattern: wrap a Fetcher with
// "check cache → fetch misses → update cache" logic.
type Fetcher[Req, Resp any] interface {
	Fetch(ctx context.Context, req Req) (Resp, error)
}

// Compile-time interface satisfaction check.
var _ Fetcher[int, int] = FetcherFunc[int, int](nil)

// FetcherFunc adapts a plain function to the Fetcher interface.
type FetcherFunc[Req, Resp any] func(ctx context.Context, req Req) (Resp, error)

// Fetch calls the underlying function.
func (f FetcherFunc[Req, Resp]) Fetch(ctx context.Context, req Req) (Resp, error) {
	return f(ctx, req)
}
