package cli

import (
	"errors"

	"github.com/nitoba/pr-tools/apps/cli-go/internal/config"
	"github.com/spf13/cobra"
)

func NewTestCmd(_ *config.Config) *cobra.Command {
	return &cobra.Command{
		Use:   "test",
		Short: "Run CLI checks and tests.",
		RunE: func(_ *cobra.Command, _ []string) error {
			return &ExitError{
				Code: 2,
				Err:  errors.New("test not implemented yet; see " + approvedSpecPath),
			}
		},
	}
}
