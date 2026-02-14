//go:build ignore

package commands_test

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/cmd/codefang/commands"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestMCPCommand_Exists(t *testing.T) {
	t.Parallel()

	cmd := commands.NewMCPCommand()
	require.NotNil(t, cmd)
	assert.Equal(t, "mcp", cmd.Use)
	assert.NotEmpty(t, cmd.Short)
	assert.NotEmpty(t, cmd.Long)
}

func TestMCPCommand_DebugFlag(t *testing.T) {
	t.Parallel()

	cmd := commands.NewMCPCommand()
	flag := cmd.Flags().Lookup("debug")
	require.NotNil(t, flag)
	assert.Equal(t, "false", flag.DefValue)
}
