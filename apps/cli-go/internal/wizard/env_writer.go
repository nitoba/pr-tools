package wizard

import (
	"bufio"
	"fmt"
	"os"
	"strings"
)

// SetEnvVar sets or appends a KEY="VALUE" line in the given .env file.
// If the key already exists (inside or outside the managed block), it is updated in-place.
// If it does not exist, it is appended before the managed-block end marker, or at the end of the file.
func SetEnvVar(path, key, value string) error {
	existing, err := os.ReadFile(path)
	if err != nil {
		if !os.IsNotExist(err) {
			return fmt.Errorf("read env file: %w", err)
		}
		// File does not exist — create it with the single key
		line := fmt.Sprintf("%s=%q\n", key, value)
		return os.WriteFile(path, []byte(line), 0o600)
	}

	lines := splitLines(string(existing))
	updated, ok := replaceKey(lines, key, value)
	if ok {
		content := strings.Join(updated, "\n")
		if !strings.HasSuffix(content, "\n") {
			content += "\n"
		}
		return os.WriteFile(path, []byte(content), 0o600)
	}

	// Key not found — append before managed-block end or at end of file
	appended := appendKey(lines, key, value)
	content := strings.Join(appended, "\n")
	if !strings.HasSuffix(content, "\n") {
		content += "\n"
	}
	return os.WriteFile(path, []byte(content), 0o600)
}

// replaceKey looks for an existing KEY= line and replaces it.
// Returns the modified lines and true if a replacement was made.
func replaceKey(lines []string, key, value string) ([]string, bool) {
	prefix := key + "="
	result := make([]string, len(lines))
	copy(result, lines)

	for i, line := range result {
		bare := strings.TrimSpace(line)
		bare = stripExportPrefix(bare)
		if strings.HasPrefix(bare, prefix) {
			result[i] = fmt.Sprintf("%s=%q", key, value)
			return result, true
		}
	}
	return result, false
}

// appendKey inserts KEY="VALUE" after the managed-block end marker if present,
// otherwise appends at the end.
func appendKey(lines []string, key, value string) []string {
	const blockEnd = "# --- PRT managed block end ---"
	newLine := fmt.Sprintf("%s=%q", key, value)

	for i, line := range lines {
		if strings.TrimSpace(line) == blockEnd {
			result := make([]string, 0, len(lines)+1)
			result = append(result, lines[:i+1]...)
			result = append(result, newLine)
			result = append(result, lines[i+1:]...)
			return result
		}
	}

	return append(lines, newLine)
}

// splitLines splits content into lines, preserving empty lines but not the trailing newline.
func splitLines(content string) []string {
	if content == "" {
		return nil
	}
	scanner := bufio.NewScanner(strings.NewReader(content))
	var lines []string
	for scanner.Scan() {
		lines = append(lines, scanner.Text())
	}
	return lines
}

func stripExportPrefix(line string) string {
	if !strings.HasPrefix(line, "export") {
		return line
	}
	remainder := strings.TrimPrefix(line, "export")
	if remainder == "" || (remainder[0] != ' ' && remainder[0] != '\t') {
		return line
	}
	return strings.TrimSpace(remainder)
}
