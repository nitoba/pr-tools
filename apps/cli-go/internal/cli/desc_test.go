package cli

import (
	"testing"

	"github.com/nitoba/pr-tools/apps/cli-go/internal/config"
	"github.com/stretchr/testify/require"
)

func TestExitErrorIsNilSafe(t *testing.T) {
	t.Parallel()

	err := &ExitError{}

	require.Equal(t, "", err.Error())
	require.NoError(t, err.Unwrap())
}

func TestNewDescCmdHasCorrectMetadata(t *testing.T) {
	t.Parallel()

	cfg := &config.Config{}
	cmd := NewDescCmd(cfg)

	require.Equal(t, "desc", cmd.Use)
	require.Equal(t, "Generate PR descriptions.", cmd.Short)
	require.NotNil(t, cmd.Flags().Lookup("source"))
	require.NotNil(t, cmd.Flags().Lookup("dry-run"))
	require.NotNil(t, cmd.Flags().Lookup("create"))
}

func TestParseTitleAndBody_ExtractsTITULO(t *testing.T) {
	t.Parallel()

	resp := "TITULO: My PR Title\n## Descrição\nSome description"
	title, body := parseTitleAndBody(resp)

	require.Equal(t, "My PR Title", title)
	require.Contains(t, body, "## Descrição")
}

func TestParseTitleAndBody_FallbackToFirstLine(t *testing.T) {
	t.Parallel()

	resp := "First line\nSecond line"
	title, body := parseTitleAndBody(resp)

	require.Equal(t, "First line", title)
	require.Equal(t, "Second line", body)
}
