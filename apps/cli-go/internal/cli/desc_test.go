package cli

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestExitErrorIsNilSafe(t *testing.T) {
	t.Parallel()

	err := &ExitError{}

	require.Equal(t, "", err.Error())
	require.NoError(t, err.Unwrap())
}

func TestNewDescCmdReportsNotImplemented(t *testing.T) {
	t.Parallel()

	cmd := NewDescCmd()

	require.Equal(t, "desc", cmd.Use)
	require.Equal(t, "Generate PR descriptions.", cmd.Short)

	err := cmd.RunE(cmd, nil)

	var exitErr *ExitError
	require.ErrorAs(t, err, &exitErr)
	require.Equal(t, 2, exitErr.Code)
	require.EqualError(t, exitErr.Unwrap(), "desc not implemented yet; see docs/superpowers/specs/2026-04-06-prt-go-foundation-design.md")
	require.Equal(t, exitErr.Unwrap().Error(), exitErr.Error())
}
