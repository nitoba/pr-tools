package cli

import (
	"testing"

	"github.com/nitoba/pr-tools/apps/cli-go/internal/config"
	"github.com/stretchr/testify/require"
)

func TestNewTestCmdHasCorrectMetadata(t *testing.T) {
	t.Parallel()
	cfg := &config.Config{}
	cmd := NewTestCmd(cfg)
	require.Equal(t, "test", cmd.Use)
	require.Equal(t, "Generate Azure DevOps test card from Work Item.", cmd.Short)
	require.NotNil(t, cmd.Flags().Lookup("work-item"))
	require.NotNil(t, cmd.Flags().Lookup("dry-run"))
	require.NotNil(t, cmd.Flags().Lookup("no-create"))
	require.NotNil(t, cmd.Flags().Lookup("raw"))
}

func TestBuildTestPrompt_WithNilWorkItem(t *testing.T) {
	t.Parallel()
	prompt := buildTestPrompt(nil, 42, nil, nil, nil, testFlagSet{})
	require.Contains(t, prompt, "ID: 42")
	require.Contains(t, prompt, "## Contexto do Work Item")
}
