# CLI UI Desc Polish - Design Spec

## Problem

The current Go CLI still has three UX gaps in the `prt desc` flow even after the Bash-parity port:

- the active step dot pulses too neutrally instead of using the yellow emphasis from the Bash UI
- the generated title/description block is printed flush-left, visually detached from the surrounding titled tree
- the Azure DevOps publication prompts do not show the expected input affordance, so the user is not told what pressing Enter does

## Goal

Polish the `prt desc` terminal experience so it feels closer to the Bash `v2.9.8` interaction model without redesigning the renderer.

The desired result is:

- the active `●` reads as a yellow pulse
- the generated title/body align visually with the titled output block instead of jumping to the left edge
- publication prompts clearly communicate accepted input and Enter behavior

## Non-Goals

- No full renderer rewrite
- No change to title sparkle frame order
- No change to the `prt test` transcript in this task
- No change to PR creation behavior beyond prompt wording and layout

## Current Root Cause

### Active Step Dot

In `apps/cli-go/internal/ui/ui.go`, `renderTick(...)` only changes the step dot between `Bold` and `Dim`. It does not use the explicit yellow pulse that the Bash UI used for the active step.

### Generated Output Block

In `apps/cli-go/internal/cli/desc.go`, the generated output is emitted as:

- `Titulo: ...`
- `Descricao:`
- raw body text

directly to `stdout`, outside the visual tree used by the summary block. That breaks alignment and makes the generated content feel detached from the rest of the transcript.

### Publication Prompts

In `publishDescPRs(...)`, prompts like:

- `Criar PR(s) no Azure DevOps?`
- `Reviewer (email)`

do not show whether Enter cancels, confirms, or preserves defaults.

## Proposed Change

Keep the current structure of the Go CLI and tighten only these three presentation behaviors.

### 1. Active Step Pulse

In `apps/cli-go/internal/ui/ui.go`:

- keep the existing shared ticker and frame cadence
- keep the same step dot glyph `●`
- change the active step pulse styling so the dot uses yellow emphasis, matching the Bash feel more closely
- preserve the existing bold/dim alternation, but make the pulsing dot yellow rather than a neutral text-weight-only blink

### 2. Generated Output Alignment

In `apps/cli-go/internal/cli/desc.go`:

- keep the `PR — <branch>` summary title block
- treat this as an intentional Go-side polish, not as strict Bash parity; the Bash implementation still printed the generated block flush-left on `stdout`
- keep the summary block on `stderr` unchanged
- keep the generated title/body on `stdout`
- in non-raw mode, stop printing the generated block flush-left and instead indent it consistently to the right so it visually belongs to the surrounding titled section
- use simple indentation on `stdout`, not `│` / `└` glyphs there
- expected non-raw shape:

```text
  Titulo: <titulo>

  Descricao:
  <linha 1>
  <linha 2>
```

- every line of the rendered description body should be prefixed with two spaces
- `--raw` remains unchanged and out of scope for this polish

This means the output should read as one coherent terminal section rather than two disconnected regions, while preserving stream ownership (`stderr` for UI/status, `stdout` for generated content).

### 3. Prompt Affordance

In `publishDescPRs(...)`:

- change `Criar PR(s) no Azure DevOps?` to `Criar PR(s) no Azure DevOps? [y/N]`
- make the reviewer prompt explicit about Enter behavior, with conditional wording:
  - if a default reviewer is already resolved for that target: `Reviewer (email) [Enter para manter atual]`
  - if no default reviewer exists: `Reviewer (email) [Enter para deixar vazio]`
- keep the accepted answers unchanged (`y`, `yes`, `s`, `sim`)
- preserve the existing `(cancelado)` path and default reviewer resolution logic

## Testing

Add or update focused transcript tests in `apps/cli-go/internal/cli/desc_test.go` and UI tests in `apps/cli-go/internal/ui/ui_test.go` to cover:

- yellow pulsing step dot output in the render tick
- visual alignment of the generated title/body block under the summary section
- explicit prompt text for publication confirmation and reviewer input

## Success Criteria

The change is successful when:

- the active step dot visibly reads as yellow-pulsing instead of neutral blinking
- the generated PR title/body no longer jump to the far left outside the titled block
- the publication prompts clearly communicate what Enter does
- the `prt desc` transcript remains behaviorally the same aside from these UX improvements
