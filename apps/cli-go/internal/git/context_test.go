package git_test

import (
	"context"
	"fmt"
	"strings"
	"testing"

	"github.com/nitoba/pr-tools/apps/cli-go/internal/git"
	"github.com/stretchr/testify/require"
)

type mockRunner struct {
	outputs map[string]string
	err     map[string]error
}

func (m mockRunner) Run(_ context.Context, name string, args ...string) (string, error) {
	key := name + " " + join(args)
	if e, ok := m.err[key]; ok {
		return "", e
	}
	if out, ok := m.outputs[key]; ok {
		return out, nil
	}
	return "", fmt.Errorf("unexpected command: %s", key)
}

func join(args []string) string {
	return strings.Join(args, " ")
}

func TestContext_Collect_DetectsBaseBranch(t *testing.T) {
	runner := mockRunner{
		outputs: map[string]string{
			"git branch --show-current":                     "feature/456-auth",
			"git rev-parse --verify origin/dev":             "abc123",
			"git diff dev...feature/456-auth --stat":        "1 file changed",
			"git log dev...feature/456-auth --oneline -50":  "abc123 add auth",
			"git remote get-url origin":                     "https://github.com/org/repo",
		},
		err: map[string]error{
			"git rev-parse --verify origin/sprint": fmt.Errorf("not found"),
		},
	}

	ctx := context.Background()
	gitCtx := git.NewContext(runner)
	err := gitCtx.Collect(ctx, "")

	require.NoError(t, err)
	require.Equal(t, "dev", gitCtx.BaseBranch)
	require.Equal(t, "feature/456-auth", gitCtx.BranchName)
	require.Equal(t, "feature/456-auth", gitCtx.SourceBranch)
}

func TestContext_Collect_NotGitRepo(t *testing.T) {
	runner := mockRunner{
		outputs: map[string]string{},
		err: map[string]error{
			"git branch --show-current": fmt.Errorf("not a git repo"),
		},
	}

	ctx := context.Background()
	gitCtx := git.NewContext(runner)
	err := gitCtx.Collect(ctx, "")

	require.Error(t, err)
	require.Equal(t, "not a git repository", err.Error())
}

func TestContext_Collect_ExtractsWorkItemID(t *testing.T) {
	runner := mockRunner{
		outputs: map[string]string{
			"git branch --show-current":                     "feature/456-auth",
			"git rev-parse --verify origin/dev":             "abc123",
			"git diff dev...feature/456-auth --stat":        "1 file changed",
			"git log dev...feature/456-auth --oneline -50":  "abc123 add auth",
			"git remote get-url origin":                     "https://github.com/org/repo",
		},
		err: map[string]error{
			"git rev-parse --verify origin/sprint": fmt.Errorf("not found"),
		},
	}

	ctx := context.Background()
	gitCtx := git.NewContext(runner)
	err := gitCtx.Collect(ctx, "")

	require.NoError(t, err)
	require.Equal(t, "456", gitCtx.WorkItemID)
}

func TestContext_DiffWithLimit_Truncates(t *testing.T) {
	longDiff := strings.Repeat("line\n", 20)
	runner := mockRunner{
		outputs: map[string]string{
			"git diff main...feature": longDiff,
		},
		err: map[string]error{},
	}

	ctx := context.Background()
	gitCtx := git.NewContext(runner)
	result, err := gitCtx.DiffWithLimit(ctx, "main", "feature", 5)

	require.NoError(t, err)
	require.Contains(t, result, "[diff truncated:")
	lines := strings.Split(result, "\n")
	// The truncated result should have far fewer lines than the original
	require.Less(t, len(lines), 20)
}

func TestContext_Collect_DetectsAzureDevOpsRemote(t *testing.T) {
	runner := mockRunner{
		outputs: map[string]string{
			"git branch --show-current":                  "main",
			"git rev-parse --verify origin/main":         "abc123",
			"git diff main...main --stat":                "",
			"git log main...main --oneline -50":          "",
			"git remote get-url origin":                  "https://dev.azure.com/myorg/myproject/_git/myrepo",
		},
		err: map[string]error{
			"git rev-parse --verify origin/sprint": fmt.Errorf("not found"),
			"git rev-parse --verify origin/dev":    fmt.Errorf("not found"),
		},
	}

	ctx := context.Background()
	gitCtx := git.NewContext(runner)
	err := gitCtx.Collect(ctx, "")

	require.NoError(t, err)
	require.True(t, gitCtx.IsAzureDevOps)
	require.Equal(t, "myorg", gitCtx.AzureOrg)
	require.Equal(t, "myproject", gitCtx.AzureProject)
	require.Equal(t, "myrepo", gitCtx.AzureRepo)
}
