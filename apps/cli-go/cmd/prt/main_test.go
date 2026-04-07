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

func TestRunReturnsOneForTestWithoutConfig(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	for _, key := range []string{
		"PRT_CONFIG_VERSION", "PRT_NO_COLOR", "PRT_DEBUG",
		"PR_PROVIDERS", "OPENROUTER_API_KEY", "GROQ_API_KEY",
		"GEMINI_API_KEY", "OLLAMA_API_KEY",
		"OPENROUTER_MODEL", "GROQ_MODEL", "GEMINI_MODEL", "OLLAMA_MODEL",
		"AZURE_PAT", "PR_REVIEWER_DEV", "PR_REVIEWER_SPRINT",
		"TEST_CARD_AREA_PATH", "TEST_CARD_ASSIGNED_TO",
	} {
		t.Setenv(key, "")
	}

	stdout := new(bytes.Buffer)
	stderr := new(bytes.Buffer)

	code := run([]string{"test"}, stdout, stderr)

	require.Equal(t, 1, code)
	require.Empty(t, stdout.String())
	require.Contains(t, stderr.String(), "Gerando card de teste...")
	require.Contains(t, stderr.String(), "Validando Azure PAT")
	require.Contains(t, stderr.String(), "configuracao incompleta: Azure PAT não configurado")
}
