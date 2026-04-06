package cli

import (
	"github.com/nitoba/pr-tools/apps/cli-go/internal/version"
	"github.com/spf13/cobra"
)

func NewRootCmd() *cobra.Command {
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
	cmd.AddCommand(NewDescCmd(), NewTestCmd(), initCommand(), doctorCommand())

	return cmd
}
