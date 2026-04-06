package setup

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestEnsureEnvFile_CreatesManagedBlockOnce(t *testing.T) {
	dir := t.TempDir()
	envFile := filepath.Join(dir, ".env")

	result, err := EnsureEnvFile(envFile)
	require.NoError(t, err)
	assert.Equal(t, ResultCreatedEnvFile, result)

	contents, err := os.ReadFile(envFile)
	require.NoError(t, err)
	assert.Contains(t, string(contents), "# --- PRT managed block start ---")
	assert.Contains(t, string(contents), "PRT_CONFIG_VERSION=1")
}

func TestEnsureEnvFile_IsIdempotent(t *testing.T) {
	dir := t.TempDir()
	envFile := filepath.Join(dir, ".env")

	_, err := EnsureEnvFile(envFile)
	require.NoError(t, err)

	result, err := EnsureEnvFile(envFile)
	require.NoError(t, err)
	assert.Equal(t, ResultAlreadyUpToDate, result)
}

func TestEnsureEnvFile_FailsOnMultipleManagedBlocks(t *testing.T) {
	dir := t.TempDir()
	envFile := filepath.Join(dir, ".env")
	require.NoError(t, os.WriteFile(envFile, []byte(BlockStart+"\n"+BlockEnd+"\n"+BlockStart+"\n"+BlockEnd+"\n"), 0o600))

	_, err := EnsureEnvFile(envFile)
	require.Error(t, err)
}

func TestEnsureEnvFile_FailsOnUnmatchedMarkers(t *testing.T) {
	dir := t.TempDir()
	envFile := filepath.Join(dir, ".env")
	require.NoError(t, os.WriteFile(envFile, []byte(BlockStart+"\nPRT_DEBUG=false\n"), 0o600))

	_, err := EnsureEnvFile(envFile)
	require.Error(t, err)
}
