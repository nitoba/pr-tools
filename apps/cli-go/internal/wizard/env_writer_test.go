package wizard

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestSetEnvVar_CreatesFile(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, ".env")

	err := SetEnvVar(path, "MY_KEY", "myvalue")
	require.NoError(t, err)

	content, err := os.ReadFile(path)
	require.NoError(t, err)
	assert.Contains(t, string(content), `MY_KEY="myvalue"`)
}

func TestSetEnvVar_UpdatesExistingKey(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, ".env")

	initial := `SOME_KEY="old"` + "\nOTHER_KEY=\"kept\"\n"
	require.NoError(t, os.WriteFile(path, []byte(initial), 0o600))

	err := SetEnvVar(path, "SOME_KEY", "new")
	require.NoError(t, err)

	content, err := os.ReadFile(path)
	require.NoError(t, err)
	assert.Contains(t, string(content), `SOME_KEY="new"`)
	assert.Contains(t, string(content), `OTHER_KEY="kept"`)
	assert.NotContains(t, string(content), `SOME_KEY="old"`)
}

func TestSetEnvVar_AppendsAfterManagedBlockEnd(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, ".env")

	initial := "# --- PRT managed block start ---\nPRT_CONFIG_VERSION=1\n# --- PRT managed block end ---\n"
	require.NoError(t, os.WriteFile(path, []byte(initial), 0o600))

	err := SetEnvVar(path, "NEW_KEY", "newval")
	require.NoError(t, err)

	content, err := os.ReadFile(path)
	require.NoError(t, err)
	lines := strings.Split(strings.TrimSpace(string(content)), "\n")

	var newKeyIdx, endIdx int
	for i, l := range lines {
		if strings.HasPrefix(l, "NEW_KEY=") {
			newKeyIdx = i
		}
		if l == "# --- PRT managed block end ---" {
			endIdx = i
		}
	}
	assert.Greater(t, newKeyIdx, endIdx, "NEW_KEY should appear after block end (outside the managed block)")
}

func TestSetEnvVar_AppendsAtEndIfNoBlock(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, ".env")

	initial := "FOO=\"bar\"\n"
	require.NoError(t, os.WriteFile(path, []byte(initial), 0o600))

	err := SetEnvVar(path, "NEW_KEY", "value")
	require.NoError(t, err)

	content, err := os.ReadFile(path)
	require.NoError(t, err)
	assert.Contains(t, string(content), `NEW_KEY="value"`)
}

func TestSetEnvVar_UpdatesKeyInsideManagedBlock(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, ".env")

	initial := "# --- PRT managed block start ---\nOPENROUTER_API_KEY=\"old\"\n# --- PRT managed block end ---\n"
	require.NoError(t, os.WriteFile(path, []byte(initial), 0o600))

	err := SetEnvVar(path, "OPENROUTER_API_KEY", "newkey")
	require.NoError(t, err)

	content, err := os.ReadFile(path)
	require.NoError(t, err)
	assert.Contains(t, string(content), `OPENROUTER_API_KEY="newkey"`)
	assert.NotContains(t, string(content), `OPENROUTER_API_KEY="old"`)
}

func TestSetEnvVar_HandlesExportPrefix(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, ".env")

	initial := `export MY_KEY="old"` + "\n"
	require.NoError(t, os.WriteFile(path, []byte(initial), 0o600))

	err := SetEnvVar(path, "MY_KEY", "updated")
	require.NoError(t, err)

	content, err := os.ReadFile(path)
	require.NoError(t, err)
	// The line should be updated (export prefix stripped from matching, but replacement is plain)
	assert.Contains(t, string(content), `MY_KEY="updated"`)
}

func TestSetEnvVar_MultipleUpdatesPreserveOtherKeys(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, ".env")

	initial := "# --- PRT managed block start ---\nPRT_CONFIG_VERSION=1\nPRT_DEBUG=false\n# --- PRT managed block end ---\n"
	require.NoError(t, os.WriteFile(path, []byte(initial), 0o600))

	require.NoError(t, SetEnvVar(path, "AZURE_PAT", "token123"))
	require.NoError(t, SetEnvVar(path, "PR_REVIEWER_DEV", "dev@example.com"))

	content, err := os.ReadFile(path)
	require.NoError(t, err)
	assert.Contains(t, string(content), `AZURE_PAT="token123"`)
	assert.Contains(t, string(content), `PR_REVIEWER_DEV="dev@example.com"`)
	assert.Contains(t, string(content), "PRT_CONFIG_VERSION=1")
}
