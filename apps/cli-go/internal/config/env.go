package config

import (
	"bufio"
	"fmt"
	"io"
	"strings"
)

type Issue struct {
	Line    int
	Key     string
	Message string
}

func ParseEnv(r io.Reader) (map[string]string, []Issue) {
	values := make(map[string]string)
	issues := make([]Issue, 0)

	scanner := bufio.NewScanner(r)
	for lineNumber := 1; scanner.Scan(); lineNumber++ {
		line := strings.TrimSpace(scanner.Text())
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}

		key, value, ok := parseAssignment(line)
		if !ok {
			if key = parseMalformedPRTKey(line); key != "" {
				issues = append(issues, Issue{Line: lineNumber, Key: key, Message: "invalid assignment"})
			}
			continue
		}

		if issue, ok := validateKnownOrPrefixedKey(lineNumber, key, value); ok {
			issues = append(issues, issue)
			if isOwnedKey(key) {
				continue
			}
		}

		values[key] = value
	}

	if err := scanner.Err(); err != nil {
		issues = append(issues, Issue{Message: fmt.Sprintf("scan env: %v", err)})
	}

	return values, issues
}

func parseAssignment(line string) (string, string, bool) {
	trimmed := stripExportPrefix(line)

	parts := strings.SplitN(trimmed, "=", 2)
	if len(parts) != 2 {
		return "", "", false
	}

	key := strings.TrimSpace(parts[0])
	if key == "" {
		return "", "", false
	}

	value := strings.TrimSpace(parts[1])
	if len(value) >= 2 {
		if value[0] == '\'' && value[len(value)-1] == '\'' {
			value = value[1 : len(value)-1]
		} else if value[0] == '"' && value[len(value)-1] == '"' {
			value = value[1 : len(value)-1]
		}
	}

	return key, value, true
}

func parseMalformedPRTKey(line string) string {
	trimmed := stripExportPrefix(line)
	fields := strings.Fields(trimmed)
	if len(fields) == 0 {
		return ""
	}

	key := strings.TrimSpace(fields[0])
	if strings.HasPrefix(key, "PRT_") {
		return key
	}

	return ""
}

func stripExportPrefix(line string) string {
	trimmed := strings.TrimSpace(line)
	if !strings.HasPrefix(trimmed, "export") {
		return trimmed
	}

	remainder := strings.TrimPrefix(trimmed, "export")
	if remainder == "" {
		return trimmed
	}

	if remainder[0] != ' ' && remainder[0] != '\t' {
		return trimmed
	}

	return strings.TrimSpace(remainder)
}

func validateKnownOrPrefixedKey(line int, key, value string) (Issue, bool) {
	if !strings.HasPrefix(key, "PRT_") {
		return Issue{}, false
	}

	if !isOwnedKey(key) {
		return Issue{}, false
	}

	if key == "PRT_CONFIG_VERSION" {
		if value == "" {
			return Issue{Line: line, Key: key, Message: "invalid value"}, true
		}
		return Issue{}, false
	}

	if _, ok := parseBoolValue(value); !ok {
		return Issue{Line: line, Key: key, Message: "invalid value"}, true
	}

	return Issue{}, false
}
