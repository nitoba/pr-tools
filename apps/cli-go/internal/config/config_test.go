package config

import (
	"strings"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestMergeAppliesConfigPrecedenceAndExplicitFalse(t *testing.T) {
	t.Parallel()

	defaults := Config{
		ConfigVersion: "1",
		NoColor:       Bool(true),
		Debug:         Bool(false),
	}
	fileConfig := Config{
		NoColor: Bool(false),
		Debug:   Bool(true),
	}
	envConfig := Config{
		Debug: Bool(false),
	}
	flagConfig := Config{
		NoColor: Bool(true),
	}

	merged := Merge(defaults, fileConfig, envConfig, flagConfig)

	require.Equal(t, "1", merged.ConfigVersion)
	require.Equal(t, Bool(true), merged.NoColor)
	require.Equal(t, Bool(false), merged.Debug)
}

func TestLoadFileConfigMapsOnlyOwnedKeys(t *testing.T) {
	t.Parallel()

	input := strings.Join([]string{
		"PRT_CONFIG_VERSION=1",
		"PRT_NO_COLOR=true",
		"PRT_DEBUG=false",
		"PRT_FUTURE=enabled",
		"OTHER=value",
	}, "\n")

	cfg, issues := LoadFileConfig(strings.NewReader(input))

	require.Equal(t, Config{
		ConfigVersion: "1",
		NoColor:       Bool(true),
		Debug:         Bool(false),
	}, cfg)
	require.Len(t, issues, 0)
}

func TestLoadEnvConfigUsesLookupEnvAndOwnedSubset(t *testing.T) {
	t.Parallel()

	lookupCalls := make([]string, 0)
	lookupEnv := func(key string) (string, bool) {
		lookupCalls = append(lookupCalls, key)

		values := map[string]string{
			"PRT_CONFIG_VERSION": "2",
			"PRT_NO_COLOR":       "false",
			"PRT_DEBUG":          "true",
			"PRT_UNUSED":         "ignored",
		}

		value, ok := values[key]
		return value, ok
	}

	cfg, issues := LoadEnvConfig(lookupEnv)

	require.Equal(t, []string{"PRT_CONFIG_VERSION", "PRT_NO_COLOR", "PRT_DEBUG"}, lookupCalls)
	require.Equal(t, Config{
		ConfigVersion: "2",
		NoColor:       Bool(false),
		Debug:         Bool(true),
	}, cfg)
	require.Empty(t, issues)
}

func TestLoadersReportMalformedOwnedValues(t *testing.T) {
	t.Parallel()

	fileCfg, fileIssues := LoadFileConfig(strings.NewReader("PRT_NO_COLOR=maybe\nPRT_DEBUG=true\n"))
	require.Equal(t, Config{Debug: Bool(true)}, fileCfg)
	require.Len(t, fileIssues, 1)
	require.Equal(t, "PRT_NO_COLOR", fileIssues[0].Key)

	lookupEnv := func(key string) (string, bool) {
		if key == "PRT_DEBUG" {
			return "sometimes", true
		}
		return "", false
	}

	envCfg, envIssues := LoadEnvConfig(lookupEnv)
	require.Equal(t, Config{}, envCfg)
	require.Len(t, envIssues, 1)
	require.Equal(t, "PRT_DEBUG", envIssues[0].Key)
}

func TestLoadFileConfigKeepsLastValidOwnedValueAcrossInvalidDuplicate(t *testing.T) {
	t.Parallel()

	input := strings.Join([]string{
		"PRT_NO_COLOR=true",
		"PRT_NO_COLOR=maybe",
	}, "\n")

	cfg, issues := LoadFileConfig(strings.NewReader(input))

	require.Equal(t, Config{NoColor: Bool(true)}, cfg)
	require.Len(t, issues, 1)
	require.Equal(t, "PRT_NO_COLOR", issues[0].Key)
}

func TestLoadFileConfigAllowsLaterValidOwnedValueAfterInvalidDuplicate(t *testing.T) {
	t.Parallel()

	input := strings.Join([]string{
		"PRT_NO_COLOR=true",
		"PRT_NO_COLOR=maybe",
		"PRT_NO_COLOR=false",
	}, "\n")

	cfg, issues := LoadFileConfig(strings.NewReader(input))

	require.Equal(t, Config{NoColor: Bool(false)}, cfg)
	require.Len(t, issues, 1)
	require.Equal(t, "PRT_NO_COLOR", issues[0].Key)
}
