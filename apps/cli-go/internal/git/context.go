package git

import (
	"context"
	"errors"
	"fmt"
	"os/exec"
	"strconv"
	"strings"
)

type Context struct {
	BranchName         string
	SourceBranch       string
	BaseBranch         string
	SprintBranch       string // e.g. "sprint/98" if detected
	Diff               string
	DiffTruncated      bool
	DiffOriginalLines  int
	Log                string
	WorkItemID         string
	IsAzureDevOps      bool
	AzureOrg           string
	AzureProject       string
	AzureRepo          string

	runner Runner
}

type Runner interface {
	Run(ctx context.Context, name string, args ...string) (string, error)
}

type ExecRunner struct{}

func (e ExecRunner) Run(ctx context.Context, name string, args ...string) (string, error) {
	cmd := exec.CommandContext(ctx, name, args...)
	out, err := cmd.Output()
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(string(out)), nil
}

func NewContext(runner Runner) *Context {
	return &Context{runner: runner}
}

// Collect populates the Context from the current git repository.
// sourceBranch overrides the current branch if provided.
func (c *Context) Collect(ctx context.Context, sourceBranch string) error {
	// Get current branch
	branch, err := c.runner.Run(ctx, "git", "branch", "--show-current")
	if err != nil {
		return errors.New("not a git repository")
	}
	c.BranchName = branch
	if sourceBranch != "" {
		c.SourceBranch = sourceBranch
	} else {
		c.SourceBranch = branch
	}

	// Extract work item ID from branch name (e.g. feature/123-login → 123)
	c.WorkItemID = extractWorkItemID(c.BranchName)

	// Get base branch (try: sprint, dev, main, master)
	base, err := c.detectBaseBranch(ctx)
	if err != nil {
		return err
	}
	c.BaseBranch = base

	// Get full diff, truncated to 8000 lines
	const diffMaxLines = 8000
	rawDiff, err := c.runner.Run(ctx, "git", "diff", base+"..."+c.SourceBranch)
	if err == nil {
		lines := strings.Split(rawDiff, "\n")
		c.DiffOriginalLines = len(lines)
		if len(lines) > diffMaxLines {
			c.DiffTruncated = true
			c.Diff = strings.Join(lines[:diffMaxLines], "\n")
		} else {
			c.Diff = rawDiff
		}
	}

	// Get log
	log, err := c.runner.Run(ctx, "git", "log", base+"..."+c.SourceBranch, "--oneline", "-50")
	if err == nil {
		c.Log = log
	}

	// Detect Azure DevOps remote
	c.detectAzureRemote(ctx)

	return nil
}

func (c *Context) detectBaseBranch(ctx context.Context) (string, error) {
	// 1. Try to find sprint/NNN branches (highest number wins)
	sprintBranch := c.detectLatestSprintBranch(ctx)
	if sprintBranch != "" {
		c.SprintBranch = sprintBranch
	}

	// 2. Find integration branch: dev > main > master
	for _, candidate := range []string{"dev", "main", "master"} {
		if _, err := c.runner.Run(ctx, "git", "rev-parse", "--verify", "origin/"+candidate); err == nil {
			return candidate, nil
		}
	}

	// 3. Fallback: if only sprint found, use it
	if sprintBranch != "" {
		return sprintBranch, nil
	}

	return "", errors.New("could not detect base branch")
}

func (c *Context) detectLatestSprintBranch(ctx context.Context) string {
	out, err := c.runner.Run(ctx, "git", "branch", "-r")
	if err != nil {
		return ""
	}
	// Find all origin/sprint/NNN branches
	var latest string
	var latestNum int
	for _, line := range strings.Split(out, "\n") {
		line = strings.TrimSpace(line)
		if !strings.HasPrefix(line, "origin/sprint/") {
			continue
		}
		suffix := strings.TrimPrefix(line, "origin/sprint/")
		// extract leading digits
		numStr := ""
		for _, ch := range suffix {
			if ch >= '0' && ch <= '9' {
				numStr += string(ch)
			} else {
				break
			}
		}
		if numStr == "" {
			continue
		}
		n, _ := strconv.Atoi(numStr)
		if n > latestNum {
			latestNum = n
			latest = "sprint/" + suffix // keep full branch name like sprint/98
		}
	}
	return latest
}

func (c *Context) detectAzureRemote(ctx context.Context) {
	remote, err := c.runner.Run(ctx, "git", "remote", "get-url", "origin")
	if err != nil {
		return
	}
	// Azure DevOps URL patterns:
	// https://dev.azure.com/org/project/_git/repo
	// https://org.visualstudio.com/project/_git/repo
	if strings.Contains(remote, "dev.azure.com") {
		c.IsAzureDevOps = true
		parts := strings.Split(remote, "/")
		// https://dev.azure.com/{org}/{project}/_git/{repo}
		for i, p := range parts {
			if p == "dev.azure.com" && i+3 < len(parts) {
				c.AzureOrg = parts[i+1]
				c.AzureProject = parts[i+2]
				if i+4 < len(parts) {
					c.AzureRepo = parts[i+4]
				}
				break
			}
		}
	} else if strings.Contains(remote, "visualstudio.com") {
		c.IsAzureDevOps = true
	}
}

// DiffWithLimit returns the full diff between base and source, truncated to maxLines.
func (c *Context) DiffWithLimit(ctx context.Context, base, source string, maxLines int) (string, error) {
	diff, err := c.runner.Run(ctx, "git", "diff", base+"..."+source)
	if err != nil {
		return "", err
	}

	lines := strings.Split(diff, "\n")
	if maxLines > 0 && len(lines) > maxLines {
		diff = strings.Join(lines[:maxLines], "\n")
		diff += fmt.Sprintf("\n\n[diff truncated: %d -> %d lines]", len(lines), maxLines)
	}
	return diff, nil
}

func extractWorkItemID(branch string) string {
	// Look for numeric segment: feature/123-login → 123
	parts := strings.FieldsFunc(branch, func(r rune) bool {
		return r == '/' || r == '-' || r == '_'
	})
	for _, p := range parts {
		if len(p) > 0 && isDigits(p) {
			return p
		}
	}
	return ""
}

func isDigits(s string) bool {
	for _, r := range s {
		if r < '0' || r > '9' {
			return false
		}
	}
	return true
}
