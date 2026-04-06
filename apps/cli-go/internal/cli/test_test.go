package cli

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestNewTestCmdReportsNotImplemented(t *testing.T) {
	t.Parallel()

	cmd := NewTestCmd()

	require.Equal(t, "test", cmd.Use)
	require.Equal(t, "Run CLI checks and tests.", cmd.Short)

	err := cmd.RunE(cmd, nil)

	var exitErr *ExitError
	require.ErrorAs(t, err, &exitErr)
	require.Equal(t, 2, exitErr.Code)
	require.EqualError(t, exitErr.Unwrap(), "test not implemented yet; see docs/superpowers/specs/2026-04-06-prt-go-foundation-design.md")
	require.Equal(t, exitErr.Unwrap().Error(), exitErr.Error())
}
