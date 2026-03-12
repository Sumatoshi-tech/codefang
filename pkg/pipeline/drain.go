package pipeline

// SignalOnDrain forwards items from src to the returned forwarded channel
// and closes the returned drained channel once src is exhausted.
// This enables ending pipeline stage spans independently.
func SignalOnDrain[T any](src <-chan T) (forwarded <-chan T, drained <-chan struct{}) {
	sig := make(chan struct{})
	out := make(chan T)

	go func() {
		defer close(sig)
		defer close(out)

		for item := range src {
			out <- item
		}
	}()

	return out, sig
}
