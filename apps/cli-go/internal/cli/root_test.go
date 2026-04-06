package cli

import (
	"bytes"
	"testing"

	"github.com/nitoba/pr-tools/apps/cli-go/internal/version"
	"github.com/stretchr/testify/require"
)

func TestNewRootCmdBuildsStableMetadata(t *testing.T) {
	t.Parallel()

	cmd := NewRootCmd()

	require.Equal(t, "prt", cmd.Use)
	require.Equal(t, "pr-tools command line interface.", cmd.Short)
	require.Equal(t, version.Info(), cmd.Version)
	require.True(t, cmd.SilenceUsage)
	require.True(t, cmd.SilenceErrors)
	require.NotNil(t, cmd.Commands())
	require.Len(t, cmd.Commands(), 4)
	require.Equal(t, "desc", cmd.Commands()[0].Name())
	require.Equal(t, "doctor", cmd.Commands()[1].Name())
	require.Equal(t, "init", cmd.Commands()[2].Name())
	require.Equal(t, "test", cmd.Commands()[3].Name())
}

func TestNewRootCmdRendersHelp(t *testing.T) {
	t.Parallel()

	cmd := NewRootCmd()
	buffer := new(bytes.Buffer)
	cmd.SetOut(buffer)
	cmd.SetErr(buffer)
	cmd.SetArgs([]string{"--help"})

	err := cmd.Execute()

	require.NoError(t, err)
	require.Contains(t, buffer.String(), "pr-tools command line interface.")
	require.Contains(t, buffer.String(), "Usage:")
	require.Contains(t, buffer.String(), "--version")
}

func TestNewRootCmdPrintsVersion(t *testing.T) {
	t.Parallel()

	cmd := NewRootCmd()
	buffer := new(bytes.Buffer)
	cmd.SetOut(buffer)
	cmd.SetErr(buffer)
	cmd.SetArgs([]string{"--version"})

	err := cmd.Execute()

	require.NoError(t, err)
	require.Contains(t, buffer.String(), version.Info())
}

func TestNewRootCmdWithoutArgsRendersHelp(t *testing.T) {
	t.Parallel()

	cmd := NewRootCmd()
	buffer := new(bytes.Buffer)
	cmd.SetOut(buffer)
	cmd.SetErr(buffer)
	cmd.SetArgs(nil)

	err := cmd.Execute()

	require.Nil(t, err)
	require.Contains(t, buffer.String(), "Usage:")
}
