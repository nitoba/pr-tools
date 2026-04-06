package cli

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/nitoba/pr-tools/apps/cli-go/internal/config"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/setup"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/wizard"
	"github.com/spf13/cobra"
)

type InitResult struct {
	Summary string
}

type InitDependencies struct {
	ConfigDir func() (string, error)
	EnsureEnv func(string) (setup.EnsureEnvResult, error)
	Run       func() (InitResult, error)
}

func newInitCommand(deps InitDependencies) *cobra.Command {
	var cmd = &cobra.Command{
		Use:   "init",
		Short: "Initialize or update the PRT configuration",
		RunE: func(cmd *cobra.Command, _ []string) error {
			result, err := deps.Run()
			if err != nil {
				return err
			}
			cmd.Println(result.Summary)
			return nil
		},
	}
	return cmd
}

type defaultInitDeps struct {
	ensureEnv func(string) (setup.EnsureEnvResult, error)
}

func (d *defaultInitDeps) configDir() (string, error) {
	return config.Dir()
}

func (d *defaultInitDeps) ensure(path string) (setup.EnsureEnvResult, error) {
	return d.ensureEnv(path)
}

func (d *defaultInitDeps) run() (InitResult, error) {
	dir, err := d.configDir()
	if err != nil {
		return InitResult{}, err
	}

	dirCreated := false
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return InitResult{}, err
	}
	if isDirNewlyCreated(dir) {
		dirCreated = true
	}

	envPath := filepath.Join(dir, ".env")
	result, err := d.ensure(envPath)
	if err != nil {
		return InitResult{}, err
	}

	// Run the interactive wizard when stdin is a real terminal.
	if wizard.IsTerminal(os.Stdin.Fd()) {
		if werr := wizard.Run(os.Stdin, os.Stderr, envPath); werr != nil {
			return InitResult{}, fmt.Errorf("wizard: %w", werr)
		}
		return InitResult{Summary: ""}, nil
	}

	// Non-interactive mode: just print the standard summary.
	_, _ = fmt.Fprintln(os.Stderr, "[AVISO] Edite ~/.config/pr-tools/.env e preencha suas API keys.")
	summary := computeSummary(dirCreated, result)
	return InitResult{Summary: summary}, nil
}

func isDirNewlyCreated(path string) bool {
	info, err := os.Stat(path)
	if err != nil {
		return false
	}
	return info.Size() == 0
}

func computeSummary(dirCreated bool, envResult setup.EnsureEnvResult) string {
	switch envResult {
	case setup.ResultCreatedEnvFile:
		if dirCreated {
			return "created config dir and env file"
		}
		return "created env file"
	case setup.ResultUpdatedManagedBlock:
		return "updated managed block"
	case setup.ResultAlreadyUpToDate:
		return "config already up to date"
	default:
		return "unknown state"
	}
}

func initCommand() *cobra.Command {
	deps := &defaultInitDeps{
		ensureEnv: setup.EnsureEnvFile,
	}

	realDeps := InitDependencies{
		ConfigDir: deps.configDir,
		EnsureEnv: deps.ensure,
		Run:       deps.run,
	}

	return newInitCommand(realDeps)
}
