package main

import (
	"bytes"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestRunReturnsZeroForDescHelp(t *testing.T) {
	t.Parallel()

	stdout := new(bytes.Buffer)
	stderr := new(bytes.Buffer)

	code := run([]string{"desc", "--help"}, stdout, stderr)

	require.Equal(t, 0, code)
	require.Contains(t, stdout.String(), "desc")
	require.Empty(t, stderr.String())
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

func TestRunReturnsOneForTestMissingRequiredFlag(t *testing.T) {
	t.Parallel()

	stdout := new(bytes.Buffer)
	stderr := new(bytes.Buffer)

	code := run([]string{"test"}, stdout, stderr)

	require.Equal(t, 1, code)
	require.Empty(t, stdout.String())
	require.Contains(t, stderr.String(), "work-item")
}
