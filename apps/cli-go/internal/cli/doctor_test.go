package cli

import (
	"bytes"
	"testing"

	"github.com/nitoba/pr-tools/apps/cli-go/internal/doctor"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestDoctorCommand_ReturnsExitErrorOnBlockingReport(t *testing.T) {
	cmd := newDoctorCommand(DoctorDependencies{
		Run: func() (doctor.Report, error) {
			return doctor.Report{Lines: []string{"[ERR] unreadable .env"}, Blocking: true}, nil
		},
	})
	buf := new(bytes.Buffer)
	cmd.SetOut(buf)
	cmd.SetErr(buf)

	err := cmd.Execute()
	require.Error(t, err)
	var exitErr *ExitError
	require.ErrorAs(t, err, &exitErr)
	assert.Equal(t, 1, exitErr.Code)
}

func TestDoctorCommand_PrintsNonBlockingReport(t *testing.T) {
	cmd := newDoctorCommand(DoctorDependencies{
		Run: func() (doctor.Report, error) {
			return doctor.Report{Lines: []string{"[OK] config dir is creatable", "[WARN] .env missing"}, Blocking: false}, nil
		},
	})
	buf := new(bytes.Buffer)
	cmd.SetOut(buf)
	cmd.SetErr(buf)

	err := cmd.Execute()
	require.NoError(t, err)
	assert.Contains(t, buf.String(), "[OK] config dir is creatable")
	assert.Contains(t, buf.String(), "[WARN] .env missing")
}

func TestDoctorCommand_ReturnsErrorOnRunFailure(t *testing.T) {
	cmd := newDoctorCommand(DoctorDependencies{
		Run: func() (doctor.Report, error) {
			return doctor.Report{}, assert.AnError
		},
	})
	buf := new(bytes.Buffer)
	cmd.SetOut(buf)
	cmd.SetErr(buf)

	err := cmd.Execute()
	require.Error(t, err)
	var exitErr *ExitError
	require.ErrorAs(t, err, &exitErr)
	assert.Equal(t, 1, exitErr.Code)
}
