package main

import (
	"bytes"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestRunReturnsExitCodeTwoForDesc(t *testing.T) {
	t.Parallel()

	stdout := new(bytes.Buffer)
	stderr := new(bytes.Buffer)

	code := run([]string{"desc"}, stdout, stderr)

	require.Equal(t, 2, code)
	require.Empty(t, stdout.String())
	require.Contains(t, stderr.String(), "desc not implemented yet")
	require.Contains(t, stderr.String(), "docs/superpowers/specs/2026-04-06-prt-go-foundation-design.md")
}

func TestRunReturnsZeroForHelp(t *testing.T) {
	t.Parallel()

	stdout := new(bytes.Buffer)
	stderr := new(bytes.Buffer)

	code := run([]string{"--help"}, stdout, stderr)

	require.Equal(t, 0, code)
	require.Contains(t, stdout.String(), "Usage:")
	require.Empty(t, stderr.String())
}

func TestRunReturnsOneForInvalidCommand(t *testing.T) {
	t.Parallel()

	stdout := new(bytes.Buffer)
	stderr := new(bytes.Buffer)

	code := run([]string{"missing-command"}, stdout, stderr)

	require.Equal(t, 1, code)
	require.Empty(t, stdout.String())
	require.Contains(t, stderr.String(), "unknown command \"missing-command\" for \"prt\"")
}

func TestRunReturnsExitCodeTwoForTest(t *testing.T) {
	t.Parallel()

	stdout := new(bytes.Buffer)
	stderr := new(bytes.Buffer)

	code := run([]string{"test"}, stdout, stderr)

	require.Equal(t, 2, code)
	require.Empty(t, stdout.String())
	require.Contains(t, stderr.String(), "test not implemented yet")
	require.Contains(t, stderr.String(), "docs/superpowers/specs/2026-04-06-prt-go-foundation-design.md")
}
