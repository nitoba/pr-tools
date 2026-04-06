package cli

import (
	"errors"

	"github.com/spf13/cobra"
)

const approvedSpecPath = "docs/superpowers/specs/2026-04-06-prt-go-foundation-design.md"

type ExitError struct {
	Code int
	Err  error
}

func (e *ExitError) Error() string {
	if e == nil || e.Err == nil {
		return ""
	}

	return e.Err.Error()
}

func (e *ExitError) Unwrap() error {
	if e == nil {
		return nil
	}

	return e.Err
}

func NewDescCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "desc",
		Short: "Generate PR descriptions.",
		RunE: func(_ *cobra.Command, _ []string) error {
			return &ExitError{
				Code: 2,
				Err:  errors.New("desc not implemented yet; see " + approvedSpecPath),
			}
		},
	}
}
