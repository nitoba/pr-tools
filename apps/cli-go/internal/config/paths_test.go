package config

import (
	"testing"

	"github.com/nitoba/pr-tools/apps/cli-go/internal/platform"
	"github.com/stretchr/testify/require"
)

func TestResolvePathsUsesHomeConfigPolicyForAllOSValues(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name  string
		facts platform.Facts
		want  Paths
	}{
		{
			name: "linux uses home config path",
			facts: platform.Facts{
				OS:      "linux",
				Arch:    "amd64",
				HomeDir: "/home/alice",
			},
			want: Paths{
				ConfigDir: "/home/alice/.config/pr-tools",
				EnvFile:   "/home/alice/.config/pr-tools/.env",
			},
		},
		{
			name: "darwin uses home config path",
			facts: platform.Facts{
				OS:      "darwin",
				Arch:    "arm64",
				HomeDir: "/Users/alice",
			},
			want: Paths{
				ConfigDir: "/Users/alice/.config/pr-tools",
				EnvFile:   "/Users/alice/.config/pr-tools/.env",
			},
		},
		{
			name: "windows keeps home dot config parity",
			facts: platform.Facts{
				OS:      "windows",
				Arch:    "amd64",
				HomeDir: `C:\Users\alice`,
			},
			want: Paths{
				ConfigDir: `C:\Users\alice\.config\pr-tools`,
				EnvFile:   `C:\Users\alice\.config\pr-tools\.env`,
			},
		},
		{
			name: "windows unc home keeps unc root",
			facts: platform.Facts{
				OS:      "windows",
				Arch:    "amd64",
				HomeDir: `\\server\share\alice`,
			},
			want: Paths{
				ConfigDir: `\\server\share\alice\.config\pr-tools`,
				EnvFile:   `\\server\share\alice\.config\pr-tools\.env`,
			},
		},
	}

	for _, tt := range tests {
		tc := tt
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			paths := ResolvePaths(tc.facts)

			require.Equal(t, tc.want, paths)
		})
	}
}
