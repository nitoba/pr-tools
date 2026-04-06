package config

import (
	"io"
	"strings"
)

type Config struct {
	ConfigVersion string
	NoColor       *bool
	Debug         *bool

	// PR/Test configuration keys
	Providers          string
	OpenRouterAPIKey   string
	GroqAPIKey         string
	GeminiAPIKey       string
	OllamaAPIKey       string
	OpenRouterModel    string
	GroqModel          string
	GeminiModel        string
	OllamaModel        string
	AzurePAT           string
	PRReviewerDev      string
	PRReviewerSprint   string
	TestCardAreaPath   string
	TestCardAssignedTo string
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
		if cfg.Providers != "" {
			merged.Providers = cfg.Providers
		}
		if cfg.OpenRouterAPIKey != "" {
			merged.OpenRouterAPIKey = cfg.OpenRouterAPIKey
		}
		if cfg.GroqAPIKey != "" {
			merged.GroqAPIKey = cfg.GroqAPIKey
		}
		if cfg.GeminiAPIKey != "" {
			merged.GeminiAPIKey = cfg.GeminiAPIKey
		}
		if cfg.OllamaAPIKey != "" {
			merged.OllamaAPIKey = cfg.OllamaAPIKey
		}
		if cfg.OpenRouterModel != "" {
			merged.OpenRouterModel = cfg.OpenRouterModel
		}
		if cfg.GroqModel != "" {
			merged.GroqModel = cfg.GroqModel
		}
		if cfg.GeminiModel != "" {
			merged.GeminiModel = cfg.GeminiModel
		}
		if cfg.OllamaModel != "" {
			merged.OllamaModel = cfg.OllamaModel
		}
		if cfg.AzurePAT != "" {
			merged.AzurePAT = cfg.AzurePAT
		}
		if cfg.PRReviewerDev != "" {
			merged.PRReviewerDev = cfg.PRReviewerDev
		}
		if cfg.PRReviewerSprint != "" {
			merged.PRReviewerSprint = cfg.PRReviewerSprint
		}
		if cfg.TestCardAreaPath != "" {
			merged.TestCardAreaPath = cfg.TestCardAreaPath
		}
		if cfg.TestCardAssignedTo != "" {
			merged.TestCardAssignedTo = cfg.TestCardAssignedTo
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
	for _, key := range []string{
		"PRT_CONFIG_VERSION", "PRT_NO_COLOR", "PRT_DEBUG",
		"PR_PROVIDERS", "OPENROUTER_API_KEY", "GROQ_API_KEY",
		"GEMINI_API_KEY", "OLLAMA_API_KEY",
		"OPENROUTER_MODEL", "GROQ_MODEL", "GEMINI_MODEL", "OLLAMA_MODEL",
		"AZURE_PAT", "PR_REVIEWER_DEV", "PR_REVIEWER_SPRINT",
		"TEST_CARD_AREA_PATH", "TEST_CARD_ASSIGNED_TO",
	} {
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
		case "PR_PROVIDERS":
			config.Providers = value
		case "OPENROUTER_API_KEY":
			config.OpenRouterAPIKey = value
		case "GROQ_API_KEY":
			config.GroqAPIKey = value
		case "GEMINI_API_KEY":
			config.GeminiAPIKey = value
		case "OLLAMA_API_KEY":
			config.OllamaAPIKey = value
		case "OPENROUTER_MODEL":
			config.OpenRouterModel = value
		case "GROQ_MODEL":
			config.GroqModel = value
		case "GEMINI_MODEL":
			config.GeminiModel = value
		case "OLLAMA_MODEL":
			config.OllamaModel = value
		case "AZURE_PAT":
			config.AzurePAT = value
		case "PR_REVIEWER_DEV":
			config.PRReviewerDev = value
		case "PR_REVIEWER_SPRINT":
			config.PRReviewerSprint = value
		case "TEST_CARD_AREA_PATH":
			config.TestCardAreaPath = value
		case "TEST_CARD_ASSIGNED_TO":
			config.TestCardAssignedTo = value
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
