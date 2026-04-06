package cli

import (
	"os"
	"path/filepath"

	"github.com/nitoba/pr-tools/apps/cli-go/internal/config"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/version"
	"github.com/spf13/cobra"
)

func NewRootCmd() *cobra.Command {
	cfg := loadConfig()

	cmd := &cobra.Command{
		Use:           "prt",
		Short:         "pr-tools command line interface.",
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(cmd *cobra.Command, _ []string) error {
			return cmd.Help()
		},
	}

	cmd.Version = version.Info()
	cmd.AddCommand(NewDescCmd(cfg), NewTestCmd(cfg), initCommand(), doctorCommand())

	return cmd
}

func loadConfig() *config.Config {
	var fileCfg config.Config
	if dir, err := config.Dir(); err == nil {
		if f, err := os.Open(filepath.Join(dir, ".env")); err == nil {
			defer func() { _ = f.Close() }()
			fileCfg, _ = config.LoadFileConfig(f)
		}
	}
	envCfg, _ := config.LoadEnvConfig(os.LookupEnv)
	merged := config.Merge(fileCfg, envCfg)
	return &merged
}
