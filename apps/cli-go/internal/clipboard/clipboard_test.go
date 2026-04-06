package clipboard_test

import (
	"testing"

	"github.com/nitoba/pr-tools/apps/cli-go/internal/clipboard"
	"github.com/stretchr/testify/require"
)

func TestErrUnavailableIsExported(t *testing.T) {
	require.Error(t, clipboard.ErrUnavailable)
	require.Equal(t, "clipboard: no compatible tool found", clipboard.ErrUnavailable.Error())
}
