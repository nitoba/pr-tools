package main

import (
	"errors"
	"fmt"
	"io"
	"os"

	"github.com/nitoba/pr-tools/apps/cli-go/internal/cli"
)

func main() {
	os.Exit(run(os.Args[1:], os.Stdout, os.Stderr))
}

func run(args []string, stdout, stderr io.Writer) int {
	cmd := cli.NewRootCmd()
	cmd.SetArgs(args)
	cmd.SetOut(stdout)
	cmd.SetErr(stderr)

	err := cmd.Execute()
	if err == nil {
		return 0
	}

	_, _ = fmt.Fprintln(stderr, err)

	var exitErr *cli.ExitError
	if ok := errors.As(err, &exitErr); ok {
		return exitErr.Code
	}

	return 1
}
