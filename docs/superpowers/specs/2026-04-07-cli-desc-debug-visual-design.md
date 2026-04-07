# CLI Desc Debug Visual Output - Design Spec

## Problem

`prt desc` currently shows provider-attempt progress inside the visual UI tree, but when all LLM providers fail it falls back to a raw returned error like:

- `LLM call failed: todos os provedores falharam: ...`

That dumps the technical detail outside the styled transcript and breaks the visual consistency of the command.

At the same time, `prt desc` does not expose an explicit `--debug` flag even though the project already supports global debug config via `PRT_DEBUG`, and `prt test` already has a `--debug` mode that prints diagnostics through the same `ui.Info(...)` tree.

## Goal

Make `prt desc` support a proper debug mode and render LLM failure diagnostics inside the same visual UI style as the rest of the command.

The desired result is:

- `prt desc` accepts `--debug`
- debug can also be enabled through `PRT_DEBUG=true`
- when the LLM chain fails, technical details appear inside the styled terminal tree instead of as a raw dumped error block
- non-debug mode stays concise

## Non-Goals

- No change to successful `prt desc` output beyond optional debug information
- No change to `prt test` behavior in this task
- No change to provider retry logic
- No full verbose/trace mode beyond the agreed debug details

## Current Root Cause

### Missing Command-Level Debug Flag

`apps/cli-go/internal/config/config.go` already supports `PRT_DEBUG`, and `apps/cli-go/internal/cli/test.go` already exposes `--debug`, but `apps/cli-go/internal/cli/desc.go` has no equivalent flag.

### Raw Error Escapes the UI Tree

In `apps/cli-go/internal/cli/desc.go`, the LLM failure path currently does this:

- closes the step as failed
- prints `Todos os providers falharam` in the tree
- returns `fmt.Errorf("LLM call failed: %w", err)`

The wrapped fallback error includes per-provider details, and in the current end-to-end execution that detail ends up visible outside the styled transcript. This task should stop returning the full verbose fallback error so higher-level callers no longer surface it as a raw postscript.

## Proposed Change

### 1. Add `--debug` to `prt desc`

In `apps/cli-go/internal/cli/desc.go`:

- add `debug bool` to `descFlagSet`
- register `--debug` on `NewDescCmd(...)`, matching the wording used by `prt test`
- compute debug mode with explicit precedence:
  - if the flag was explicitly provided on the command line, its parsed boolean value wins
  - otherwise, fall back to config debug when `cfg.Debug != nil && *cfg.Debug`

This means:

- `prt desc --debug` enables debug regardless of config
- `prt desc --debug=false` disables debug even if `PRT_DEBUG=true`
- if the flag is omitted, `PRT_DEBUG=true` enables debug

### 2. Keep Normal Mode Concise

In non-debug mode, the LLM failure path should remain compact:

- failed step row
- `Todos os providers falharam`
- short returned error only

The command should no longer return the full aggregated fallback error in normal mode. If a higher-level caller prints returned errors, it must only receive the short form.

### 3. Render Debug Detail Inside the Tree

When debug is active and the LLM chain fails, `prt desc` should print diagnostics using the same styled tree helpers (`ui.Info`, `ui.Warn`, `ui.Error`) before returning.

Required debug detail sections:

- provider/model chosen for preview/configured default
- prompt sizing context useful for LLM failures
  - diff line count already collected
  - prompt length summary or equivalent lightweight size signal
- each provider failure, line by line, inside the tree

Expected behavior:

- multi-line provider errors should be split into lines and rendered as separate tree lines
- no raw multi-line dump should appear outside the visual block in debug mode

### 4. Close the Visual Failure Block Explicitly

In the LLM failure path for `prt desc`:

- the active step must be closed as failed
- `Todos os providers falharam` must be rendered in the tree
- any debug detail lines must be rendered before the title block is closed
- after the debug section is printed, the command must explicitly close the visual block with the same `ui.TitleDone(...)` plus `printDescBlockClose(...)` pattern used in the success path before returning

### 5. Final Returned Error

After rendering debug details in the tree, the returned error from `runDesc()` should be short and non-duplicative, for example equivalent to:

- `LLM call failed`

The technical detail should live in the styled debug block, not be repeated as a raw postscript outside it.

## Testing

Add or update focused tests in `apps/cli-go/internal/cli/desc_test.go` to cover:

- `--debug` flag support for `prt desc`
- debug activation from config (`cfg.Debug != nil && *cfg.Debug`)
- non-debug LLM failure stays concise and does not dump provider-detail text outside the tree
- debug-mode LLM failure prints provider/model plus line-by-line provider errors in the tree
- returned error in debug mode is short and non-duplicative

## Success Criteria

The change is successful when:

- `prt desc --debug` works
- `PRT_DEBUG=true` also enables the same behavior
- LLM failure diagnostics are rendered inside the styled transcript tree
- normal mode stays concise
- no raw verbose provider dump escapes outside the visual UI when debug mode is used
