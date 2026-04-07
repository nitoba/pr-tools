# CLI UI Animation Pacing Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Slow the Go CLI title and step animation so it feels more natural without changing frames, transcript shape, or renderer architecture.

**Architecture:** Keep the current single renderer loop in `internal/ui` and replace the hard-coded ticker duration with a named constant. Add a small test that locks the agreed pacing value while keeping the existing frame-rendering tests intact.

**Tech Stack:** Go 1.25, Go stdlib, testify

---

## File Structure

- Modify: `apps/cli-go/internal/ui/ui.go:13-18,266-269`
  Purpose: extract the animation pacing into a named constant and slow the ticker from `110ms` to `170ms`.
- Modify: `apps/cli-go/internal/ui/ui_test.go:1-10,196-224`
  Purpose: add the `time` import if needed, keep frame tests intact, and add a focused assertion for the pacing constant.

## Chunk 1: Animation Pace

### Task 1: Slow the shared UI ticker with TDD

**Files:**
- Modify: `apps/cli-go/internal/ui/ui.go:13-18,266-269`
- Modify: `apps/cli-go/internal/ui/ui_test.go:1-10,196-224`

- [ ] **Step 1: Write the failing pacing test**

Add this test near the existing frame-rendering tests in `apps/cli-go/internal/ui/ui_test.go`:

```go
func TestAnimationIntervalIsNatural(t *testing.T) {
  require.Equal(t, 170*time.Millisecond, animationInterval)
}
```

- [ ] **Step 2: Run the targeted UI test to confirm RED**

Run from `apps/cli-go`:

```bash
go test ./internal/ui -run 'TestAnimationIntervalIsNatural' -count=1
```

Expected: FAIL because the renderer still uses a hard-coded `110 * time.Millisecond` and no `animationInterval` constant exists yet.

- [ ] **Step 3: Implement the minimal pacing change**

In `apps/cli-go/internal/ui/ui.go`, add a named constant near the top of the file and use it in the ticker:

```go
const animationInterval = 170 * time.Millisecond
```

Then replace:

```go
ticker := time.NewTicker(110 * time.Millisecond)
```

with:

```go
ticker := time.NewTicker(animationInterval)
```

Do not change:

- the single-loop architecture
- title frame order `✦`, `✧`, `✦`, `·`
- step pulse rendering
- transcript output

- [ ] **Step 4: Run the focused UI verification**

Run:

```bash
go test ./internal/ui -run 'TestAnimationIntervalIsNatural|TestRenderTickUsesBashSparkleFramesAndLineOffsets' -count=1
```

Expected: PASS.

- [ ] **Step 5: Run the full UI package tests**

Run:

```bash
go test ./internal/ui -count=1
```

Expected: PASS.
