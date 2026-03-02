package framework_test

import (
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/framework"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestWatchdog_Nil_WhenTimeoutDisabled(t *testing.T) {
	t.Parallel()

	watchdog := framework.NewWatchdog(framework.WatchdogConfig{
		Config: framework.CoordinatorConfig{WorkerTimeout: 0},
	})

	assert.Nil(t, watchdog)
}

func TestWatchdog_WaitForResponse_Success(t *testing.T) {
	t.Parallel()

	repo := framework.NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("x.txt", "x")
	repo.Commit("init")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	require.NoError(t, err)

	defer libRepo.Free()

	poolChan := make(chan gitlib.WorkerRequest, 1)
	worker := gitlib.NewWorker(libRepo, poolChan)
	worker.Start()

	watchdog := framework.NewWatchdog(framework.WatchdogConfig{
		RepoPath:     repo.Path(),
		Config:       framework.CoordinatorConfig{WorkerTimeout: 5 * time.Second},
		PoolRepos:    []*gitlib.Repository{libRepo},
		PoolWorkers:  []*gitlib.Worker{worker},
		PoolRequests: poolChan,
	})
	require.NotNil(t, watchdog)

	// Simulate a fast response.
	respChan := make(chan gitlib.BlobBatchResponse, 1)
	respChan <- gitlib.BlobBatchResponse{}

	_, ok := watchdog.WaitForResponse(respChan)
	assert.True(t, ok)
	assert.Zero(t, watchdog.StalledCount())

	close(poolChan)
	worker.Stop()
}

func TestWatchdog_WaitForResponse_Timeout(t *testing.T) {
	t.Parallel()

	repo := framework.NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("x.txt", "x")
	repo.Commit("init")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	require.NoError(t, err)

	defer libRepo.Free()

	poolChan := make(chan gitlib.WorkerRequest, 1)
	worker := gitlib.NewWorker(libRepo, poolChan)
	worker.Start()

	watchdog := framework.NewWatchdog(framework.WatchdogConfig{
		RepoPath:     repo.Path(),
		Config:       framework.CoordinatorConfig{WorkerTimeout: 50 * time.Millisecond},
		PoolRepos:    []*gitlib.Repository{libRepo},
		PoolWorkers:  []*gitlib.Worker{worker},
		PoolRequests: poolChan,
	})
	require.NotNil(t, watchdog)

	// Empty channel — no response will arrive.
	respChan := make(chan gitlib.BlobBatchResponse, 1)

	_, ok := watchdog.WaitForResponse(respChan)
	assert.False(t, ok)
	assert.Equal(t, 1, watchdog.StalledCount())

	close(poolChan)
	worker.Stop()
}

func TestWatchdog_WaitForTreeDiffResponse_Success(t *testing.T) {
	t.Parallel()

	watchdog := framework.NewWatchdog(framework.WatchdogConfig{
		RepoPath: t.TempDir(),
		Config:   framework.CoordinatorConfig{WorkerTimeout: 5 * time.Second},
	})
	require.NotNil(t, watchdog)

	respChan := make(chan gitlib.TreeDiffResponse, 1)
	respChan <- gitlib.TreeDiffResponse{}

	_, ok := watchdog.WaitForTreeDiffResponse(respChan)
	assert.True(t, ok)
	assert.Zero(t, watchdog.StalledCount())
}

func TestWatchdog_WaitForTreeDiffResponse_Timeout(t *testing.T) {
	t.Parallel()

	watchdog := framework.NewWatchdog(framework.WatchdogConfig{
		RepoPath: t.TempDir(),
		Config:   framework.CoordinatorConfig{WorkerTimeout: 50 * time.Millisecond},
	})
	require.NotNil(t, watchdog)

	respChan := make(chan gitlib.TreeDiffResponse, 1)

	_, ok := watchdog.WaitForTreeDiffResponse(respChan)
	assert.False(t, ok)
	assert.Equal(t, 1, watchdog.StalledCount())
}

func TestWatchdog_WaitForDiffResponse_Success(t *testing.T) {
	t.Parallel()

	watchdog := framework.NewWatchdog(framework.WatchdogConfig{
		RepoPath: t.TempDir(),
		Config:   framework.CoordinatorConfig{WorkerTimeout: 5 * time.Second},
	})
	require.NotNil(t, watchdog)

	respChan := make(chan gitlib.DiffBatchResponse, 1)
	respChan <- gitlib.DiffBatchResponse{}

	_, ok := watchdog.WaitForDiffResponse(respChan)
	assert.True(t, ok)
	assert.Zero(t, watchdog.StalledCount())
}

func TestWatchdog_WaitForDiffResponse_Timeout(t *testing.T) {
	t.Parallel()

	watchdog := framework.NewWatchdog(framework.WatchdogConfig{
		RepoPath: t.TempDir(),
		Config:   framework.CoordinatorConfig{WorkerTimeout: 50 * time.Millisecond},
	})
	require.NotNil(t, watchdog)

	respChan := make(chan gitlib.DiffBatchResponse, 1)

	_, ok := watchdog.WaitForDiffResponse(respChan)
	assert.False(t, ok)
	assert.Equal(t, 1, watchdog.StalledCount())
}

func TestBackoffDuration_Sequence(t *testing.T) {
	t.Parallel()

	assert.Equal(t, time.Duration(0), framework.BackoffDuration(0))
	assert.Equal(t, time.Second, framework.BackoffDuration(1))
	assert.Equal(t, 4*time.Second, framework.BackoffDuration(2))
}

func TestStallError_ContainsDetails(t *testing.T) {
	t.Parallel()

	err := framework.StallError("BlobBatchRequest", 3)
	require.ErrorIs(t, err, framework.ErrWorkerStalled)
	assert.Contains(t, err.Error(), "BlobBatchRequest")
	assert.Contains(t, err.Error(), "retries=3")
	assert.Contains(t, err.Error(), "git fsck")
}

func TestWatchdog_StalledCount_Increments(t *testing.T) {
	t.Parallel()

	watchdog := framework.NewWatchdog(framework.WatchdogConfig{
		RepoPath: t.TempDir(),
		Config:   framework.CoordinatorConfig{WorkerTimeout: 10 * time.Millisecond},
	})
	require.NotNil(t, watchdog)

	// Trigger 3 stalls in sequence.
	for range 3 {
		respChan := make(chan gitlib.BlobBatchResponse, 1)
		watchdog.WaitForResponse(respChan)
	}

	assert.Equal(t, 3, watchdog.StalledCount())
}
