# CLI Desc Debug Visual Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a real debug mode to `prt desc` and render LLM failure diagnostics inside the same styled UI tree instead of leaking the verbose fallback error as a raw postscript.

**Architecture:** Extend `descFlagSet` and `NewDescCmd(...)` with a command-level `--debug` flag, then compute an effective debug mode from the flag plus `cfg.Debug`. Keep the existing provider retry logic, but on LLM failure print debug details with `ui.Info(...)` before explicitly closing the title block and returning a short error.

**Tech Stack:** Go 1.25, Go stdlib, Cobra, testify

---

## File Structure

- Modify: `apps/cli-go/internal/cli/desc.go:45-55,116-140,235-260`
  Purpose: add `--debug`, compute effective debug mode, render LLM failure diagnostics in the visual tree, and return a short non-duplicative error.
- Modify: `apps/cli-go/internal/cli/desc_test.go:30-41,79-120`
  Purpose: add coverage for `--debug` flag presence and the new debug/non-debug LLM failure behavior.

## Chunk 1: Visual Debug Mode for `prt desc`

### Task 1: Add `--debug` and tree-rendered LLM failure diagnostics

**Files:**
- Modify: `apps/cli-go/internal/cli/desc.go:45-55,116-140,235-260`
- Modify: `apps/cli-go/internal/cli/desc_test.go:30-41,79-120`

- [ ] **Step 1: Write the failing tests first**

Update `apps/cli-go/internal/cli/desc_test.go` with focused tests for the new behavior.

Add at least:

```go
func TestNewDescCmdHasCorrectMetadata(t *testing.T) {
  cfg := &config.Config{}
  cmd := NewDescCmd(cfg)

  require.NotNil(t, cmd.Flags().Lookup("debug"))
}

func TestRunDescLLMFailureIsConciseWithoutDebug(t *testing.T) {
  // debug off, runDescLLM returns aggregated fallback error
  // assert stderr contains "Todos os providers falharam"
  // assert returned error is short (e.g. "LLM call failed")
  // assert provider-detail text is NOT printed in stderr
}

func TestRunDescLLMFailureShowsTreeDiagnosticsWithFlagDebug(t *testing.T) {
  // flags.debug = true
  // assert stderr includes provider/model preview, prompt sizing context,
  // and each provider failure line rendered in the tree
  // assert returned error stays short
}

func TestRunDescLLMFailureShowsTreeDiagnosticsWithConfigDebug(t *testing.T) {
  // cfg.Debug = config.Bool(true), flags.debug left false/unset
  // same assertions as flag-driven debug
}

func TestRunDescLLMFailureFlagFalseOverridesConfigDebug(t *testing.T) {
  // cmd flag explicitly set to --debug=false while cfg.Debug = config.Bool(true)
  // assert stderr stays concise and does NOT print debug detail lines
}
```

For the explicit-flag precedence tests, make sure the Cobra command records the flag as changed. Use the command returned by `NewDescCmd(...)` or set the flag directly before calling `runDesc(...)`, e.g.:

```go
cmd := newDescTestCommand(stdout, stderr, "")
cmd.Flags().Bool("debug", false, "Show diagnostic details")
require.NoError(t, cmd.Flags().Set("debug", "false"))
```

Use a synthetic aggregated fallback error like:

```go
errors.New("todos os provedores falharam:\n  openrouter: no choices in response\n  groq: status 413: request too large")
```

- [ ] **Step 2: Run the targeted desc tests to confirm RED**

Run from `apps/cli-go`:

```bash
go test ./internal/cli -run 'TestNewDescCmdHasCorrectMetadata|TestRunDescDryRunUsesBashTranscript|TestRunDescLLMFailure' -count=1
```

Expected: FAIL because `prt desc` has no `--debug` flag yet and the LLM failure path still returns the full wrapped fallback error.

- [ ] **Step 3: Implement the minimal `desc` debug support**

In `apps/cli-go/internal/cli/desc.go`:

1. Extend `descFlagSet`:

```go
type descFlagSet struct {
  source   string
  targets  []string
  workItem string
  dryRun   bool
  raw      bool
  debug    bool
  ...
}
```

2. Register the flag in `NewDescCmd(...)`:

```go
cmd.Flags().BoolVar(&flags.debug, "debug", false, "Show diagnostic details")
```

3. Compute effective debug mode in `runDesc(...)` with the precedence from the spec. This is mandatory, not optional:

```go
debugEnabled := false
if cmd.Flags().Changed("debug") {
  debugEnabled = flags.debug
} else if cfg.Debug != nil && *cfg.Debug {
  debugEnabled = true
}
```

This must preserve the required behavior:

- `--debug` enables debug
- `--debug=false` disables debug even if `PRT_DEBUG=true`
- omitted flag falls back to `cfg.Debug`

4. In the LLM failure path:

```go
if err != nil {
  stepLLM(false, "Gerando descrição via LLM")
  ui.Error(stderr, "Todos os providers falharam")

  if debugEnabled {
    ui.Info(stderr, fmt.Sprintf("provider/model: %s/%s", configuredProvider, configuredModel))
    ui.Info(stderr, fmt.Sprintf("diff lines: %d", gitCtx.DiffOriginalLines))
    ui.Info(stderr, fmt.Sprintf("prompt chars: %d", len(systemPrompt)+len(userPrompt)))
    for _, line := range strings.Split(err.Error(), "\n") {
      line = strings.TrimSpace(line)
      if line == "" || strings.EqualFold(line, "todos os provedores falharam:") {
        continue
      }
      ui.Info(stderr, line)
    }
  }

  ui.TitleDone(stderr)
  printDescBlockClose(stderr)
  return fmt.Errorf("LLM call failed")
}
```

Implementation notes:

- split multi-line provider errors into separate tree lines
- skip the aggregate banner line `todos os provedores falharam:` because `ui.Error(stderr, "Todos os providers falharam")` was already rendered above
- keep normal mode concise
- do not change retry logic or success path behavior
- do not touch `prt test`

- [ ] **Step 4: Run the targeted desc verification**

Run:

```bash
go test ./internal/cli -run 'TestNewDescCmdHasCorrectMetadata|TestRunDescDryRunUsesBashTranscript|TestRunDescLLMFailure' -count=1
```

Expected: PASS.

- [ ] **Step 5: Run the package-level CLI verification**

Run:

```bash
go test ./internal/cli -count=1
```

Expected: PASS.

- [ ] **Step 6: Run the full Go module verification**

Run:

```bash
go test ./... -count=1
```

Expected: PASS.
