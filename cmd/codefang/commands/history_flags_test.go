package commands_test

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/cmd/codefang/commands"
)

func TestHistoryCommand_ResourceKnobFlags(t *testing.T) {
	t.Parallel()

	cmd := commands.NewHistoryCommand()

	// Verify all resource knob flags are registered.
	flags := []string{
		"workers",
		"buffer-size",
		"commit-batch-size",
		"blob-cache-size",
		"diff-cache-size",
		"blob-arena-size",
	}

	for _, flagName := range flags {
		t.Run(flagName, func(t *testing.T) {
			t.Parallel()

			flag := cmd.Flags().Lookup(flagName)
			require.NotNil(t, flag, "flag --%s should be registered", flagName)
		})
	}
}

func TestHistoryCommand_WorkersFlag(t *testing.T) {
	t.Parallel()

	cmd := commands.NewHistoryCommand()

	// Set workers flag.
	err := cmd.Flags().Set("workers", "4")
	require.NoError(t, err)

	val, err := cmd.Flags().GetInt("workers")
	require.NoError(t, err)
	assert.Equal(t, 4, val)
}

func TestHistoryCommand_BlobCacheSizeFlag(t *testing.T) {
	t.Parallel()

	cmd := commands.NewHistoryCommand()

	// Set blob-cache-size flag with size suffix.
	err := cmd.Flags().Set("blob-cache-size", "128MB")
	require.NoError(t, err)

	val, err := cmd.Flags().GetString("blob-cache-size")
	require.NoError(t, err)
	assert.Equal(t, "128MB", val)
}
