package checkpoint

import "github.com/Sumatoshi-tech/codefang/pkg/persist"

// Persister is an alias for [persist.Persister].
type Persister[T any] = persist.Persister[T]

// NewPersister creates a checkpoint persister with the given basename and codec.
func NewPersister[T any](basename string, codec Codec) *Persister[T] {
	return persist.NewPersister[T](basename, codec)
}
