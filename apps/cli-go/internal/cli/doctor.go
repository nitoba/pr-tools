package cli

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/nitoba/pr-tools/apps/cli-go/internal/config"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/doctor"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/platform"
	"github.com/nitoba/pr-tools/apps/cli-go/internal/version"
	"github.com/spf13/cobra"
)

type DoctorDependencies struct {
	Run func() (doctor.Report, error)
}

func newDoctorCommand(deps DoctorDependencies) *cobra.Command {
	return &cobra.Command{
		Use:   "doctor",
		Short: "Inspect prt configuration health",
		RunE: func(cmd *cobra.Command, args []string) error {
			report, err := deps.Run()
			if err != nil {
				return &ExitError{Code: 1, Err: err}
			}
			for _, line := range report.Lines {
				cmd.Println(line)
			}
			if report.Blocking {
				return &ExitError{Code: 1, Err: fmt.Errorf("doctor found blocking issues")}
			}
			return nil
		},
	}
}

func doctorCommand() *cobra.Command {
	deps := DoctorDependencies{
		Run: runDoctor,
	}
	return newDoctorCommand(deps)
}

func runDoctor() (doctor.Report, error) {
	facts, err := platform.Detect()
	if err != nil {
		return doctor.Report{}, err
	}

	paths := config.ResolvePaths(facts)

	_, configDirErr := os.Stat(paths.ConfigDir)
	configDirExists := configDirErr == nil
	configDirCreatable := !configDirExists && isDirCreatable(paths.ConfigDir)

	_, envFileErr := os.Stat(paths.EnvFile)
	envFileExists := envFileErr == nil

	envFileReadable := true
	goOwnedIssues := 0
	var unknownKeys []string

	if envFileExists {
		file, err := os.Open(paths.EnvFile)
		if err != nil {
			envFileReadable = false
		} else {
			defer func() { _ = file.Close() }()
			_, parseIssues := config.ParseEnv(file)
			for _, issue := range parseIssues {
				if isGoOwnedKey(issue.Key) {
					goOwnedIssues++
				} else if issue.Key != "" {
					unknownKeys = append(unknownKeys, issue.Key)
				}
			}
		}
	}

	in := doctor.Input{
		ConfigDirExists:    configDirExists,
		ConfigDirCreatable: configDirCreatable,
		EnvFileExists:      envFileExists,
		EnvFileReadable:    envFileReadable,
		GoOwnedParseIssues: goOwnedIssues,
		UnknownPRTKeys:     unknownKeys,
		Version:            version.Version,
		Commit:             version.Commit,
		Date:               version.Date,
		OS:                 facts.OS,
		Arch:               facts.Arch,
	}

	return doctor.Evaluate(in), nil
}

func isDirCreatable(path string) bool {
	dir := filepath.Dir(path)
	info, err := os.Stat(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return isDirCreatable(dir)
		}
		return false
	}
	if !info.IsDir() {
		return false
	}
	testFile := filepath.Join(dir, ".prt-test-write")
	file, err := os.Create(testFile)
	if err != nil {
		return false
	}
	_ = file.Close()
	_ = os.Remove(testFile)
	return true
}

func isGoOwnedKey(key string) bool {
	switch key {
	case "PRT_CONFIG_VERSION", "PRT_NO_COLOR", "PRT_DEBUG":
		return true
	default:
		return false
	}
}
