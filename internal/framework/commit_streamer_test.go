package framework_test

import (
	"context"
	"testing"

	"github.com/Sumatoshi-tech/codefang/internal/framework"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestCommitStreamer_NewCommitStreamer(t *testing.T) {
	t.Parallel()

	streamer := framework.NewCommitStreamer()
	if streamer.BatchSize != 10 {
		t.Errorf("BatchSize = %d, want 10", streamer.BatchSize)
	}

	if streamer.Lookahead != 2 {
		t.Errorf("Lookahead = %d, want 2", streamer.Lookahead)
	}
}

func TestCommitStreamer_StreamEmpty(t *testing.T) {
	t.Parallel()

	streamer := framework.NewCommitStreamer()
	ctx := context.Background()
	ch := streamer.Stream(ctx, []*gitlib.Commit{})

	n := 0
	for range ch {
		n++
	}

	if n != 0 {
		t.Errorf("got %d batches, want 0", n)
	}
}

func TestCommitStreamer_StreamBatches(t *testing.T) {
	t.Parallel()

	repo := framework.NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("a.txt", "a")
	repo.Commit("first")
	repo.CreateFile("b.txt", "b")
	repo.Commit("second")
	repo.CreateFile("c.txt", "c")
	repo.Commit("third")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	if err != nil {
		t.Fatalf("OpenRepository: %v", err)
	}
	defer libRepo.Free()

	commits := framework.CollectCommits(t, libRepo, 3)
	if len(commits) < 3 {
		t.Fatalf("got %d commits, want at least 3", len(commits))
	}

	commits = commits[:3]

	streamer := &framework.CommitStreamer{BatchSize: 2, Lookahead: 1}
	ctx := context.Background()
	ch := streamer.Stream(ctx, commits)

	batches := make([]framework.CommitBatch, 0, 8)
	for b := range ch {
		batches = append(batches, b)
	}

	if len(batches) != 2 {
		t.Fatalf("got %d batches, want 2", len(batches))
	}

	if len(batches[0].Commits) != 2 {
		t.Errorf("batch 0: got %d commits, want 2", len(batches[0].Commits))
	}

	if len(batches[1].Commits) != 1 {
		t.Errorf("batch 1: got %d commits, want 1", len(batches[1].Commits))
	}

	if batches[0].StartIndex != 0 || batches[1].StartIndex != 2 {
		t.Errorf("StartIndex: got %d, %d; want 0, 2", batches[0].StartIndex, batches[1].StartIndex)
	}
}

func TestCommitStreamer_StreamFromIterator(t *testing.T) {
	t.Parallel()

	repo := framework.NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("a.txt", "a")
	repo.Commit("first")
	repo.CreateFile("b.txt", "b")
	repo.Commit("second")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	if err != nil {
		t.Fatalf("OpenRepository: %v", err)
	}
	defer libRepo.Free()

	iter, err := libRepo.Log(&gitlib.LogOptions{})
	if err != nil {
		t.Fatalf("Log: %v", err)
	}

	streamer := &framework.CommitStreamer{BatchSize: 1, Lookahead: 1}
	ctx := context.Background()
	ch := streamer.StreamFromIterator(ctx, iter, 2)

	batches := make([]framework.CommitBatch, 0, 8)
	for b := range ch {
		batches = append(batches, b)
	}

	if len(batches) != 2 {
		t.Errorf("got %d batches, want 2", len(batches))
	}

	if len(batches) >= 1 && len(batches[0].Commits) != 1 {
		t.Errorf("batch 0: got %d commits, want 1", len(batches[0].Commits))
	}
}

func TestCommitStreamer_StreamSingle(t *testing.T) {
	t.Parallel()

	repo := framework.NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("x.txt", "x")
	repo.Commit("only")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	if err != nil {
		t.Fatalf("OpenRepository: %v", err)
	}
	defer libRepo.Free()

	commits := framework.CollectCommits(t, libRepo, 1)
	if len(commits) != 1 {
		t.Fatalf("got %d commits, want 1", len(commits))
	}

	streamer := framework.NewCommitStreamer()
	ctx := context.Background()
	ch := streamer.StreamSingle(ctx, commits)

	batches := make([]framework.CommitBatch, 0, 4)
	for b := range ch {
		batches = append(batches, b)
	}

	if len(batches) != 1 {
		t.Errorf("got %d batches, want 1", len(batches))
	}

	if len(batches[0].Commits) != 1 || batches[0].StartIndex != 0 || batches[0].BatchID != 0 {
		t.Errorf("batch: Commits=%d StartIndex=%d BatchID=%d", len(batches[0].Commits), batches[0].StartIndex, batches[0].BatchID)
	}
}

func TestCommitStreamer_StreamContextCancel(t *testing.T) {
	t.Parallel()

	repo := framework.NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("x.txt", "x")
	repo.Commit("only")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	if err != nil {
		t.Fatalf("OpenRepository: %v", err)
	}
	defer libRepo.Free()

	commits := framework.CollectCommits(t, libRepo, 1)
	if len(commits) != 1 {
		t.Fatalf("got %d commits, want 1", len(commits))
	}

	ctx, cancel := context.WithCancel(context.Background())
	streamer := framework.NewCommitStreamer()
	ch := streamer.Stream(ctx, commits)

	cancel()

	n := 0
	for range ch {
		n++
	}

	if n > 1 {
		t.Errorf("after cancel got %d batches", n)
	}
}
