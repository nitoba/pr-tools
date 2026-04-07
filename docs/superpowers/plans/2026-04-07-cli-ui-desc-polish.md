# CLI UI Desc Polish Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Polish the `prt desc` experience by restoring a yellow active-step pulse, aligning generated output with the surrounding section visually, and making PR publication prompts explicit about input behavior.

**Architecture:** Keep the current renderer and transcript structure. Limit changes to the active step styling in `internal/ui` and the `prt desc` output/prompt formatting in `internal/cli/desc.go`, with transcript tests locking the intended shape.

**Tech Stack:** Go 1.25, Go stdlib, testify

---

## File Structure

- Modify: `apps/cli-go/internal/ui/ui.go:358-392`
  Purpose: change the active step dot pulse from neutral bold/dim to yellow bold/dim while preserving the existing loop and frame order.
- Modify: `apps/cli-go/internal/ui/ui_test.go:221-253`
  Purpose: extend render-tick coverage to assert the yellow pulsing active dot.
- Modify: `apps/cli-go/internal/cli/desc.go:269-289,431-481`
  Purpose: indent the generated title/body on `stdout` and make publication prompts explicit about Enter behavior.
- Modify: `apps/cli-go/internal/cli/desc_test.go:154-383`
  Purpose: lock the polished output alignment and prompt transcript.

## Chunk 1: Desc Polish

### Task 1: Restore the yellow active-step pulse

**Files:**
- Modify: `apps/cli-go/internal/ui/ui.go:358-392`
- Modify: `apps/cli-go/internal/ui/ui_test.go:221-253`

- [ ] **Step 1: Write the failing UI test**

Extend `apps/cli-go/internal/ui/ui_test.go` near `TestRenderTickUsesBashSparkleFramesAndLineOffsets` with assertions that the active `●` uses the yellow palette in both pulse states.

```go
require.Contains(t, frame0, "<yellow><bold>●<reset>")
require.Contains(t, frame1, "<yellow><dim>●<reset>")
```

- [ ] **Step 2: Run the targeted UI test to confirm RED**

Run from `apps/cli-go`:

```bash
go test ./internal/ui -run 'TestRenderTickUsesBashSparkleFramesAndLineOffsets' -count=1
```

Expected: FAIL because `renderTick(...)` currently uses only `Bold` / `Dim` for the step dot.

- [ ] **Step 3: Implement the minimal renderer change**

In `apps/cli-go/internal/ui/ui.go`, update `renderTick(...)` so the active step dot stays yellow while pulsing between bold and dim:

```go
stepStyle := p.Yellow + p.Dim
if frame%2 == 0 {
  stepStyle = p.Yellow + p.Bold
}
```

Keep unchanged:

- ticker pacing
- title sparkle frames
- transcript structure
- shared animation loop

- [ ] **Step 4: Run the focused UI verification**

Run:

```bash
go test ./internal/ui -run 'TestRenderTickUsesBashSparkleFramesAndLineOffsets|TestAnimationIntervalIsNatural' -count=1
```

Expected: PASS.

### Task 2: Align generated output and clarify publish prompts

**Files:**
- Modify: `apps/cli-go/internal/cli/desc.go:269-289,431-481`
- Modify: `apps/cli-go/internal/cli/desc_test.go:154-383`

- [ ] **Step 1: Write the failing transcript tests**

Update `apps/cli-go/internal/cli/desc_test.go` with assertions for the polished output shape.

Add or adjust tests so they require:

```go
require.Contains(t, stdout.String(), "  Titulo: Melhorar login\n\n  Descricao:\n  ## Descrição\n  Body\n")
require.Contains(t, stderr.String(), "│ Criar PR(s) no Azure DevOps? [y/N]")
require.Contains(t, stderr.String(), "│ Reviewer (email) [Enter para manter atual]")
```

Also add a case where no default reviewer exists and assert:

```go
require.Contains(t, stderr.String(), "│ Reviewer (email) [Enter para deixar vazio]")
```

- [ ] **Step 2: Run the targeted desc tests to confirm RED**

Run:

```bash
go test ./internal/cli -run 'TestRunDescSummaryBlockShowsBashRows|TestRunDescPublishTranscriptCancelSuccessAndFailure|TestRunDescStdoutIsPlainWhenOnlyStderrIsInteractive' -count=1
```

Expected: FAIL because `runDesc()` still prints `Titulo:` / `Descricao:` flush-left and the prompts do not include explicit affordance text.

- [ ] **Step 3: Implement the minimal desc transcript change**

In `apps/cli-go/internal/cli/desc.go`:

1. Replace the non-raw `stdout` block:

```go
_, _ = fmt.Fprintf(stdout, "\nTitulo: %s%s%s\n\n", titleColor, title, titleReset)
_, _ = fmt.Fprintf(stdout, "Descricao:\n%s\n", body)
```

with an indented shape that preserves `stdout` ownership but moves the content rightward:

```go
_, _ = fmt.Fprintf(stdout, "\n  Titulo: %s%s%s\n\n", titleColor, title, titleReset)
_, _ = fmt.Fprintln(stdout, "  Descricao:")
for _, line := range strings.Split(body, "\n") {
  if line == "" {
    _, _ = fmt.Fprintln(stdout, "  ")
    continue
  }
  _, _ = fmt.Fprintf(stdout, "  %s\n", line)
}
```

2. Update `publishDescPRs(...)` prompt text:

```go
ui.Info(stderr, "Criar PR(s) no Azure DevOps? [y/N]")
```

3. Make the reviewer prompt conditional:

```go
prompt := "Reviewer (email) [Enter para deixar vazio]"
if reviewer != "" {
  prompt = "Reviewer (email) [Enter para manter atual]"
}
ui.Info(stderr, prompt)
```

Keep unchanged:

- accepted answers (`y`, `yes`, `s`, `sim`)
- `(cancelado)` path
- default reviewer resolution logic
- raw mode behavior

- [ ] **Step 4: Run the focused desc verification**

Run:

```bash
go test ./internal/cli -run 'TestRunDescSummaryBlockShowsBashRows|TestRunDescPublishTranscriptCancelSuccessAndFailure|TestRunDescStdoutIsPlainWhenOnlyStderrIsInteractive' -count=1
```

Expected: PASS.

- [ ] **Step 5: Run the package-level verification**

Run:

```bash
go test ./internal/ui ./internal/cli -count=1
```

Expected: PASS.

- [ ] **Step 6: Run the full Go module verification**

Run from `apps/cli-go`:

```bash
go test ./... -count=1
```

Expected: PASS.
