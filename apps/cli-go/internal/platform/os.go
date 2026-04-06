package platform

import (
	"os"
	"runtime"
)

type Facts struct {
	OS      string
	Arch    string
	HomeDir string
}

func Detect() (Facts, error) {
	homeDir, err := os.UserHomeDir()
	if err != nil {
		return Facts{}, err
	}

	return Facts{
		OS:      runtime.GOOS,
		Arch:    runtime.GOARCH,
		HomeDir: homeDir,
	}, nil
}
