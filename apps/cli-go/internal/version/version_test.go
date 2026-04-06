package version

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestDefaultsAreStable(t *testing.T) {
	t.Parallel()

	require.Equal(t, "dev", Version)
	require.Equal(t, "unknown", Commit)
	require.Equal(t, "unknown", Date)
}

func TestInfoUsesStableOrdering(t *testing.T) {
	t.Parallel()

	require.Equal(t, "dev (unknown, unknown)", Info())
}

func TestInfoUsesOverriddenMetadata(t *testing.T) {
	originalVersion := Version
	originalCommit := Commit
	originalDate := Date
	t.Cleanup(func() {
		Version = originalVersion
		Commit = originalCommit
		Date = originalDate
	})

	tests := []struct {
		name    string
		version string
		commit  string
		date    string
		want    string
	}{
		{
			name:    "release build metadata",
			version: "v1.2.3",
			commit:  "abc1234",
			date:    "2026-04-06T12:00:00Z",
			want:    "v1.2.3 (abc1234, 2026-04-06T12:00:00Z)",
		},
		{
			name:    "snapshot build metadata",
			version: "snapshot",
			commit:  "deadbeef",
			date:    "2026-04-07",
			want:    "snapshot (deadbeef, 2026-04-07)",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			Version = tt.version
			Commit = tt.commit
			Date = tt.date

			require.Equal(t, tt.want, Info())
		})
	}
}
