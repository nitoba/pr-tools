# PRT Go Foundation Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first Go-based foundation of `pr-tools` as a single cross-platform binary named `prt`, with working `init` and `doctor` flows and stubbed `desc`/`test` commands.

**Architecture:** Add a new Go app at `apps/cli-go` using `cobra` for command structure, explicit internal packages for config/setup/doctor/version concerns, and `testify` for tests. Keep the Bash CLI intact during migration while adding a separate Go release path that publishes `prt` artifacts on the same semver tags.

**Tech Stack:** Go, Cobra, Testify, GolangCI-Lint, GoReleaser, GitHub Actions

---

**Spec:** `docs/superpowers/specs/2026-04-06-prt-go-foundation-design.md`

## File Structure

### Create

- `apps/cli-go/go.mod` — Go module definition using `github.com/nitoba/pr-tools/apps/cli-go`
- `apps/cli-go/go.sum` — dependency lockfile
- `apps/cli-go/cmd/prt/main.go` — executable entrypoint and root exit mapping
- `apps/cli-go/cmd/prt/main_test.go` — subprocess-style exit-code tests for `main`
- `apps/cli-go/internal/cli/root.go` — root command construction
- `apps/cli-go/internal/cli/root_test.go` — root command tests
- `apps/cli-go/internal/cli/desc.go` — stub `desc` command
- `apps/cli-go/internal/cli/desc_test.go` — `desc` command tests
- `apps/cli-go/internal/cli/test.go` — stub `test` command
- `apps/cli-go/internal/cli/test_test.go` — `test` command tests
- `apps/cli-go/internal/cli/init.go` — `init` command wiring
- `apps/cli-go/internal/cli/init_test.go` — `init` command tests
- `apps/cli-go/internal/cli/doctor.go` — `doctor` command wiring
- `apps/cli-go/internal/cli/doctor_test.go` — `doctor` command tests
- `apps/cli-go/internal/version/version.go` — version/build metadata defaults and helpers
- `apps/cli-go/internal/version/version_test.go` — version metadata tests
- `apps/cli-go/internal/platform/os.go` — raw runtime facts only: OS, arch, home dir
- `apps/cli-go/internal/platform/os_test.go` — runtime-facts tests
- `apps/cli-go/internal/config/paths.go` — config dir/file resolution from explicit runtime facts
- `apps/cli-go/internal/config/paths_test.go` — path-resolution tests for Linux/macOS/Windows
- `apps/cli-go/internal/config/env.go` — `.env` parsing logic
- `apps/cli-go/internal/config/env_test.go` — `.env` parsing tests
- `apps/cli-go/internal/config/config.go` — config loading and precedence logic
- `apps/cli-go/internal/config/config_test.go` — config loading tests
- `apps/cli-go/internal/setup/bootstrap.go` — managed-block create/update logic for `prt init`
- `apps/cli-go/internal/setup/bootstrap_test.go` — `init` filesystem/idempotency tests
- `apps/cli-go/internal/doctor/doctor.go` — diagnostics engine and blocking matrix
- `apps/cli-go/internal/doctor/doctor_test.go` — `doctor` result tests
- `apps/cli-go/README.md` — local developer instructions for the Go CLI
- `.golangci.yml` — lint rules for the Go CLI
- `.goreleaser.prt.yml` — Go binary packaging config
- `.github/workflows/cli-go-ci.yml` — Go-specific CI workflow

### Modify

- `.github/workflows/release.yml` — attach `prt` artifacts to the existing semver-tag release flow
- `README.md` — add an experimental Go CLI section pointing to `prt` release artifacts without replacing the Bash installer

## Chunk 1: CLI Foundation

### Task 1: Bootstrap the Go module and dependencies

**Files:**
- Create: `apps/cli-go/go.mod`
- Create: `apps/cli-go/README.md`
- Create: `apps/cli-go/internal/version/version_test.go`

- [ ] **Step 1: Create the directory skeleton**

Run from repo root:

```bash
mkdir -p apps/cli-go/cmd/prt apps/cli-go/internal/cli apps/cli-go/internal/version
```

Expected: directories exist for the files created in this task

- [ ] **Step 2: Create the module and dependency manifest**

Create `apps/cli-go/go.mod`:

```go
module github.com/nitoba/pr-tools/apps/cli-go

go 1.24

require (
    github.com/spf13/cobra v1.9.1
    github.com/stretchr/testify v1.10.0
)
```

Create `apps/cli-go/README.md` with a short developer note:

```md
# cli-go

Experimental Go foundation for the future `prt` binary.

## Commands

```bash
go test ./...
go run ./cmd/prt --help
```
```

- [ ] **Step 3: Write the failing module smoke test**

Create `apps/cli-go/internal/version/version_test.go` with:

```go
package version

import "testing"

func TestDefaultsAreStable(t *testing.T) {
    if Version == "" {
        t.Fatal("Version must have a default value")
    }
}
```

- [ ] **Step 4: Run the test to verify it fails**

Run from `apps/cli-go`:

```bash
go test ./internal/version -run TestDefaultsAreStable -v
```

Expected: FAIL with `undefined: Version`

- [ ] **Step 5: Continue directly to Task 2 without committing yet**

Expected: the branch remains intentionally red until Task 2 adds the first passing implementation

### Task 2: Add version metadata and root CLI execution

**Files:**
- Create: `apps/cli-go/cmd/prt/main.go`
- Create: `apps/cli-go/internal/cli/root.go`
- Create: `apps/cli-go/internal/cli/root_test.go`
- Create: `apps/cli-go/internal/version/version.go`
- Modify: `apps/cli-go/internal/version/version_test.go`
- Modify: `apps/cli-go/go.sum`

- [ ] **Step 1: Write the failing root command tests**

Create `apps/cli-go/internal/cli/root_test.go` with:

```go
package cli

import (
    "testing"

    "github.com/stretchr/testify/assert"
    "github.com/stretchr/testify/require"
)

func TestNewRootCommand_HasExpectedMetadata(t *testing.T) {
    cmd := NewRootCommand()

    require.NotNil(t, cmd)
    assert.Equal(t, "prt", cmd.Use)
    assert.Contains(t, cmd.Short, "pr-tools")
    assert.True(t, cmd.SilenceErrors)
    assert.True(t, cmd.SilenceUsage)
}
```

Update `apps/cli-go/internal/version/version_test.go` to expect defaults:

```go
package version

import (
    "testing"

    "github.com/stretchr/testify/assert"
)

func TestDefaultsAreStable(t *testing.T) {
    assert.Equal(t, "dev", Version)
    assert.Equal(t, "unknown", Commit)
    assert.Equal(t, "unknown", Date)
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run from `apps/cli-go`:

```bash
go test ./internal/cli ./internal/version -v
```

Expected: FAIL with `undefined: NewRootCommand` and `undefined: Version`

- [ ] **Step 3: Implement minimal version package**

Create `apps/cli-go/internal/version/version.go`:

```go
package version

var (
    Version = "dev"
    Commit  = "unknown"
    Date    = "unknown"
)
```

- [ ] **Step 4: Implement minimal root command and main entrypoint**

Create `apps/cli-go/internal/cli/root.go`:

```go
package cli

import "github.com/spf13/cobra"

func NewRootCommand() *cobra.Command {
    cmd := &cobra.Command{
        Use:           "prt",
        Short:         "pr-tools Go CLI foundation",
        SilenceErrors: true,
        SilenceUsage:  true,
    }

    return cmd
}
```

Create `apps/cli-go/cmd/prt/main.go`:

```go
package main

import (
    "os"

    "github.com/nitoba/pr-tools/apps/cli-go/internal/cli"
)

func main() {
    if err := cli.NewRootCommand().Execute(); err != nil {
        os.Exit(1)
    }
}
```

- [ ] **Step 5: Normalize module dependencies after imports exist**

Run from `apps/cli-go`:

```bash
go mod tidy
```

Expected: SUCCESS and `go.sum` is created or updated with the actual imported modules

- [ ] **Step 6: Run the tests to verify they pass**

Run from `apps/cli-go`:

```bash
go test ./internal/cli ./internal/version -v
```

Expected: PASS

- [ ] **Step 7: Smoke test the binary entrypoint**

Run from `apps/cli-go`:

```bash
go run ./cmd/prt --help
```

Expected: help output begins with `pr-tools Go CLI foundation`

- [ ] **Step 8: Commit**

Run from repo root:

```bash
git add apps/cli-go/go.mod apps/cli-go/go.sum apps/cli-go/README.md apps/cli-go/cmd/prt/main.go apps/cli-go/internal/cli/root.go apps/cli-go/internal/cli/root_test.go apps/cli-go/internal/version/version.go apps/cli-go/internal/version/version_test.go
git commit -m "feat(cli-go): add prt root command and version metadata"
```

### Task 3: Add stub `desc` and `test` commands with exit code contract

**Files:**
- Create: `apps/cli-go/internal/cli/desc.go`
- Create: `apps/cli-go/internal/cli/desc_test.go`
- Create: `apps/cli-go/internal/cli/test.go`
- Create: `apps/cli-go/internal/cli/test_test.go`
- Modify: `apps/cli-go/internal/cli/root.go`
- Modify: `apps/cli-go/cmd/prt/main.go`
- Create: `apps/cli-go/cmd/prt/main_test.go`

- [ ] **Step 1: Write the failing stub-command tests**

Create `apps/cli-go/internal/cli/desc_test.go`:

```go
package cli

import (
    "bytes"
    "testing"

    "github.com/stretchr/testify/assert"
    "github.com/stretchr/testify/require"
)

func TestDescCommand_IsUnimplemented(t *testing.T) {
    cmd := NewRootCommand()
    buf := new(bytes.Buffer)
    cmd.SetOut(buf)
    cmd.SetErr(buf)
    cmd.SetArgs([]string{"desc"})

    err := cmd.Execute()
    require.Error(t, err)
    assert.Contains(t, err.Error(), "not implemented yet")
}
```

Create `apps/cli-go/internal/cli/test_test.go` with:

```go
package cli

import (
    "bytes"
    "testing"

    "github.com/stretchr/testify/assert"
    "github.com/stretchr/testify/require"
)

func TestTestCommand_IsUnimplemented(t *testing.T) {
    cmd := NewRootCommand()
    buf := new(bytes.Buffer)
    cmd.SetOut(buf)
    cmd.SetErr(buf)
    cmd.SetArgs([]string{"test"})

    err := cmd.Execute()
    require.Error(t, err)
    assert.Contains(t, err.Error(), "not implemented yet")
}
```

Create `apps/cli-go/cmd/prt/main_test.go` with:

```go
package main

import (
    "bytes"
    "testing"

    "github.com/stretchr/testify/require"
)

func TestRun_DescReturnsExitCode2(t *testing.T) {
    stdout := new(bytes.Buffer)
    stderr := new(bytes.Buffer)

    code := run([]string{"desc"}, stdout, stderr)
    require.Equal(t, 2, code)
}

func TestRun_TestReturnsExitCode2(t *testing.T) {
    stdout := new(bytes.Buffer)
    stderr := new(bytes.Buffer)

    code := run([]string{"test"}, stdout, stderr)
    require.Equal(t, 2, code)
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run from `apps/cli-go`:

```bash
go test ./internal/cli ./cmd/prt -run 'Test(DescCommand_IsUnimplemented|TestTestCommand_IsUnimplemented|TestRun_DescReturnsExitCode2|TestRun_TestReturnsExitCode2)' -v
```

Expected: FAIL with `unknown command` and/or missing exit-code mapping

- [ ] **Step 3: Implement typed CLI errors and stub commands**

Update `apps/cli-go/internal/cli/root.go`:

```go
package cli

import (
    "fmt"

    "github.com/spf13/cobra"
)

type ExitError struct {
    Code int
    Err  error
}

func (e *ExitError) Error() string {
    return e.Err.Error()
}

func (e *ExitError) Unwrap() error {
    return e.Err
}

func NewRootCommand() *cobra.Command {
    cmd := &cobra.Command{
        Use:           "prt",
        Short:         "pr-tools Go CLI foundation",
        SilenceErrors: true,
        SilenceUsage:  true,
    }

    cmd.AddCommand(newDescCommand(), newTestCommand())
    return cmd
}

func unimplemented(command string) error {
    return &ExitError{
        Code: 2,
        Err:  fmt.Errorf("prt %s is not implemented yet; see docs/superpowers/specs/2026-04-06-prt-go-foundation-design.md", command),
    }
}
```

Create `apps/cli-go/internal/cli/desc.go`:

```go
package cli

import "github.com/spf13/cobra"

func newDescCommand() *cobra.Command {
    return &cobra.Command{
        Use:   "desc",
        Short: "Generate PR descriptions",
        RunE: func(cmd *cobra.Command, args []string) error {
            return unimplemented("desc")
        },
    }
}
```

Create `apps/cli-go/internal/cli/test.go` with:

```go
package cli

import "github.com/spf13/cobra"

func newTestCommand() *cobra.Command {
    return &cobra.Command{
        Use:   "test",
        Short: "Generate test cards",
        RunE: func(cmd *cobra.Command, args []string) error {
            return unimplemented("test")
        },
    }
}
```

- [ ] **Step 4: Map typed exit errors in `main.go`**

Update `apps/cli-go/cmd/prt/main.go`:

```go
package main

import (
    "errors"
    "fmt"
    "io"
    "os"

    "github.com/nitoba/pr-tools/apps/cli-go/internal/cli"
)

func run(args []string, stdout, stderr io.Writer) int {
    cmd := cli.NewRootCommand()
    cmd.SetOut(stdout)
    cmd.SetErr(stderr)
    cmd.SetArgs(args)

    err := cmd.Execute()
    if err == nil {
        return 0
    }

    var exitErr *cli.ExitError
    if errors.As(err, &exitErr) {
        fmt.Fprintln(stderr, exitErr.Error())
        return exitErr.Code
    }

    fmt.Fprintln(stderr, err)
    return 1
}

func main() {
    os.Exit(run(os.Args[1:], os.Stdout, os.Stderr))
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run from `apps/cli-go`:

```bash
go test ./internal/cli ./cmd/prt -run 'Test(DescCommand_IsUnimplemented|TestTestCommand_IsUnimplemented|TestRun_DescReturnsExitCode2|TestRun_TestReturnsExitCode2)' -v
```

Expected: PASS

- [ ] **Step 6: Build the binary for manual smoke checks**

Run from `apps/cli-go`:

```bash
mkdir -p ./bin && go build -o ./bin/prt ./cmd/prt
```

Expected: SUCCESS and `./bin/prt` exists

- [ ] **Step 7: Verify exit code `2` and message text**

Run from `apps/cli-go`:

```bash
set +e; output="$(./bin/prt desc 2>&1)"; status=$?; set -e; test "$status" -eq 2 && printf '%s' "$output" | grep -F "not implemented yet"
```

Expected: exit code check succeeds and output contains `not implemented yet`

- [ ] **Step 8: Commit**

Run from repo root:

```bash
git add apps/cli-go/cmd/prt/main.go apps/cli-go/cmd/prt/main_test.go apps/cli-go/internal/cli/root.go apps/cli-go/internal/cli/desc.go apps/cli-go/internal/cli/desc_test.go apps/cli-go/internal/cli/test.go apps/cli-go/internal/cli/test_test.go
git commit -m "feat(cli-go): add stub desc and test commands"
```

## Chunk 2: Config, Init, and Doctor

Prerequisite: complete Chunk 1 first so `apps/cli-go` exists, the Go module resolves, and the root CLI wiring is already in place.

### Task 4: Add runtime-facts and config path resolution

**Files:**
- Create: `apps/cli-go/internal/platform/os.go`
- Create: `apps/cli-go/internal/platform/os_test.go`
- Create: `apps/cli-go/internal/config/paths.go`
- Create: `apps/cli-go/internal/config/paths_test.go`

Compatibility rule for this task: milestone 1 intentionally uses `$HOME/.config/pr-tools` semantics on every OS, including Windows, to preserve a single logical config location during migration.

- [ ] **Step 1: Write the failing path-resolution tests**

Create `apps/cli-go/internal/config/paths_test.go`:

```go
package config

import (
    "testing"

    "github.com/nitoba/pr-tools/apps/cli-go/internal/platform"
    "github.com/stretchr/testify/assert"
)

func TestResolvePaths_Linux(t *testing.T) {
    paths := ResolvePaths(platform.Facts{OS: "linux", HomeDir: "/home/nito"})
    assert.Equal(t, "/home/nito/.config/pr-tools", paths.ConfigDir)
    assert.Equal(t, "/home/nito/.config/pr-tools/.env", paths.EnvFile)
}

func TestResolvePaths_Windows(t *testing.T) {
    paths := ResolvePaths(platform.Facts{OS: "windows", HomeDir: `C:\Users\nito`})
    assert.Equal(t, `C:\Users\nito\.config\pr-tools`, paths.ConfigDir)
    assert.Equal(t, `C:\Users\nito\.config\pr-tools\.env`, paths.EnvFile)
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run from `apps/cli-go`:

```bash
go test ./internal/config -run TestResolvePaths -v
```

Expected: FAIL with `undefined: ResolvePaths`

- [ ] **Step 3: Implement runtime facts and path resolver**

Create `apps/cli-go/internal/platform/os_test.go`:

```go
package platform

import "testing"

func TestFacts_AllowsExplicitValues(t *testing.T) {
    _ = Facts{OS: "windows", Arch: "amd64", HomeDir: `C:\Users\nito`}
}
```

Create `apps/cli-go/internal/platform/os.go`:

```go
package platform

import (
    "os"
    "runtime"
)

type Facts struct {
    OS      string
    Arch    string
    HomeDir string
}

func Detect() (Facts, error) {
    home, err := os.UserHomeDir()
    if err != nil {
        return Facts{}, err
    }

    return Facts{OS: runtime.GOOS, Arch: runtime.GOARCH, HomeDir: home}, nil
}
```

Create `apps/cli-go/internal/config/paths.go`:

```go
package config

import (
    "path/filepath"
    "strings"

    "github.com/nitoba/pr-tools/apps/cli-go/internal/platform"
)

type Paths struct {
    ConfigDir string
    EnvFile   string
}

func ResolvePaths(facts platform.Facts) Paths {
    dir := joinForOS(facts.OS, facts.HomeDir, ".config", "pr-tools")
    return Paths{
        ConfigDir: dir,
        EnvFile:   joinForOS(facts.OS, dir, ".env"),
    }
}

func joinForOS(osName string, parts ...string) string {
    joined := filepath.Join(parts...)
    if osName == "windows" {
        return strings.ReplaceAll(joined, "/", `\`)
    }
    return joined
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run from `apps/cli-go`:

```bash
go test ./internal/platform ./internal/config -v
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add apps/cli-go/internal/platform/os.go apps/cli-go/internal/platform/os_test.go apps/cli-go/internal/config/paths.go apps/cli-go/internal/config/paths_test.go
git commit -m "feat(cli-go): add runtime facts and config path resolution"
```

### Task 5: Implement `.env` parsing and config precedence

**Files:**
- Create: `apps/cli-go/internal/config/env.go`
- Create: `apps/cli-go/internal/config/env_test.go`
- Create: `apps/cli-go/internal/config/config.go`
- Create: `apps/cli-go/internal/config/config_test.go`

- [ ] **Step 1: Write the failing parser tests**

Create `apps/cli-go/internal/config/env_test.go`:

```go
package config

import (
    "strings"
    "testing"

    "github.com/stretchr/testify/assert"
    "github.com/stretchr/testify/require"
)

func TestParseEnv_SupportsManagedKeys(t *testing.T) {
    values, issues := ParseEnv(strings.NewReader(`
# comment
export PRT_CONFIG_VERSION=1
PRT_NO_COLOR="false"
PRT_DEBUG='true'
`))

    require.Empty(t, issues)
    assert.Equal(t, "1", values["PRT_CONFIG_VERSION"])
    assert.Equal(t, "false", values["PRT_NO_COLOR"])
    assert.Equal(t, "true", values["PRT_DEBUG"])
}
```

Create `apps/cli-go/internal/config/config_test.go`:

```go
package config

import (
    "strings"
    "testing"

    "github.com/stretchr/testify/assert"
    "github.com/stretchr/testify/require"
)

func boolPtr(v bool) *bool { return &v }

func TestMergePrecedence_ExplicitFalseWins(t *testing.T) {
    cfg := Merge(
        Config{Debug: boolPtr(true)},
        Config{},
        Config{},
        Config{Debug: boolPtr(false)},
    )
    assert.NotNil(t, cfg.Debug)
    assert.False(t, *cfg.Debug)
}

func TestParseEnv_LastDuplicateWins(t *testing.T) {
    values, issues := ParseEnv(strings.NewReader("PRT_DEBUG=false\nPRT_DEBUG=true\n"))
    require.Empty(t, issues)
    assert.Equal(t, "true", values["PRT_DEBUG"])
}

func TestParseEnv_UnknownPRTKeyIsReportedNonBlocking(t *testing.T) {
    _, issues := ParseEnv(strings.NewReader("PRT_FUTURE_KEY=true\n"))
    require.Len(t, issues, 1)
    assert.Equal(t, "PRT_FUTURE_KEY", issues[0].Key)
}

func strPtr(v string) *string { return &v }

func TestLoadFileConfig_IgnoresNonGoOwnedKeys(t *testing.T) {
    cfg, issues := LoadFileConfig(strings.NewReader("AZURE_PAT=secret\nPRT_DEBUG=true\n"))
    require.Empty(t, issues)
    require.NotNil(t, cfg.Debug)
    assert.True(t, *cfg.Debug)
    assert.Nil(t, cfg.ConfigVersion)
}

func TestLoadFileConfig_ParsesManagedKeys(t *testing.T) {
    cfg, issues := LoadFileConfig(strings.NewReader("PRT_NO_COLOR=true\nPRT_DEBUG=false\n"))
    require.Empty(t, issues)
    require.NotNil(t, cfg.NoColor)
    require.NotNil(t, cfg.Debug)
    assert.True(t, *cfg.NoColor)
    assert.False(t, *cfg.Debug)
}

func TestMergePrecedence_DefaultsFileEnvFlags(t *testing.T) {
    version1 := "1"
    version2 := "2"
    version3 := "3"

    cfg := Merge(
        Config{ConfigVersion: &version1, Debug: boolPtr(false)},
        Config{Debug: boolPtr(true)},
        Config{ConfigVersion: &version2},
        Config{ConfigVersion: &version3},
    )

    require.NotNil(t, cfg.ConfigVersion)
    require.NotNil(t, cfg.Debug)
    assert.Equal(t, "3", *cfg.ConfigVersion)
    assert.True(t, *cfg.Debug)
}

func TestLoadEnvConfig_OverridesFileValues(t *testing.T) {
    cfg := LoadEnvConfig(func(key string) (string, bool) {
        switch key {
        case "PRT_DEBUG":
            return "false", true
        case "PRT_CONFIG_VERSION":
            return "9", true
        default:
            return "", false
        }
    })

    require.NotNil(t, cfg.Debug)
    require.NotNil(t, cfg.ConfigVersion)
    assert.False(t, *cfg.Debug)
    assert.Equal(t, "9", *cfg.ConfigVersion)
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run from `apps/cli-go`:

```bash
go test ./internal/config -run 'Test(ParseEnv_SupportsManagedKeys|TestMergePrecedence_ExplicitFalseWins|TestParseEnv_LastDuplicateWins|TestParseEnv_UnknownPRTKeyIsReportedNonBlocking|TestLoadFileConfig_IgnoresNonGoOwnedKeys|TestLoadFileConfig_ParsesManagedKeys|TestMergePrecedence_DefaultsFileEnvFlags|TestLoadEnvConfig_OverridesFileValues)' -v
```

Expected: FAIL with `undefined: ParseEnv`, `undefined: Merge`, or unresolved optional-value helpers

- [ ] **Step 3: Implement env parser and config types**

Create `apps/cli-go/internal/config/env.go` with line-by-line parsing for:

```go
package config

import "io"

type Issue struct {
    Line    int
    Message string
    Key     string
}

func ParseEnv(r io.Reader) (map[string]string, []Issue) {
    // Accept blank lines, # comments, KEY=value, export KEY=value,
    // single/double/unquoted values, last valid key wins.
}
```

Implementation rules:

- only `PRT_CONFIG_VERSION`, `PRT_NO_COLOR`, and `PRT_DEBUG` are Go-owned keys in milestone 1
- `PRT_NO_COLOR` and `PRT_DEBUG` accept only `true` or `false`
- malformed Go-owned lines produce an `Issue` with the owning key in `Issue.Key`
- malformed unknown lines do not produce blocking issues and do not need to be preserved by `ParseEnv`
- unknown `PRT_*` keys produce non-blocking issues with the unknown key name recorded in `Issue.Key`

Create `apps/cli-go/internal/config/config.go`:

```go
package config

import "io"

type Config struct {
    ConfigVersion *string
    NoColor       *bool
    Debug         *bool
}

func Defaults() Config {
    version := "1"
    noColor := false
    debug := false
    return Config{
        ConfigVersion: &version,
        NoColor:       &noColor,
        Debug:         &debug,
    }
}

func Merge(defaults, fileCfg, envCfg, flagCfg Config) Config {
    cfg := defaults
    // apply file -> env -> flag in order, only overriding non-nil fields
    return cfg
}

func LoadFileConfig(r io.Reader) (Config, []Issue) {
    // Parse env values and map only PRT_* keys into Config.
}

func LoadEnvConfig(lookupEnv func(string) (string, bool)) Config {
    // Read only the Go-owned PRT_* keys from process environment.
}
```

Merge rules:

- apply values in this order: defaults, file, environment, flags
- nil field means `unset` and must not override an earlier value
- explicit `false` must override an earlier `true`
- `LoadFileConfig` maps only the Go-owned key subset into `Config`
- `LoadEnvConfig` maps only the Go-owned key subset into `Config`

- [ ] **Step 4: Run the tests to verify they pass**

Run from `apps/cli-go`:

```bash
go test ./internal/config -v
```

Expected: PASS

- [ ] **Step 5: Commit**

Run from repo root:

```bash
git add apps/cli-go/internal/config/env.go apps/cli-go/internal/config/env_test.go apps/cli-go/internal/config/config.go apps/cli-go/internal/config/config_test.go
git commit -m "feat(cli-go): add env parsing and config precedence"
```

### Task 6: Implement `prt init` with managed-block writes

**Files:**
- Create: `apps/cli-go/internal/setup/bootstrap.go`
- Create: `apps/cli-go/internal/setup/bootstrap_test.go`
- Create: `apps/cli-go/internal/cli/init.go`
- Create: `apps/cli-go/internal/cli/init_test.go`
- Modify: `apps/cli-go/internal/cli/root.go`

- [ ] **Step 1: Write the failing managed-block tests**

Create `apps/cli-go/internal/setup/bootstrap_test.go`:

```go
package setup

import (
    "os"
    "path/filepath"
    "testing"

    "github.com/stretchr/testify/assert"
    "github.com/stretchr/testify/require"
)

func TestEnsureEnvFile_CreatesManagedBlockOnce(t *testing.T) {
    dir := t.TempDir()
    envFile := filepath.Join(dir, ".env")

    result, err := EnsureEnvFile(envFile)
    require.NoError(t, err)
    assert.Equal(t, ResultCreatedEnvFile, result)

    contents, err := os.ReadFile(envFile)
    require.NoError(t, err)
    assert.Contains(t, string(contents), "# --- PRT managed block start ---")
    assert.Contains(t, string(contents), "PRT_CONFIG_VERSION=1")
}

func TestEnsureEnvFile_IsIdempotent(t *testing.T) {
    dir := t.TempDir()
    envFile := filepath.Join(dir, ".env")

    _, err := EnsureEnvFile(envFile)
    require.NoError(t, err)

    result, err := EnsureEnvFile(envFile)
    require.NoError(t, err)
    assert.Equal(t, ResultAlreadyUpToDate, result)
}

func TestEnsureEnvFile_FailsOnMultipleManagedBlocks(t *testing.T) {
    dir := t.TempDir()
    envFile := filepath.Join(dir, ".env")
    require.NoError(t, os.WriteFile(envFile, []byte(BlockStart+"\n"+BlockEnd+"\n"+BlockStart+"\n"+BlockEnd+"\n"), 0o600))

    _, err := EnsureEnvFile(envFile)
    require.Error(t, err)
}

func TestEnsureEnvFile_FailsOnUnmatchedMarkers(t *testing.T) {
    dir := t.TempDir()
    envFile := filepath.Join(dir, ".env")
    require.NoError(t, os.WriteFile(envFile, []byte(BlockStart+"\nPRT_DEBUG=false\n"), 0o600))

    _, err := EnsureEnvFile(envFile)
    require.Error(t, err)
}
```

Create `apps/cli-go/internal/cli/init_test.go`:

```go
package cli

import (
    "bytes"
    "testing"

    "github.com/stretchr/testify/assert"
    "github.com/stretchr/testify/require"
)

func TestInitCommand_PrintsExactSummary(t *testing.T) {
    cmd := newInitCommand(InitDependencies{
        Run: func() (InitResult, error) {
            return InitResult{Summary: "created env file"}, nil
        },
    })
    buf := new(bytes.Buffer)
    cmd.SetOut(buf)
    cmd.SetErr(buf)

    err := cmd.Execute()
    require.NoError(t, err)
    assert.Equal(t, "created env file\n", buf.String())
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run from `apps/cli-go`:

```bash
go test ./internal/setup ./internal/cli -run 'Test(EnsureEnvFile_CreatesManagedBlockOnce|TestEnsureEnvFile_IsIdempotent|TestEnsureEnvFile_FailsOnMultipleManagedBlocks|TestEnsureEnvFile_FailsOnUnmatchedMarkers|TestInitCommand_PrintsExactSummary)' -v
```

Expected: FAIL with `undefined: EnsureEnvFile` or missing init command dependencies

- [ ] **Step 3: Implement bootstrap writer**

Create `apps/cli-go/internal/setup/bootstrap.go` with:

```go
package setup

const (
    BlockStart = "# --- PRT managed block start ---"
    BlockEnd   = "# --- PRT managed block end ---"
)

type EnsureEnvResult string

const (
    ResultCreatedEnvFile      EnsureEnvResult = "created env file"
    ResultUpdatedManagedBlock EnsureEnvResult = "updated managed block"
    ResultAlreadyUpToDate     EnsureEnvResult = "config already up to date"
)

func EnsureEnvFile(path string) (EnsureEnvResult, error) {
    // Create file when missing.
    // Preserve unknown lines verbatim.
    // Preserve relative order of non-Go-owned lines.
    // Append one managed block when missing.
    // Replace the contents of a single valid managed block in place.
    // Fail on multiple or malformed managed blocks.
}
```

Managed-block rules:

- managed block must contain exactly `PRT_CONFIG_VERSION=1`, `PRT_NO_COLOR=false`, and `PRT_DEBUG=false`
- if a valid managed block exists, replace only the block contents and keep surrounding lines untouched
- `EnsureEnvFile` returns one of the three exact result constants shown above
- the `init` runner combines the directory-created flag with `EnsureEnvFile` result to print one of these exact summaries:
  - `created config dir and env file`
  - `created env file`
  - `updated managed block`
  - `config already up to date`

- [ ] **Step 4: Implement `init` command wiring**

Create `apps/cli-go/internal/cli/init.go` around injected dependencies:

```go
package cli

import "github.com/spf13/cobra"

type InitResult struct {
    Summary string
}

type InitDependencies struct {
    Run func() (InitResult, error)
}

func newInitCommand(deps InitDependencies) *cobra.Command {
    return &cobra.Command{
        Use:   "init",
        Short: "Create or update prt config",
        RunE: func(cmd *cobra.Command, args []string) error {
            result, err := deps.Run()
            if err != nil {
                return &ExitError{Code: 1, Err: err}
            }
            cmd.Println(result.Summary)
            return nil
        },
    }
}
```

Wire `newInitCommand(...)` into `NewRootCommand()` from `apps/cli-go/internal/cli/init.go` using a small package-local constructor helper.

The real dependency runner for `init` must live in `apps/cli-go/internal/cli/init.go` and must:

- call `platform.Detect()`
- call `config.ResolvePaths(...)`
- ensure the config directory exists and track whether it had to be created
- call `setup.EnsureEnvFile(...)`
- combine the directory-created flag plus `EnsureEnvFile` result into the exact summary string expected by `TestInitCommand_PrintsExactSummary`
- return `InitResult{Summary: ...}` instead of a positional string

- [ ] **Step 5: Run the tests to verify they pass**

Run from `apps/cli-go`:

```bash
go test ./internal/setup ./internal/cli -run 'Test(EnsureEnvFile_CreatesManagedBlockOnce|TestEnsureEnvFile_IsIdempotent|TestEnsureEnvFile_FailsOnMultipleManagedBlocks|TestEnsureEnvFile_FailsOnUnmatchedMarkers|TestInitCommand_PrintsExactSummary)' -v
```

Expected: PASS

- [ ] **Step 6: Run the full package tests**

Run from `apps/cli-go`:

```bash
go test ./internal/setup ./internal/cli -v
```

Expected: PASS

- [ ] **Step 7: Commit**

Run from repo root:

```bash
git add apps/cli-go/internal/setup/bootstrap.go apps/cli-go/internal/setup/bootstrap_test.go apps/cli-go/internal/cli/init.go apps/cli-go/internal/cli/init_test.go apps/cli-go/internal/cli/root.go
git commit -m "feat(cli-go): add init command and managed env bootstrap"
```

### Task 7: Implement `prt doctor` diagnostics and blocking matrix

**Files:**
- Create: `apps/cli-go/internal/doctor/doctor.go`
- Create: `apps/cli-go/internal/doctor/doctor_test.go`
- Create: `apps/cli-go/internal/cli/doctor.go`
- Create: `apps/cli-go/internal/cli/doctor_test.go`
- Modify: `apps/cli-go/internal/cli/root.go`

- [ ] **Step 1: Write the failing diagnostics tests**

Create `apps/cli-go/internal/doctor/doctor_test.go`:

```go
package doctor

import (
    "strings"
    "testing"

    "github.com/stretchr/testify/assert"
    "github.com/stretchr/testify/require"
)

func TestEvaluate_MissingEnvFileIsNonBlocking(t *testing.T) {
    report := Evaluate(Input{EnvFileExists: false, ConfigDirCreatable: true})
    assert.False(t, report.Blocking)
}

func TestEvaluate_InvalidGoOwnedSyntaxIsBlocking(t *testing.T) {
    report := Evaluate(Input{GoOwnedParseIssues: 1})
    assert.True(t, report.Blocking)
}

func TestEvaluate_OrdersLinesBySection(t *testing.T) {
    report := Evaluate(Input{ConfigDirCreatable: true, EnvFileExists: false})
    require.GreaterOrEqual(t, len(report.Lines), 5)
    assert.Contains(t, report.Lines[0], "config dir")
    assert.Contains(t, report.Lines[1], "env file")
    assert.Contains(t, report.Lines[2], "parse")
    assert.Contains(t, report.Lines[3], "version")
    assert.Contains(t, report.Lines[4], "runtime")
}

func TestEvaluate_UnknownPRTKeyProducesWarningLine(t *testing.T) {
    report := Evaluate(Input{UnknownPRTKeys: []string{"PRT_FUTURE_KEY"}})
    assert.Contains(t, strings.Join(report.Lines, "\n"), "PRT_FUTURE_KEY")
    assert.False(t, report.Blocking)
}
```

Create `apps/cli-go/internal/cli/doctor_test.go` with:

```go
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

    err := cmd.Execute()
    require.Error(t, err)
    exitErr := new(ExitError)
    require.ErrorAs(t, err, &exitErr)
    assert.Equal(t, 1, exitErr.Code)
}

func TestDoctorCommand_PrintsNonBlockingReport(t *testing.T) {
    buf := new(bytes.Buffer)
    cmd := newDoctorCommand(DoctorDependencies{
        Run: func() (doctor.Report, error) {
            return doctor.Report{Lines: []string{"[OK] config dir is creatable", "[WARN] .env missing"}, Blocking: false}, nil
        },
    })
    cmd.SetOut(buf)
    cmd.SetErr(buf)

    err := cmd.Execute()
    require.NoError(t, err)
    assert.Contains(t, buf.String(), "[OK] config dir is creatable")
    assert.Contains(t, buf.String(), "[WARN] .env missing")
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run from `apps/cli-go`:

```bash
go test ./internal/doctor ./internal/cli -run 'Test(Evaluate_MissingEnvFileIsNonBlocking|Evaluate_InvalidGoOwnedSyntaxIsBlocking|Evaluate_OrdersLinesBySection|Evaluate_UnknownPRTKeyProducesWarningLine|DoctorCommand_ReturnsExitErrorOnBlockingReport|DoctorCommand_PrintsNonBlockingReport)' -v
```

Expected: FAIL with `undefined: Evaluate`

- [ ] **Step 3: Implement doctor engine**

Create `apps/cli-go/internal/doctor/doctor.go`:

```go
package doctor

type Input struct {
    ConfigDirExists    bool
    ConfigDirCreatable bool
    EnvFileExists      bool
    EnvFileReadable    bool
    GoOwnedParseIssues int
    UnknownPRTKeys     []string
    Version            string
    Commit             string
    Date               string
    OS                 string
    Arch               string
}

type Report struct {
    Blocking bool
    Lines    []string
}

func Evaluate(in Input) Report {
    report := Report{}
    // Blocking rules:
    // - missing config dir but creatable => non-blocking
    // - missing .env => non-blocking
    // - unreadable .env => blocking
    // - invalid syntax affecting Go-owned keys => blocking
    // - malformed or unknown non-Go-owned lines => non-blocking
    // - missing ldflags metadata in dev builds => non-blocking
    return report
}
```

Output format rules:

- success lines start with `[OK]`
- non-blocking issues start with `[WARN]`
- blocking issues start with `[ERR]`
- line order must be stable: config dir, env file, parse status, version/build metadata, runtime facts

- [ ] **Step 4: Implement `doctor` command wiring**

Create `apps/cli-go/internal/cli/doctor.go` with injected dependencies:

```go
package cli

import (
    "fmt"

    "github.com/nitoba/pr-tools/apps/cli-go/internal/doctor"
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
```

Wire `newDoctorCommand(...)` into `NewRootCommand()` from `apps/cli-go/internal/cli/doctor.go` using a small package-local constructor helper.

The real dependency runner for `doctor` must live in `apps/cli-go/internal/cli/doctor.go` and must:

- call `platform.Detect()` to get OS, arch, and home dir
- call `config.ResolvePaths(...)` to resolve config dir and `.env`
- check whether the config dir exists or whether its parent is writable enough to treat it as creatable
- read and parse `.env` when present
- count only Go-owned parse issues as blocking input to `doctor.Evaluate(...)`
- pass version and runtime facts into `doctor.Input`
- format the final lines returned by `doctor.Evaluate(...)` for the CLI

- [ ] **Step 5: Run the tests to verify they pass**

Run from `apps/cli-go`:

```bash
go test ./internal/doctor ./internal/cli -v
```

Expected: PASS

- [ ] **Step 6: Run doctor exit-behavior tests and a clean smoke check**

Run from `apps/cli-go`:

```bash
go test ./internal/doctor ./internal/cli -run 'Test(DoctorCommand_ReturnsExitErrorOnBlockingReport|DoctorCommand_PrintsNonBlockingReport|Evaluate_MissingEnvFileIsNonBlocking|Evaluate_InvalidGoOwnedSyntaxIsBlocking|Evaluate_OrdersLinesBySection|Evaluate_UnknownPRTKeyProducesWarningLine)' -v && mkdir -p ./bin && go build -o ./bin/prt ./cmd/prt && tmp_home="$(mktemp -d)" && HOME="$tmp_home" ./bin/prt doctor
```

Expected: tests PASS and `./bin/prt doctor` exits `0` against the temporary clean home directory

- [ ] **Step 7: Commit**

Run from repo root:

```bash
git add apps/cli-go/internal/doctor/doctor.go apps/cli-go/internal/doctor/doctor_test.go apps/cli-go/internal/cli/doctor.go apps/cli-go/internal/cli/doctor_test.go apps/cli-go/internal/cli/root.go
git commit -m "feat(cli-go): add doctor diagnostics and exit rules"
```

## Chunk 3: Automation, Release, and Docs

Prerequisite: complete Chunks 1 and 2 first. This chunk assumes `apps/cli-go/go.mod`, `apps/cli-go/cmd/prt/main.go`, `apps/cli-go/internal/version/version.go`, and the `prt init|doctor|desc|test` commands already exist and pass local Go tests.

### Task 8: Add Go CI workflow

**Files:**
- Create: `.golangci.yml`
- Create: `.github/workflows/cli-go-ci.yml`

- [ ] **Step 1: Write the workflow file**

Create `.golangci.yml`:

```yaml
linters:
  enable:
    - errcheck
    - gosimple
    - govet
    - ineffassign
    - staticcheck
    - unused
```

Create `.github/workflows/cli-go-ci.yml`:

```yaml
name: CLI Go CI

on:
  pull_request:
    branches: [main]
    paths:
      - "apps/cli-go/**"
      - ".golangci.yml"
      - ".goreleaser.prt.yml"
      - ".github/workflows/cli-go-ci.yml"
  push:
    branches: [main]
    paths:
      - "apps/cli-go/**"
      - ".golangci.yml"
      - ".goreleaser.prt.yml"
      - ".github/workflows/cli-go-ci.yml"

jobs:
  test:
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: apps/cli-go
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-go@v5
        with:
          go-version-file: apps/cli-go/go.mod
      - uses: golangci/golangci-lint-action@v6
        with:
          version: latest
          working-directory: apps/cli-go
      - run: go vet ./...
      - run: go test -race ./...
      - run: go test ./...
      - run: go build ./cmd/prt
      - run: go run ./cmd/prt --help
```

- [ ] **Step 2: Review the workflow contents locally**

Run from repo root:

```bash
sed -n '1,240p' .golangci.yml .github/workflows/cli-go-ci.yml
```

Expected: file contents show the lint config and the `CLI Go CI` workflow with the intended triggers and `apps/cli-go` working directory

- [ ] **Step 3: Run the local Go checks that the workflow will run**

Run from `apps/cli-go`:

```bash
go run github.com/golangci/golangci-lint/cmd/golangci-lint@latest run && go vet ./... && go test -race ./... && go test ./... && go build ./cmd/prt && go run ./cmd/prt --help
```

Expected: SUCCESS

- [ ] **Step 4: Commit**

Run from repo root:

```bash
git add .golangci.yml .github/workflows/cli-go-ci.yml
git commit -m "ci(cli-go): add Go test and build workflow"
```

### Task 9: Add GoReleaser config and integrate tag releases

**Files:**
- Create: `.goreleaser.prt.yml`
- Modify: `.github/workflows/release.yml`

- [ ] **Step 1: Write the GoReleaser config**

Create `.goreleaser.prt.yml`:

```yaml
version: 2

project_name: prt

before:
  hooks:
    - sh -c 'cd apps/cli-go && go vet ./...'
    - sh -c 'cd apps/cli-go && go test -race ./...'
    - sh -c 'cd apps/cli-go && go test ./...'

builds:
  - id: prt
    dir: apps/cli-go
    main: ./cmd/prt
    binary: prt
    ldflags:
      - -s -w -X github.com/nitoba/pr-tools/apps/cli-go/internal/version.Version={{ .Version }} -X github.com/nitoba/pr-tools/apps/cli-go/internal/version.Commit={{ .Commit }} -X github.com/nitoba/pr-tools/apps/cli-go/internal/version.Date={{ .Date }}
    goos: [linux, darwin, windows]
    goarch: [amd64, arm64]

archives:
  - id: prt-archives
    builds: [prt]
    name_template: "prt_{{ .Version }}_{{ .Os }}_{{ .Arch }}"
    formats:
      - tar.gz
    format_overrides:
      - goos: windows
        format: zip

checksum:
  name_template: prt_{{ .Version }}_checksums.txt

release:
  disable: false
```

- [ ] **Step 2: Update the release workflow to build Go artifacts and upload everything with `softprops`**

Modify `.github/workflows/release.yml` so that after the Bash asset packaging it also:

```yaml
      - name: Setup Go
        uses: actions/setup-go@v5
        with:
          go-version-file: apps/cli-go/go.mod

      - name: Install GoReleaser
        uses: goreleaser/goreleaser-action@v6
        with:
          version: latest
          install-only: true

      - name: Build prt release artifacts
        uses: goreleaser/goreleaser-action@v6
        with:
          distribution: goreleaser
          version: latest
          args: release --clean --skip=publish --config .goreleaser.prt.yml

      - name: Collect prt asset list
        run: |
          shopt -s nullglob
          assets=(dist/*.tar.gz dist/*.zip dist/prt_*_checksums.txt)
          test ${#assets[@]} -gt 0
          printf 'PRT_ASSETS<<EOF\n' >> "$GITHUB_ENV"
          printf '%s\n' "${assets[@]}" >> "$GITHUB_ENV"
          printf 'EOF\n' >> "$GITHUB_ENV"
```

and update the existing `softprops/action-gh-release@v2` step so `files:` includes both the Bash zip and the Go assets:

```yaml
          files: |
            ${{ env.DIST_FILE }}
            ${{ env.PRT_ASSETS }}
```

- [ ] **Step 3: Run the dry-run packaging check locally**

Run from repo root:

```bash
go version
```

Expected: Go toolchain is available before relying on CI

Run from repo root:

```bash
go run github.com/goreleaser/goreleaser/v2@latest release --snapshot --clean --skip=publish --config .goreleaser.prt.yml
```

Expected: archives and checksum file are generated locally

- [ ] **Step 4: Validate the release asset selection logic locally**

Run from repo root after the snapshot build:

```bash
shopt -s nullglob && assets=(dist/*.tar.gz dist/*.zip dist/prt_*_checksums.txt) && test ${#assets[@]} -gt 0 && printf '%s\n' "${assets[@]}"
```

Expected: output lists only uploadable archive and checksum files, not directories

- [ ] **Step 5: Review the release workflow edit locally**

Run from repo root:

```bash
sed -n '1,260p' .github/workflows/release.yml
```

Expected: the workflow includes the Go setup, GoReleaser build, deterministic asset collection, and the updated `softprops` `files:` block

- [ ] **Step 6: Verify the host smoke test locally**

Run from `apps/cli-go`:

```bash
go build -ldflags "-X github.com/nitoba/pr-tools/apps/cli-go/internal/version.Version=v0.0.0-dev -X github.com/nitoba/pr-tools/apps/cli-go/internal/version.Commit=local -X github.com/nitoba/pr-tools/apps/cli-go/internal/version.Date=2026-04-06" -o ./bin/prt ./cmd/prt && ./bin/prt --help
```

Expected: SUCCESS and help output prints

- [ ] **Step 7: Commit**

Run from repo root:

```bash
git add .goreleaser.prt.yml .github/workflows/release.yml
git commit -m "ci(release): publish prt binaries with goreleaser"
```

### Task 10: Document the Go CLI without replacing the Bash installer

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Add a focused README section for `prt`**

Append a short section to `README.md` like:

```md
## CLI Go experimental: `prt`

O projeto agora também possui uma fundação em Go para a futura CLI unificada.

Comandos atuais:

```bash
prt init
prt doctor
prt desc
prt test
```

Instalação nesta fase:

- baixar o arquivo `.tar.gz` ou `.zip` correspondente na página de Releases
- extrair o binário `prt` ou `prt.exe`
- colocá-lo no seu `PATH`

Observação: o instalador `install.sh` continua sendo o fluxo oficial da CLI Bash durante a migração.
Fluxo oficial Bash mantido: `bash install.sh`.
Procure artefatos com nomes como `prt_<version>_<os>_<arch>.tar.gz` e `prt_<version>_<os>_<arch>.zip`.
```

- [ ] **Step 2: Review README language for migration clarity**

Read the updated section and confirm it does all of the following:

- keeps Bash as the official current install path
- introduces `prt` as additive, not replacement
- explains the artifact-install flow clearly

- [ ] **Step 3: Run the minimal verification set**

Run from `apps/cli-go`:

```bash
go run github.com/golangci/golangci-lint/cmd/golangci-lint@latest run && go vet ./... && go test -race ./... && go test ./... && go run ./cmd/prt init --help && go run ./cmd/prt doctor --help && go run ./cmd/prt desc --help && go run ./cmd/prt test --help
```

Expected: SUCCESS

- [ ] **Step 4: Commit**

Run from repo root:

```bash
git add README.md
git commit -m "docs: add experimental prt CLI guidance"
```

## Final Verification

- [ ] **Step 1: Run the full Go validation suite**

Run from `apps/cli-go`:

```bash
go run github.com/golangci/golangci-lint/cmd/golangci-lint@latest run && go vet ./... && go test -race ./... && go test ./...
```

Expected: PASS

- [ ] **Step 2: Run the binary smoke checks**

Run from `apps/cli-go`:

```bash
tmp_home="$(mktemp -d)" && mkdir -p ./bin && go build -o ./bin/prt ./cmd/prt && ./bin/prt --help && HOME="$tmp_home" ./bin/prt doctor
```

Expected: `--help` succeeds and `doctor` exits `0` against the temporary clean home directory
Because a missing config dir and a missing `.env` are non-blocking in milestone 1.

- [ ] **Step 3: Verify the worktree is clean after the task-level commits**

Run from repo root:

```bash
rm -rf apps/cli-go/bin dist && git status --short
```

Expected: no output

- [ ] **Step 4: Record the final verification results in the PR/branch summary**

Summarize:

- `go test ./...` result
- `go build ./cmd/prt` result
- `./bin/prt --help` result
- `./bin/prt doctor` result
