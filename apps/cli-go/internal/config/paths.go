package config

import (
	"path"
	"strings"

	"github.com/nitoba/pr-tools/apps/cli-go/internal/platform"
)

func Dir() (string, error) {
	facts, err := platform.Detect()
	if err != nil {
		return "", err
	}
	paths := ResolvePaths(facts)
	return paths.ConfigDir, nil
}

type Paths struct {
	ConfigDir string
	EnvFile   string
}

func ResolvePaths(facts platform.Facts) Paths {
	configDir := joinRuntimePath(facts.OS, facts.HomeDir, ".config", "pr-tools")

	return Paths{
		ConfigDir: configDir,
		EnvFile:   joinRuntimePath(facts.OS, configDir, ".env"),
	}
}

func joinRuntimePath(goos string, base string, elems ...string) string {
	if goos == "windows" {
		return joinWindowsPath(base, elems...)
	}

	parts := make([]string, 0, len(elems)+1)
	parts = append(parts, base)
	parts = append(parts, elems...)
	return path.Join(parts...)
}

func joinWindowsPath(base string, elems ...string) string {
	normalizedBase := strings.ReplaceAll(base, `\`, "/")
	prefix := ""

	if strings.HasPrefix(normalizedBase, "//") {
		prefix = `\\`
		normalizedBase = strings.TrimPrefix(normalizedBase, "//")
	} else if len(normalizedBase) >= 3 && normalizedBase[1] == ':' && normalizedBase[2] == '/' {
		drivePrefix := normalizedBase[:3]
		prefix = strings.ReplaceAll(drivePrefix, "/", `\`)
		normalizedBase = strings.TrimPrefix(normalizedBase, drivePrefix)
	}

	parts := make([]string, 0, len(elems)+1)
	if normalizedBase != "" {
		parts = append(parts, normalizedBase)
	}
	parts = append(parts, elems...)
	joined := path.Join(parts...)
	if joined == "." {
		joined = ""
	}

	return prefix + strings.ReplaceAll(joined, "/", `\`)
}
