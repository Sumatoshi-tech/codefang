package pipeline

import "context"

// DispatchFunc sends a request to a worker pool. The worker channel is
// captured in the closure, keeping the dispatch strategy decoupled from
// request semantics.
type DispatchFunc[Req any] func(ctx context.Context, req Req) error
