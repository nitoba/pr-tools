package cli

import (
	"bytes"
	"testing"

	"github.com/nitoba/pr-tools/apps/cli-go/internal/setup"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestInitCommand_PrintsExactSummary(t *testing.T) {
	cmd := newInitCommand(InitDependencies{
		Run: func() (InitResult, error) {
			return InitResult{Summary: "created env file"}, nil
		},
	})
	buf := new(bytes.Buffer)
	cmd.SetOut(buf)
	cmd.SetErr(buf)

	err := cmd.Execute()
	require.NoError(t, err)
	assert.Equal(t, "created env file\n", buf.String())
}

func TestComputeSummary(t *testing.T) {
	tests := []struct {
		name       string
		dirCreated bool
		envResult  setup.EnsureEnvResult
		expected   string
	}{
		{
			name:       "created env file",
			dirCreated: false,
			envResult:  setup.ResultCreatedEnvFile,
			expected:   "created env file",
		},
		{
			name:       "created config dir and env file",
			dirCreated: true,
			envResult:  setup.ResultCreatedEnvFile,
			expected:   "created config dir and env file",
		},
		{
			name:       "updated managed block",
			dirCreated: false,
			envResult:  setup.ResultUpdatedManagedBlock,
			expected:   "updated managed block",
		},
		{
			name:       "config already up to date",
			dirCreated: false,
			envResult:  setup.ResultAlreadyUpToDate,
			expected:   "config already up to date",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := computeSummary(tt.dirCreated, tt.envResult)
			assert.Equal(t, tt.expected, result)
		})
	}
}
