package common

import "github.com/Sumatoshi-tech/codefang/internal/streaming"

// Compile-time assertion: NoStateHibernation satisfies [streaming.Hibernatable].
var _ streaming.Hibernatable = NoStateHibernation{}

// NoStateHibernation is an embeddable zero-size mixin that provides no-op
// implementations of [streaming.Hibernatable] for analyzers that accumulate
// no working state between streaming chunks.
type NoStateHibernation struct{}

// Hibernate is a no-op. Returns nil.
func (NoStateHibernation) Hibernate() error { return nil }

// Boot is a no-op. Returns nil.
func (NoStateHibernation) Boot() error { return nil }
