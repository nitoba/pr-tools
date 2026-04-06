package config

import (
	"io"
	"strings"
)

type Config struct {
	ConfigVersion string
	NoColor       *bool
	Debug         *bool
}

func Bool(value bool) *bool {
	result := value
	return &result
}

func Merge(configs ...Config) Config {
	merged := Config{}

	for _, cfg := range configs {
		if cfg.ConfigVersion != "" {
			merged.ConfigVersion = cfg.ConfigVersion
		}
		if cfg.NoColor != nil {
			merged.NoColor = Bool(*cfg.NoColor)
		}
		if cfg.Debug != nil {
			merged.Debug = Bool(*cfg.Debug)
		}
	}

	return merged
}

func LoadFileConfig(r io.Reader) (Config, []Issue) {
	values, issues := ParseEnv(r)
	config, mappingIssues := mapConfig(values)
	return config, append(issues, filterDuplicateKeyIssues(mappingIssues, issues)...)
}

func LoadEnvConfig(lookupEnv func(string) (string, bool)) (Config, []Issue) {
	values := make(map[string]string)
	for _, key := range []string{"PRT_CONFIG_VERSION", "PRT_NO_COLOR", "PRT_DEBUG"} {
		if value, ok := lookupEnv(key); ok {
			values[key] = value
		}
	}

	return mapConfig(values)
}

func mapConfig(values map[string]string) (Config, []Issue) {
	config := Config{}
	issues := make([]Issue, 0)

	for key, value := range values {
		switch key {
		case "PRT_CONFIG_VERSION":
			if value == "" {
				issues = append(issues, Issue{Key: key, Message: "invalid value"})
				continue
			}
			config.ConfigVersion = value
		case "PRT_NO_COLOR":
			parsed, ok := parseBoolValue(value)
			if !ok {
				issues = append(issues, Issue{Key: key, Message: "invalid value"})
				continue
			}
			config.NoColor = Bool(parsed)
		case "PRT_DEBUG":
			parsed, ok := parseBoolValue(value)
			if !ok {
				issues = append(issues, Issue{Key: key, Message: "invalid value"})
				continue
			}
			config.Debug = Bool(parsed)
		default:
		}
	}

	return config, issues
}

func parseBoolValue(value string) (bool, bool) {
	switch strings.ToLower(strings.TrimSpace(value)) {
	case "true":
		return true, true
	case "false":
		return false, true
	default:
		return false, false
	}
}

func isOwnedKey(key string) bool {
	switch key {
	case "PRT_CONFIG_VERSION", "PRT_NO_COLOR", "PRT_DEBUG":
		return true
	default:
		return false
	}
}

func filterDuplicateKeyIssues(issues []Issue, existing []Issue) []Issue {
	if len(issues) == 0 || len(existing) == 0 {
		return issues
	}

	seen := make(map[string]struct{}, len(existing))
	for _, issue := range existing {
		if issue.Key != "" {
			seen[issue.Key] = struct{}{}
		}
	}

	filtered := make([]Issue, 0, len(issues))
	for _, issue := range issues {
		if _, ok := seen[issue.Key]; ok {
			continue
		}
		filtered = append(filtered, issue)
	}

	return filtered
}
