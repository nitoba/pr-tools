package setup

import (
	"bufio"
	"errors"
	"os"
	"strings"
)

const (
	BlockStart = "# --- PRT managed block start ---"
	BlockEnd   = "# --- PRT managed block end ---"
)

type EnsureEnvResult string

const (
	ResultCreatedEnvFile      EnsureEnvResult = "created env file"
	ResultUpdatedManagedBlock EnsureEnvResult = "updated managed block"
	ResultAlreadyUpToDate     EnsureEnvResult = "config already up to date"
)

var ErrMultipleManagedBlocks = errors.New("multiple managed blocks found in config file")
var ErrUnmatchedMarkers = errors.New("unmatched managed block markers")

func EnsureEnvFile(path string) (EnsureEnvResult, error) {
	existing, err := os.ReadFile(path)
	if err != nil {
		if !errors.Is(err, os.ErrNotExist) {
			return "", err
		}
		return createNewEnvFile(path)
	}

	return updateOrReplaceManagedBlock(path, string(existing))
}

func createNewEnvFile(path string) (EnsureEnvResult, error) {
	content := BlockStart + "\n" +
		"PRT_CONFIG_VERSION=1\n" +
		"PRT_NO_COLOR=false\n" +
		"PRT_DEBUG=false\n" +
		BlockEnd + "\n"

	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		return "", err
	}

	return ResultCreatedEnvFile, nil
}

func updateOrReplaceManagedBlock(path string, content string) (EnsureEnvResult, error) {
	scanner := bufio.NewScanner(strings.NewReader(content))
	var lines []string
	var insideBlock bool
	var foundBlocks int
	var blockContent []string

	for scanner.Scan() {
		line := scanner.Text()

		if line == BlockStart {
			foundBlocks++
			insideBlock = true
			lines = append(lines, line)
			continue
		}

		if line == BlockEnd {
			if !insideBlock {
				return "", ErrUnmatchedMarkers
			}
			insideBlock = false
			lines = append(lines, line)
			continue
		}

		if insideBlock {
			blockContent = append(blockContent, line)
			continue
		}

		lines = append(lines, line)
	}

	if err := scanner.Err(); err != nil {
		return "", err
	}

	if foundBlocks > 1 {
		return "", ErrMultipleManagedBlocks
	}

	if insideBlock {
		return "", ErrUnmatchedMarkers
	}

	if foundBlocks == 0 {
		return createNewEnvFile(path)
	}

	expectedContent := []string{
		"PRT_CONFIG_VERSION=1",
		"PRT_NO_COLOR=false",
		"PRT_DEBUG=false",
	}

	if slicesEqual(blockContent, expectedContent) {
		return ResultAlreadyUpToDate, nil
	}

	newContent := appendManagedBlockLines(lines, expectedContent)
	if err := os.WriteFile(path, []byte(strings.Join(newContent, "\n")+"\n"), 0o600); err != nil {
		return "", err
	}

	return ResultUpdatedManagedBlock, nil
}

func slicesEqual(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

func appendManagedBlockLines(lines []string, blockContent []string) []string {
	blockWithMarkers := []string{
		BlockStart,
	}
	blockWithMarkers = append(blockWithMarkers, blockContent...)
	blockWithMarkers = append(blockWithMarkers, BlockEnd)

	if len(lines) == 0 {
		return blockWithMarkers
	}

	lastNonEmpty := len(lines) - 1
	for lastNonEmpty >= 0 && strings.TrimSpace(lines[lastNonEmpty]) == "" {
		lastNonEmpty--
	}

	if lastNonEmpty < 0 {
		return blockWithMarkers
	}

	result := make([]string, 0, len(lines)+len(blockWithMarkers))
	result = append(result, lines[:lastNonEmpty+1]...)
	result = append(result, blockWithMarkers...)

	return result
}
