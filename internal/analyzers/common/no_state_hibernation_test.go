package common_test

// FRD: specs/frds/FRD-20260302-no-state-hibernation.md.

import (
	"testing"
	"unsafe"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common"
	"github.com/Sumatoshi-tech/codefang/internal/streaming"
)

func TestNoStateHibernation_Hibernate_ReturnsNil(t *testing.T) {
	t.Parallel()

	var h common.NoStateHibernation

	err := h.Hibernate()
	require.NoError(t, err)
}

func TestNoStateHibernation_Boot_ReturnsNil(t *testing.T) {
	t.Parallel()

	var h common.NoStateHibernation

	err := h.Boot()
	require.NoError(t, err)
}

func TestNoStateHibernation_SatisfiesHibernatable(t *testing.T) {
	t.Parallel()

	var h common.NoStateHibernation

	var _ streaming.Hibernatable = h

	// Verify the interface contract works end-to-end.
	var iface streaming.Hibernatable = h
	assert.NoError(t, iface.Hibernate())
	assert.NoError(t, iface.Boot())
}

func TestNoStateHibernation_ZeroSize(t *testing.T) {
	t.Parallel()

	var h common.NoStateHibernation
	assert.Equal(t, uintptr(0), unsafe.Sizeof(h))
}
