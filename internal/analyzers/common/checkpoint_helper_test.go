package common_test

// FRD: specs/frds/FRD-20260302-checkpoint-helper.md.

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common"
	"github.com/Sumatoshi-tech/codefang/pkg/persist"
)

// testCheckpointState is a simple state for testing.
type testCheckpointState struct {
	Name  string `json:"name"`
	Count int    `json:"count"`
}

const (
	testCheckpointBasename = "test_state"
	testCheckpointName     = "hello"
	testCheckpointCount    = 42
)

func TestCheckpointHelper_SaveLoad_JSON_RoundTrip(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	state := &testCheckpointState{Name: testCheckpointName, Count: testCheckpointCount}

	var restored testCheckpointState

	helper := common.NewCheckpointHelper[testCheckpointState](
		testCheckpointBasename,
		persist.NewJSONCodec(),
		func() *testCheckpointState { return state },
		func(s *testCheckpointState) { restored = *s },
	)

	err := helper.SaveCheckpoint(dir)
	require.NoError(t, err)

	err = helper.LoadCheckpoint(dir)
	require.NoError(t, err)

	assert.Equal(t, testCheckpointName, restored.Name)
	assert.Equal(t, testCheckpointCount, restored.Count)
}

func TestCheckpointHelper_SaveLoad_Gob_RoundTrip(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	state := &testCheckpointState{Name: testCheckpointName, Count: testCheckpointCount}

	var restored testCheckpointState

	helper := common.NewCheckpointHelper[testCheckpointState](
		testCheckpointBasename,
		persist.NewGobCodec(),
		func() *testCheckpointState { return state },
		func(s *testCheckpointState) { restored = *s },
	)

	err := helper.SaveCheckpoint(dir)
	require.NoError(t, err)

	err = helper.LoadCheckpoint(dir)
	require.NoError(t, err)

	assert.Equal(t, testCheckpointName, restored.Name)
	assert.Equal(t, testCheckpointCount, restored.Count)
}

func TestCheckpointHelper_Save_InvalidDir(t *testing.T) {
	t.Parallel()

	helper := common.NewCheckpointHelper[testCheckpointState](
		testCheckpointBasename,
		persist.NewJSONCodec(),
		func() *testCheckpointState { return &testCheckpointState{} },
		func(_ *testCheckpointState) {},
	)

	err := helper.SaveCheckpoint("/nonexistent/path/that/does/not/exist")
	require.Error(t, err)
}

func TestCheckpointHelper_Load_MissingFile(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	helper := common.NewCheckpointHelper[testCheckpointState](
		testCheckpointBasename,
		persist.NewJSONCodec(),
		func() *testCheckpointState { return &testCheckpointState{} },
		func(_ *testCheckpointState) {},
	)

	err := helper.LoadCheckpoint(dir)
	require.Error(t, err)
}

func TestCheckpointHelper_BuildCalledDuringSave(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	buildCalled := false

	helper := common.NewCheckpointHelper[testCheckpointState](
		testCheckpointBasename,
		persist.NewJSONCodec(),
		func() *testCheckpointState {
			buildCalled = true

			return &testCheckpointState{}
		},
		func(_ *testCheckpointState) {},
	)

	err := helper.SaveCheckpoint(dir)
	require.NoError(t, err)

	assert.True(t, buildCalled)
}

func TestCheckpointHelper_RestoreCalledDuringLoad(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	restoreCalled := false

	helper := common.NewCheckpointHelper[testCheckpointState](
		testCheckpointBasename,
		persist.NewJSONCodec(),
		func() *testCheckpointState { return &testCheckpointState{Name: testCheckpointName} },
		func(_ *testCheckpointState) { restoreCalled = true },
	)

	err := helper.SaveCheckpoint(dir)
	require.NoError(t, err)

	err = helper.LoadCheckpoint(dir)
	require.NoError(t, err)

	assert.True(t, restoreCalled)
}
