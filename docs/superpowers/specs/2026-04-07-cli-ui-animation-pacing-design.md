# CLI UI Animation Pacing - Design Spec

## Problem

The Go CLI renderer currently drives the title pulse and active step animation with a single ticker interval of `110ms` in `apps/cli-go/internal/ui/ui.go`.

That cadence makes the UI feel unnaturally fast. The motion reads as nervous instead of calm, especially when the title sparkle and step dot animate together.

## Goal

Make the terminal animation feel more natural by slowing the existing animation loop without changing the visual language.

The desired result is:

- same title sparkle sequence
- same step pulse behavior
- same unified loop
- slower, calmer motion

## Non-Goals

- No redesign of the animation frames
- No separate title and step timing loops
- No new configuration flags
- No transcript or wording changes

## Current Root Cause

`apps/cli-go/internal/ui/ui.go` uses one hard-coded ticker:

- `time.NewTicker(110 * time.Millisecond)`

Because the title and step both repaint on that same cadence, the combined motion is too busy.

## Proposed Change

Keep the current renderer architecture and only slow the base animation interval.

### Renderer Behavior

- Keep the single shared animation loop
- Keep the current frame sequence for the title: `✦`, `✧`, `✦`, `·`
- Keep the current bold/dim pulse behavior for the step dot
- Replace the current `110ms` ticker interval with a calmer interval in the `160-180ms` range
- Use `170ms` as the implementation target unless testing reveals an obvious regression

### Code Shape

In `apps/cli-go/internal/ui/ui.go`:

- extract the ticker duration into a named package-level constant, e.g. `animationInterval`
- use that constant in the animation loop instead of the hard-coded `110ms`

This keeps the timing explicit and easy to tune later without changing renderer logic.

### Testing

In `apps/cli-go/internal/ui/ui_test.go`:

- add a focused assertion that `animationInterval == 170*time.Millisecond`
- keep the existing frame-rendering tests unchanged unless they accidentally depend on the old pace

## Success Criteria

The change is successful when:

- the CLI animation visibly feels slower and more natural
- title and step still animate together correctly
- no transcript formatting changes
- `go test ./internal/ui` still passes
