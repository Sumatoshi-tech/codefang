package analyze

import (
	"errors"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// FRD: specs/frds/FRD-20260302-compute-metrics-safe.md.

// testMetrics is a minimal metrics type for testing SafeMetricComputer.
type testMetrics struct {
	Value int
}

// errTestCompute is a sentinel error for testing error propagation.
var errTestCompute = errors.New("compute failed")

// computeResultValue is the expected value from the test compute function.
const computeResultValue = 42

func TestSafeMetricComputer_EmptyReport(t *testing.T) {
	t.Parallel()

	empty := &testMetrics{}
	compute := func(_ Report) (*testMetrics, error) {
		t.Fatal("compute must not be called on empty report")

		return nil, errTestCompute
	}

	wrapped := SafeMetricComputer(compute, empty)
	got, err := wrapped(Report{})

	require.NoError(t, err)
	assert.Equal(t, empty, got)
}

func TestSafeMetricComputer_NilReport(t *testing.T) {
	t.Parallel()

	empty := &testMetrics{}
	compute := func(_ Report) (*testMetrics, error) {
		t.Fatal("compute must not be called on nil report")

		return nil, errTestCompute
	}

	wrapped := SafeMetricComputer(compute, empty)
	got, err := wrapped(nil)

	require.NoError(t, err)
	assert.Equal(t, empty, got)
}

func TestSafeMetricComputer_NonEmptyReport(t *testing.T) {
	t.Parallel()

	empty := &testMetrics{}
	expected := &testMetrics{Value: computeResultValue}

	compute := func(_ Report) (*testMetrics, error) {
		return expected, nil
	}

	wrapped := SafeMetricComputer(compute, empty)
	got, err := wrapped(Report{"key": "value"})

	require.NoError(t, err)
	assert.Equal(t, expected, got)
}

func TestSafeMetricComputer_ErrorPropagation(t *testing.T) {
	t.Parallel()

	empty := &testMetrics{}
	compute := func(_ Report) (*testMetrics, error) {
		return nil, errTestCompute
	}

	wrapped := SafeMetricComputer(compute, empty)
	got, err := wrapped(Report{"key": "value"})

	require.ErrorIs(t, err, errTestCompute)
	assert.Nil(t, got)
}
