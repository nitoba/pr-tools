package config

import (
	"errors"
	"io"
	"strings"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestParseEnvParsesSupportedSyntaxAndReportsKnownIssues(t *testing.T) {
	t.Parallel()

	input := strings.Join([]string{
		"",
		"# comment",
		"PRT_CONFIG_VERSION=1",
		"export PRT_NO_COLOR=true",
		"PRT_NO_COLOR=false",
		"PRT_DEBUG='true'",
		"PRT_DEBUG=\"false\"",
		"PRT_FUTURE=enabled",
		"export PRT_DEBUG",
		"BAD LINE",
		"PRT_NO_COLOR=",
		"OTHER=value",
	}, "\n")

	values, issues := ParseEnv(strings.NewReader(input))

	require.Equal(t, map[string]string{
		"OTHER":              "value",
		"PRT_CONFIG_VERSION": "1",
		"PRT_DEBUG":          "false",
		"PRT_FUTURE":         "enabled",
		"PRT_NO_COLOR":       "false",
	}, values)
	require.Len(t, issues, 2)
	require.Equal(t, "PRT_DEBUG", issues[0].Key)
	require.Equal(t, 9, issues[0].Line)
	require.Equal(t, "PRT_NO_COLOR", issues[1].Key)
	require.Equal(t, 11, issues[1].Line)
}

func TestParseEnvKeepsLastDuplicateAcrossQuoteStyles(t *testing.T) {
	t.Parallel()

	input := strings.Join([]string{
		"PRT_DEBUG=true",
		"PRT_DEBUG='false'",
		"PRT_DEBUG=\"true\"",
	}, "\n")

	values, issues := ParseEnv(strings.NewReader(input))

	require.Equal(t, map[string]string{"PRT_DEBUG": "true"}, values)
	require.Empty(t, issues)
}

func TestParseEnvKeepsLastValidOwnedValueWhenDuplicateIsInvalid(t *testing.T) {
	t.Parallel()

	input := strings.Join([]string{
		"PRT_DEBUG=true",
		"PRT_DEBUG=maybe",
	}, "\n")

	values, issues := ParseEnv(strings.NewReader(input))

	require.Equal(t, map[string]string{"PRT_DEBUG": "true"}, values)
	require.Len(t, issues, 1)
	require.Equal(t, "PRT_DEBUG", issues[0].Key)
	require.Equal(t, 2, issues[0].Line)
}

func TestParseEnvAllowsLaterValidOwnedValueAfterInvalidDuplicate(t *testing.T) {
	t.Parallel()

	input := strings.Join([]string{
		"PRT_DEBUG=true",
		"PRT_DEBUG=maybe",
		"PRT_DEBUG=false",
	}, "\n")

	values, issues := ParseEnv(strings.NewReader(input))

	require.Equal(t, map[string]string{"PRT_DEBUG": "false"}, values)
	require.Len(t, issues, 1)
	require.Equal(t, "PRT_DEBUG", issues[0].Key)
	require.Equal(t, 2, issues[0].Line)
}

func TestParseEnvSupportsExportWithVariableWhitespace(t *testing.T) {
	t.Parallel()

	input := strings.Join([]string{
		"export   PRT_NO_COLOR=true",
		"export\tPRT_DEBUG=false",
	}, "\n")

	values, issues := ParseEnv(strings.NewReader(input))

	require.Equal(t, map[string]string{
		"PRT_DEBUG":    "false",
		"PRT_NO_COLOR": "true",
	}, values)
	require.Empty(t, issues)
}

func TestParseEnvReportsScanFailuresAsIssues(t *testing.T) {
	t.Parallel()

	values, issues := ParseEnv(&errReader{err: errors.New("boom")})

	require.Empty(t, values)
	require.Len(t, issues, 1)
	require.Empty(t, issues[0].Key)
	require.Equal(t, "scan env: boom", issues[0].Message)
}

func TestParseEnvReportsMalformedUnknownPRTLines(t *testing.T) {
	t.Parallel()

	input := strings.Join([]string{
		"PRT_FUTURE",
		"export PRT_EXPERIMENTAL",
	}, "\n")

	values, issues := ParseEnv(strings.NewReader(input))

	require.Empty(t, values)
	require.Len(t, issues, 2)
	require.Equal(t, "PRT_FUTURE", issues[0].Key)
	require.Equal(t, 1, issues[0].Line)
	require.Equal(t, "invalid assignment", issues[0].Message)
	require.Equal(t, "PRT_EXPERIMENTAL", issues[1].Key)
	require.Equal(t, 2, issues[1].Line)
	require.Equal(t, "invalid assignment", issues[1].Message)
}

type errReader struct {
	err  error
	read bool
}

func (r *errReader) Read(_ []byte) (int, error) {
	if r.read {
		return 0, io.EOF
	}
	r.read = true
	return 0, r.err
}
