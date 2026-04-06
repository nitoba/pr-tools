package doctor

import (
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestEvaluate_MissingEnvFileIsNonBlocking(t *testing.T) {
	report := Evaluate(Input{EnvFileExists: false, ConfigDirCreatable: true})
	assert.False(t, report.Blocking)
}

func TestEvaluate_InvalidGoOwnedSyntaxIsBlocking(t *testing.T) {
	report := Evaluate(Input{GoOwnedParseIssues: 1})
	assert.True(t, report.Blocking)
}

func TestEvaluate_OrdersLinesBySection(t *testing.T) {
	report := Evaluate(Input{
		ConfigDirCreatable: true,
		EnvFileExists:      false,
		Version:            "dev",
		OS:                 "linux",
		Arch:               "amd64",
	})
	require.GreaterOrEqual(t, len(report.Lines), 5)
	assert.Contains(t, report.Lines[0], "config dir")
	assert.Contains(t, report.Lines[1], "env file")
	assert.Contains(t, report.Lines[2], "parse")
	assert.Contains(t, report.Lines[3], "version")
	assert.Contains(t, report.Lines[4], "runtime")
}

func TestEvaluate_UnknownPRTKeyProducesWarningLine(t *testing.T) {
	report := Evaluate(Input{ConfigDirCreatable: true, UnknownPRTKeys: []string{"PRT_FUTURE_KEY"}})
	assert.Contains(t, strings.Join(report.Lines, "\n"), "PRT_FUTURE_KEY")
	assert.False(t, report.Blocking)
}
