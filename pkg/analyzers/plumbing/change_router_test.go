package plumbing

import (
	"errors"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestChangeRouter_Route(t *testing.T) {
	t.Parallel()

	t.Run("success paths", func(t *testing.T) {
		t.Parallel()

		var inserts, deletes, modifies, renames int

		router := &ChangeRouter{
			OnInsert: func(_ *gitlib.Change) error {
				inserts++

				return nil
			},
			OnDelete: func(_ *gitlib.Change) error {
				deletes++

				return nil
			},
			OnModify: func(_ *gitlib.Change) error {
				modifies++

				return nil
			},
			OnRename: func(from, to string, _ *gitlib.Change) error {
				assert.Equal(t, "old.go", from)
				assert.Equal(t, "new.go", to)

				renames++

				return nil
			},
		}

		changes := gitlib.Changes{
			{Action: gitlib.Insert},
			{Action: gitlib.Delete},
			{Action: gitlib.Modify, From: gitlib.ChangeEntry{Name: "same.go"}, To: gitlib.ChangeEntry{Name: "same.go"}},
			{Action: gitlib.Modify, From: gitlib.ChangeEntry{Name: "old.go"}, To: gitlib.ChangeEntry{Name: "new.go"}},
		}

		err := router.Route(changes)
		require.NoError(t, err)

		assert.Equal(t, 1, inserts)
		assert.Equal(t, 1, deletes)
		assert.Equal(t, 1, modifies)
		assert.Equal(t, 1, renames)
	})

	t.Run("nil handlers", func(t *testing.T) {
		t.Parallel()

		router := &ChangeRouter{}
		changes := gitlib.Changes{
			{Action: gitlib.Insert},
			{Action: gitlib.Delete},
			{Action: gitlib.Modify, From: gitlib.ChangeEntry{Name: "same.go"}, To: gitlib.ChangeEntry{Name: "same.go"}},
			{Action: gitlib.Modify, From: gitlib.ChangeEntry{Name: "old.go"}, To: gitlib.ChangeEntry{Name: "new.go"}},
		}

		err := router.Route(changes)
		require.NoError(t, err)
	})

	t.Run("error propagation", func(t *testing.T) {
		t.Parallel()

		testErr := errors.New("test error") //nolint:err113 // Intended dynamic error for testing.
		router := &ChangeRouter{
			OnInsert: func(_ *gitlib.Change) error { return testErr },
		}

		changes := gitlib.Changes{
			{Action: gitlib.Insert},
		}

		err := router.Route(changes)
		assert.ErrorIs(t, err, testErr)
	})

	t.Run("rename error propagation", func(t *testing.T) {
		t.Parallel()

		testErr := errors.New("rename error") //nolint:err113 // Intended dynamic error for testing.
		router := &ChangeRouter{
			OnRename: func(_, _ string, _ *gitlib.Change) error { return testErr },
		}

		changes := gitlib.Changes{
			{Action: gitlib.Modify, From: gitlib.ChangeEntry{Name: "old.go"}, To: gitlib.ChangeEntry{Name: "new.go"}},
		}

		err := router.Route(changes)
		assert.ErrorIs(t, err, testErr)
	})
}
