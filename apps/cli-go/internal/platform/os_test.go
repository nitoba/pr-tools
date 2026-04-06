package platform

import (
	"os"
	"runtime"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestDetectUsesRuntimeFacts(t *testing.T) {
	t.Parallel()

	homeDir, err := os.UserHomeDir()
	require.NoError(t, err)

	facts, err := Detect()

	require.NoError(t, err)
	require.Equal(t, runtime.GOOS, facts.OS)
	require.Equal(t, runtime.GOARCH, facts.Arch)
	require.Equal(t, homeDir, facts.HomeDir)
}
