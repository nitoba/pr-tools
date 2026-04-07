# CLI Go UI Parity Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore the `prt desc` and `prt test` terminal experience so it matches the Bash implementation from `v2.9.8` in UI structure, animation, visible text, and message order while keeping the Go command logic intact.

**Architecture:** Rebuild `internal/ui` as a Bash-style stateful renderer, then remap `desc` and `test` to emit the Bash transcript against that renderer. Extend the `prt test` Azure work-item/test-case flow only where the Go port currently lacks behavior required by the old transcript, especially optional work-item resolution and the Test QA field prompts.

**Tech Stack:** Go 1.25, Cobra, Go stdlib terminal I/O, testify

---

## File Structure

- Modify: `apps/cli-go/internal/ui/ui.go:13-146`
  Purpose: replace the current stateless title/spinner helpers with a stateful Bash-parity renderer.
- Create: `apps/cli-go/internal/ui/ui_test.go`
  Purpose: lock the renderer transcript and prevent regressions in title lifecycle, tree layout, and non-interactive output.
- Modify: `apps/cli-go/internal/azure/pr.go:31-108`
  Purpose: expose PR-linked work-item lookups needed for Bash-style parent work-item resolution in `prt test`.
- Modify: `apps/cli-go/internal/cli/desc.go:84-279`
  Purpose: restore Bash `create-pr-description` step order, wording, prompts, and summary blocks.
- Modify: `apps/cli-go/internal/cli/desc_test.go:19-50`
  Purpose: add transcript tests for `runDesc`, keeping the existing parser tests.
- Modify: `apps/cli-go/internal/cli/test.go:34-291`
  Purpose: restore Bash `create-test-card` wording, make `--work-item` optional, and rebuild the two-phase transcript.
- Modify: `apps/cli-go/internal/cli/test_test.go:10-27`
  Purpose: add transcript and resolution-order coverage for `runTest`.
- Modify: `apps/cli-go/internal/azure/testcase.go:12-85`
  Purpose: send the same create payload fields the Bash flow tracked and expose enough data for the fallback summary.
- Modify: `apps/cli-go/internal/azure/workitem.go:45-117`
  Purpose: support the Test QA update flow, including `Effort` and `Custom.RealEffort`.
- Modify: `apps/cli-go/internal/azure/client_test.go:14-106`
  Purpose: extend Azure client coverage for the richer create/update patch payloads.
- Create: `apps/cli-go/internal/azure/workitem_test.go`
  Purpose: add focused tests for the Test QA patch operations and field helpers without overloading `client_test.go`.
- Modify: `apps/cli-go/internal/cli/terminal.go:1-8` only if needed
  Purpose: add a tiny terminal helper for `cmd.InOrStdin()` / `cmd.ErrOrStderr()` driven tests. Skip this if `desc.go` and `test.go` can stay self-contained.

## Chunk 1: UI Core

### Task 1: Lock the Bash renderer contract with failing tests

**Files:**
- Create: `apps/cli-go/internal/ui/ui_test.go`
- Modify: `apps/cli-go/internal/ui/ui.go:13-146`

- [ ] **Step 1: Write the failing renderer tests**

Create `apps/cli-go/internal/ui/ui_test.go` with focused transcript tests that do not depend on real time or a real terminal.

```go
func TestTitleDoesNotEmitLeadingBlankLine(t *testing.T) {
  var buf bytes.Buffer
  resetForTest(false)

  Title(&buf, "Gerando descrição do PR...")

  out := buf.String()
  require.False(t, strings.HasPrefix(out, "\n"))
  require.Contains(t, out, "✦ Gerando descrição do PR...")
}

func TestTitleDoneDoesNotPrintClosingRow(t *testing.T) {
  var buf bytes.Buffer
  resetForTest(false)

  Title(&buf, "Gerando descrição do PR...")
  before := buf.String()
  TitleDone(&buf)

  require.Equal(t, before, buf.String())
}

func TestInfoWarnErrorSuccessUseTitleTree(t *testing.T) {
  var buf bytes.Buffer
  resetForTest(false)

  Title(&buf, "Gerando descrição do PR...")
  Info(&buf, "Contexto git coletado")
  Warn(&buf, "Diff truncado")
  Error(&buf, "Todos os providers falharam")
  Success(&buf, "Descrição gerada")

  out := buf.String()
  require.Contains(t, out, "│ Contexto git coletado")
  require.Contains(t, out, "│ ⚠ Diff truncado")
  require.Contains(t, out, "│ ✗ Todos os providers falharam")
  require.Contains(t, out, "│ ✓ Descrição gerada")
}

func TestStepFailureWithActiveTitleUsesFailureTree(t *testing.T) {
  var buf bytes.Buffer
  resetForTest(false)

  Title(&buf, "Gerando descrição do PR...")
  stop := Step(&buf, "Validando API keys")
  stop(false)

  require.Contains(t, buf.String(), "│ ✗ Validando API keys")
}

func TestStandaloneSuccessHasNoTreeConnector(t *testing.T) {
  var buf bytes.Buffer
  resetForTest(false)

  stop := Step(&buf, "Criando PR → dev")
  stop(true)

  out := buf.String()
  require.Contains(t, out, "✓ Criando PR → dev")
  require.NotContains(t, out, "│")
}
```

- [ ] **Step 2: Run the new UI tests and confirm they fail against the current renderer**

Run from `apps/cli-go`:

```bash
go test ./internal/ui -run 'TestTitleDoesNotEmitLeadingBlankLine|TestTitleDoneDoesNotPrintClosingRow|TestInfoWarnErrorSuccessUseTitleTree' -count=1
```

Expected: FAIL because `Title()` prepends a blank line and `TitleDone()` still prints `│ └`.

- [ ] **Step 3: Add deterministic renderer seams before touching animation**

In `apps/cli-go/internal/ui/ui.go`, introduce a small internal session model and test helpers so the renderer can be tested without sleeping:

```go
type session struct {
  mu              sync.Mutex
  interactive     bool
  colorEnabled    bool
  titleActive     bool
  titleMsg        string
  titleLinesBelow int
  stepActive      bool
  stepMsg         string
}

var current = &session{}

type colorSnapshot struct { /* capture Bold/Dim/Green/Red/Yellow/Cyan/Orange/OrangeLight/OrangeDim/Gray/Reset */ }

var defaultColors = snapshotColorsForTest()

func resetForTest(interactive bool) {
  current = &session{interactive: interactive, colorEnabled: true}
  restoreColorsForTest(defaultColors)
}

func snapshotColorsForTest() colorSnapshot { /* copy Bold/Dim/.../Reset */ }
func restoreColorsForTest(s colorSnapshot) { /* restore Bold/Dim/.../Reset */ }
```

Do not add public API for this. Keep it test-only inside the package.

Also update `Init(w)` so production runs always set both `current.interactive` and `current.colorEnabled`, and define animation eligibility as `current.interactive && current.colorEnabled`.

- [ ] **Step 4: Re-run the targeted UI tests**

Run:

```bash
go test ./internal/ui -run 'TestTitleDoesNotEmitLeadingBlankLine|TestTitleDoneDoesNotPrintClosingRow|TestInfoWarnErrorSuccessUseTitleTree' -count=1
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/cli-go/internal/ui/ui.go apps/cli-go/internal/ui/ui_test.go
git commit -m "test(ui): lock bash title and tree transcript"
```

### Task 2: Implement the Bash-style title + spinner session

**Files:**
- Modify: `apps/cli-go/internal/ui/ui.go:13-146`
- Modify: `apps/cli-go/internal/ui/ui_test.go`
- Modify: `apps/cli-go/internal/cli/desc.go` only at the current `TitleDone()` close site if needed
- Modify: `apps/cli-go/internal/cli/test.go` only at the current `TitleDone()` close site if needed

- [ ] **Step 1: Add failing tests for step rendering and non-interactive degradation**

Extend `apps/cli-go/internal/ui/ui_test.go` with tests that cover the renderer contract that currently does not exist.

```go
func TestStepWithActiveTitleReplacesSpinnerWithTreeSuccess(t *testing.T) {
  var buf bytes.Buffer
  resetForTest(false)

  Title(&buf, "Gerando descrição do PR...")
  stop := Step(&buf, "Validando dependencias")
  stop(true)

  require.Contains(t, buf.String(), "│ ✓ Validando dependencias")
}

func TestStepWithoutTitleUsesStandaloneLayout(t *testing.T) {
  var buf bytes.Buffer
  resetForTest(false)

  stop := Step(&buf, "Criando PR → dev")
  stop(false)

  require.Contains(t, buf.String(), "✗ Criando PR → dev")
  require.NotContains(t, buf.String(), "│")
}

func TestNonInteractiveStepUsesStaticOutput(t *testing.T) {
  var buf bytes.Buffer
  resetForTest(false)
  current.interactive = false

  Title(&buf, "Gerando card de teste...")
  stop := Step(&buf, "Buscando exemplos de test case")
  stop(true)

  out := buf.String()
  require.NotContains(t, out, "\r")
  require.Contains(t, out, "│ ✓ Buscando exemplos de test case")
}

func TestRenderTickUsesBashSparkleFramesAndLineOffsets(t *testing.T) {
  // exercise a pure renderTick(frame int, titleDist int, titleMsg, stepMsg string)
  // helper and assert frames ✦, ✧, ✦, ·, plus save/restore and cursor-up escapes
  // based on titleLinesBelow
}
```

- [ ] **Step 2: Run the step/non-interactive tests and verify they fail**

Run:

```bash
go test ./internal/ui -run 'TestStepWithActiveTitleReplacesSpinnerWithTreeSuccess|TestStepWithoutTitleUsesStandaloneLayout|TestNonInteractiveStepUsesStaticOutput' -count=1
```

Expected: FAIL because the current code always renders the `│` tree and does not maintain Bash title state.

- [ ] **Step 3: Implement the Bash-style renderer loop and line accounting**

Update `apps/cli-go/internal/ui/ui.go` so the step and title share one session-aware loop.

Use this shape:

```go
func Title(w io.Writer, msg string) {
  current.mu.Lock()
  defer current.mu.Unlock()
  current.titleActive = true
  current.titleMsg = msg
  current.titleLinesBelow = 0
  fmt.Fprintf(w, " %s%s✦%s %s%s%s\n", Orange, Bold, Reset, OrangeDim, msg, Reset)
}

func TitleDone(io.Writer) {
  current.mu.Lock()
  defer current.mu.Unlock()
  current.titleActive = false
  current.titleMsg = ""
  current.titleLinesBelow = 0
}

func Step(w io.Writer, msg string) func(bool) {
  // title-aware start, title-aware stop, increment titleLinesBelow on completion
}

func renderTick(frame int, titleDistance int, titleMsg, stepMsg string) string {
  // pure helper returning the full repaint payload, including:
  // current-line clear, optional cursor save, optional cursor-up,
  // title repaint, cursor restore, and step repaint
}
```

Implementation rules:

- `Title()` must not inject a blank line.
- `TitleDone()` only resets state.
- `Info`, `Warn`, `Error`, and `Success` must increment `titleLinesBelow` when `titleActive` is true.
- Interactive mode must use cursor save/restore and cursor-up exactly like the Bash loop.
- The frame order must stay exactly `✦`, `✧`, `✦`, `·` for the title and bold/dim `●` for the active step.
- Non-interactive mode must never use carriage-return repaint.

- [ ] **Step 3.1: Keep callers compiling while `TitleDone()` becomes state-only**

Until Chunks 2 and 4 finish the transcript rewrite, patch the current `TitleDone()` call sites in `apps/cli-go/internal/cli/desc.go` and `apps/cli-go/internal/cli/test.go` to print their own temporary closing row explicitly. Remove or relocate those temporary closers when the later chunks install the final Bash transcript.

- [ ] **Step 4: Run the full UI package tests**

Run:

```bash
go test ./internal/ui -count=1
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/cli-go/internal/ui/ui.go apps/cli-go/internal/ui/ui_test.go apps/cli-go/internal/cli/desc.go apps/cli-go/internal/cli/test.go
git commit -m "feat(ui): restore bash session renderer semantics"
```

## Chunk 2: `prt desc` Transcript Parity

### Task 3: Restore the Bash pre-LLM transcript and work-item fallback

**Files:**
- Modify: `apps/cli-go/internal/cli/desc.go:84-279`
- Modify: `apps/cli-go/internal/cli/desc_test.go:19-50`
- Modify: `apps/cli-go/internal/cli/terminal.go:1-8` only if needed

- [ ] **Step 1: Add failing transcript tests for the Bash step order**

Extend `apps/cli-go/internal/cli/desc_test.go` with transcript-oriented tests that stub git, Azure, and LLM dependencies.

Add minimal seams first if needed:

```go
var newGitContext = func() gitCollector { return git.NewContext(git.ExecRunner{}) }
var newDescFallback = func(cfg llm.Config) descFallback { return llm.NewFallbackClient(cfg) }
var writeClipboard = clipboard.Write
var isInteractiveInput = func(r io.Reader) bool { /* use cmd.InOrStdin() */ }
var newAzureClient = func(pat, org string) descAzureClient { return azure.NewClient(pat, org) }
```

Then add tests like:

```go
func TestRunDesc_DryRunUsesBashTranscript(t *testing.T) {
  // stub git context with branch/work item/sprint/repo data
  // run with --dry-run and capture stdout/stderr
  // assert separator banner, DRY RUN title, [SYSTEM], [USER], and provider/model line
}

func TestRunDesc_PromptsForMissingWorkItemLikeBash(t *testing.T) {
  // use cmd.SetIn(interactivePipeFile("123\n"))
  // stub git context with branch that has no numeric segment
  // assert warning + prompt + final "Work item: #123"
}

func TestRunDesc_RawStillKeepsBashSummaryTree(t *testing.T) {
  // raw disables markdown rendering only; it must not skip the Bash transcript
}
```

- [ ] **Step 2: Run the new `desc` transcript tests and confirm they fail**

Run:

```bash
go test ./internal/cli -run 'TestRunDesc_DryRunUsesBashTranscript|TestRunDesc_PromptsForMissingWorkItemLikeBash' -count=1
```

Expected: FAIL because `runDesc` currently skips validation/config steps and reads from `os.Stdin` directly.

- [ ] **Step 3: Rewrite `runDesc` to follow the Bash transcript without changing the command surface**

Modify `apps/cli-go/internal/cli/desc.go` so `runDesc` emits this exact sequence before the final result block:

```go
ui.Title(stderr, "Gerando descrição do PR...")

step := ui.Step(stderr, "Validando dependencias")
// validate deps/git/branch
step(true)

step = ui.Step(stderr, "Carregando configuracao")
step(true)

step = ui.Step(stderr, "Validando API keys")
step(true)

step = ui.Step(stderr, "Validando branch")
step(true)

step = ui.Step(stderr, "Coletando contexto git")
step(true)

step = ui.Step(stderr, "Detectando work item")
// flag -> branch -> prompt fallback
step(true)

step = ui.Step(stderr, "Detectando sprint")
step(true)

step = ui.Step(stderr, "Resolvendo repositório Azure DevOps")
step(true)

step = ui.Step(stderr, "Gerando descrição via LLM")
step(true)
```

Implementation rules:

- use `cmd.InOrStdin()` instead of `os.Stdin`
- prompt only when stdin is interactive
- keep Bash wording exactly: `Não foi possivel extrair o work item ID da branch ...`, `ID do work item (Enter para pular):`, `Sem work item detectado`, `Sprint: <n>`, `Sem sprint ativo`, `Repositório: <org>/<project>/<repo>`, `Repositório não-Azure (sem links de PR)`
- complete the LLM phase as a step, not as a standalone success log: `Gerando descrição via LLM` -> `Descrição gerada (<provider>/<model>)`
- dry-run output must use the Bash separator block, `DRY RUN - Prompt que seria enviado ao LLM`, `[SYSTEM]`, `[USER]`, and provider/model summary line
- keep the current Go git/LLM logic underneath where it already works

- [ ] **Step 4: Re-run the targeted `desc` tests**

Run:

```bash
go test ./internal/cli -run 'TestRunDesc_DryRunUsesBashTranscript|TestRunDesc_PromptsForMissingWorkItemLikeBash' -count=1
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/cli-go/internal/cli/desc.go apps/cli-go/internal/cli/desc_test.go apps/cli-go/internal/cli/terminal.go
git commit -m "feat(desc): restore bash step order and work item prompt"
```

### Task 4: Restore the `prt desc` summary block and PR publish flow

**Files:**
- Modify: `apps/cli-go/internal/cli/desc.go:174-279`
- Modify: `apps/cli-go/internal/cli/desc_test.go`

- [ ] **Step 1: Add failing tests for the Bash summary and publish transcript**

Add tests that capture `stderr` and verify the post-LLM tree:

```go
func TestRunDesc_RendersBashSummaryBlock(t *testing.T) {
  // assert: PR — branch, Target, Provider, Work Item summary,
  // optional Azure work-item edit URL subtree, Abrir PR subtree,
  // clipboard success, and "Título disponível acima para copiar manualmente."
}

func TestRunDesc_PRPublishCancelAndSuccessMatchBash(t *testing.T) {
  // one subtest for cancel -> "(cancelado)"
  // one subtest for success -> title "Publicar no Azure DevOps",
  // blank separator rows, "→ PR para dev", reviewer prompt, success URL row
}

func TestRunDesc_PRPublishFailureMatchesBashStepWording(t *testing.T) {
  // force create failure and assert step completion is
  // "Falha ao criar PR → <target>" instead of a raw Go-only error label
}

func TestRunDesc_ClipboardUnavailableMatchesBashWarning(t *testing.T) {
  // force clipboard failure and assert the ⚠ clipboard warning row
}
```

- [ ] **Step 2: Run those tests and confirm they fail**

Run:

```bash
go test ./internal/cli -run 'TestRunDesc_RendersBashSummaryBlock|TestRunDesc_PRPublishCancelAndSuccessMatchBash' -count=1
```

Expected: FAIL because `runDesc` still prints the Go summary/prompt structure.

- [ ] **Step 3: Update the summary and publish helpers to emit the Bash transcript**

Refactor the tail of `runDesc` into two helpers if that keeps the function readable:

```go
func printDescSummary(w io.Writer, ...) {
  // print PR — <branch>, Target, Provider, Work Item, work-item edit URL,
  // Abrir PR subtree, clipboard success/warning rows
}

func maybeCreatePRs(w io.Writer, in io.Reader, ...) error {
  ui.Title(w, "Publicar no Azure DevOps")
  // blank │ row
  // Criar PR(s) no Azure DevOps?
  // per target: blank │ row, → PR para <target>, Reviewer (email), step output
  ui.TitleDone(w)
}
```

Implementation rules:

- `TitleDone()` must not be relied on to print `└`
- print the clipboard success follow-up line exactly
- generate and print the Bash-style Azure work-item edit URL and per-target PR URLs when Azure routing is available
- keep `--raw` limited to output formatting; it must not skip the Bash transcript tree
- on PR creation failure, complete the step as `Falha ao criar PR → <target>` and keep the raw error detail as a separate tree line if needed
- keep the current Go PR creation API call unless a transcript requirement forces a different sequencing point

- [ ] **Step 4: Run the full `desc` test set**

Run:

```bash
go test ./internal/cli -run 'TestRunDesc_|TestParseTitleAndBody_' -count=1
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/cli-go/internal/cli/desc.go apps/cli-go/internal/cli/desc_test.go
git commit -m "feat(desc): restore bash summary and publish transcript"
```

## Chunk 3: `prt test` Parity Prerequisites

### Task 5: Make `--work-item` optional and restore Bash routing order

**Files:**
- Modify: `apps/cli-go/internal/cli/test.go:34-183`
- Modify: `apps/cli-go/internal/cli/test_test.go:10-27`
- Modify: `apps/cli-go/internal/azure/pr.go:31-108`
- Modify: `apps/cli-go/internal/azure/client_test.go`

- [ ] **Step 1: Add failing tests for flag metadata and routing order**

Update `apps/cli-go/internal/cli/test_test.go` so it no longer expects `--work-item` to be required, and add a routing test.

```go
func TestNewTestCmd_DoesNotRequireWorkItemFlag(t *testing.T) {
  cfg := &config.Config{}
  cmd := NewTestCmd(cfg)
  require.NoError(t, cmd.Flags().Lookup("work-item").Value.Set(""))
}

func TestRunTest_ResolvesWorkItemFromPRWhenFlagIsMissing(t *testing.T) {
  // PR-linked items: #300 (Test Case), #11796 (User Story), #11820 (Bug)
  // assert Bash selection rule picks #11796
}

func TestRunTest_ExplicitWorkItemWinsOverPRLinks(t *testing.T) {
  // assert --work-item always overrides linked PR items
}

func TestRunTest_AllLinkedTestCasesFallsBackToLowestID(t *testing.T) {
  // linked items all have type Test Case => choose lowest ID
}

func TestRunTest_NoLinkedItemsFailsWithBashMessage(t *testing.T) {
  // assert: Não foi possível resolver o work item pai. Use --work-item explicitamente.
}
```

- [ ] **Step 2: Run the `test` routing tests and confirm they fail**

Run:

```bash
go test ./internal/cli -run 'TestNewTestCmd_DoesNotRequireWorkItemFlag|TestRunTest_ResolvesWorkItemFromPRWhenFlagIsMissing' -count=1
```

Expected: FAIL because `NewTestCmd` still marks `work-item` as required.

- [ ] **Step 3: Remove the required-flag assumption and codify Bash routing**

In `apps/cli-go/internal/cli/test.go`:

- delete `_ = cmd.MarkFlagRequired("work-item")`
- change the help text to stop saying `(required)`
- resolve the parent work item in this order:
  1. `--work-item`
  2. PR-linked work items using the Bash ranking rule: hydrate each linked item, choose the lowest-ID item whose type is not `Test Case`, and only fall back to the lowest ID if every linked item is `Test Case`
  3. Bash-style error if neither is available

Add the Azure-side helper in `apps/cli-go/internal/azure/pr.go` so the CLI does not own REST details:

```go
func (c *Client) GetPullRequestWorkItemIDs(ctx context.Context, project, repo string, prID int) ([]int, error) {
  // GET /pullRequests/{id}/workitems and return hydrated IDs for ranking in the CLI layer
}
```

Keep the final wording aligned with Bash:

```go
stepWI := ui.Step(stderr, "Resolvendo work item")
// resolve CLI override first, then linked PR work item
stepWI(true)
```

- [ ] **Step 4: Re-run the targeted routing tests**

Run:

```bash
go test ./internal/cli -run 'TestNewTestCmd_DoesNotRequireWorkItemFlag|TestRunTest_ResolvesWorkItemFromPRWhenFlagIsMissing' -count=1
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/cli-go/internal/cli/test.go apps/cli-go/internal/cli/test_test.go apps/cli-go/internal/azure/pr.go apps/cli-go/internal/azure/client_test.go
git commit -m "fix(test): restore bash work item resolution order"
```

### Task 6: Extend Azure helpers to match the Bash Test Case payload and Test QA update flow

**Files:**
- Modify: `apps/cli-go/internal/azure/testcase.go:12-85`
- Modify: `apps/cli-go/internal/azure/workitem.go:45-117`
- Modify: `apps/cli-go/internal/azure/client_test.go:39-106`
- Create: `apps/cli-go/internal/azure/workitem_test.go`

- [ ] **Step 1: Add failing Azure client tests for the richer patch payloads**

Extend `client_test.go` and add `workitem_test.go` with coverage like:

```go
func TestCreateTestCase_SendsBashParityFields(t *testing.T) {
  req := azure.CreateTestCaseRequest{
    Title:          "Test Case Title",
    AreaPath:       "AGROTRACE\\Devops",
    AssignedTo:     "user@example.com",
    ParentID:       123,
    DescriptionHTML:"<p>body</p>",
    StepsXML:       "<steps id=\"0\" last=\"2\"></steps>",
    IterationPath:  "AGROTRACE\\Sprint 98",
    Priority:       ptrInt(2),
    Team:           "DevOps",
    Program:        "Agrotrace",
  }
  // assert exact JSON Patch ops and values for:
  // System.Title, System.Description, Microsoft.VSTS.TCM.Steps,
  // System.AreaPath, relation URL, System.IterationPath,
  // Microsoft.VSTS.Common.Priority as number,
  // Custom.Team, Custom.ProgramasAgrotrace, System.AssignedTo
}

func TestUpdateWorkItemToTestQA_SendsEffortAndRealEffort(t *testing.T) {
  // assert /fields/System.State, /fields/Microsoft.VSTS.Scheduling.Effort,
  // and /fields/Custom.RealEffort are all sent
}

func TestUpdateWorkItemToTestQA_LeavesOptionalFieldsOutWhenNil(t *testing.T) {
  // assert nil pointers still patch System.State = Test QA but omit Effort/RealEffort ops
}
```

- [ ] **Step 2: Run the Azure client tests and verify they fail**

Run:

```bash
go test ./internal/azure -run 'TestCreateTestCase_SendsBashParityFields|TestUpdateWorkItemToTestQA_SendsEffortAndRealEffort' -count=1
```

Expected: FAIL because the current create/update helpers only send a small subset of the Bash payload.

- [ ] **Step 3: Expand the Azure request types to match the Bash flow**

Update `apps/cli-go/internal/azure/testcase.go`:

```go
type CreateTestCaseRequest struct {
  Title           string
  AreaPath        string
  AssignedTo      string
  ParentID        int
  DescriptionHTML string
  StepsXML        string
  IterationPath   string
  Priority        *int
  Team            string
  Program         string
}
```

Patch operations must include the Bash field paths:

- `/fields/System.Description`
- `/fields/Microsoft.VSTS.TCM.Steps`
- `/fields/System.AreaPath`
- `/fields/System.IterationPath`
- `/fields/Microsoft.VSTS.Common.Priority`
- `/fields/Custom.Team`
- `/fields/Custom.ProgramasAgrotrace`
- `/fields/System.AssignedTo`
- `/relations/-`

Update `apps/cli-go/internal/azure/workitem.go` with a dedicated method instead of overloading the generic one:

```go
func (c *Client) UpdateWorkItemToTestQA(ctx context.Context, project string, wiID int, effort, realEffort *float64) error {
  // patch System.State = Test QA and optional Effort / Custom.RealEffort
}
```

- [ ] **Step 4: Re-run the Azure package tests**

Run:

```bash
go test ./internal/azure -count=1
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/cli-go/internal/azure/testcase.go apps/cli-go/internal/azure/workitem.go apps/cli-go/internal/azure/client_test.go apps/cli-go/internal/azure/workitem_test.go
git commit -m "feat(azure): match bash test case and Test QA payloads"
```

## Chunk 4: `prt test` Transcript and Final Verification

### Task 7: Restore the Bash generation and publish transcript for `prt test`

**Files:**
- Modify: `apps/cli-go/internal/cli/test.go:63-291`
- Modify: `apps/cli-go/internal/cli/test_test.go`

- [ ] **Step 1: Add failing transcript tests for both `prt test` phases**

Extend `apps/cli-go/internal/cli/test_test.go` with transcript tests that stub Azure/LLM dependencies and capture `stdout` + `stderr`.

```go
func TestRunTest_RendersBashGenerationTranscript(t *testing.T) {
  // assert exact step labels and summary block:
  // Gerando card de teste..., Validando Azure PAT, Detectando contexto git,
  // Resolvendo contexto Azure DevOps, Gerando card via LLM, Test Card — PR #...
}

func TestRunTest_PublishCancelAndNonInteractiveMatchBash(t *testing.T) {
  // subtest 1: interactive cancel => title + blank │ + (cancelado)
  // subtest 2: non-interactive auto-create path => warning tree block only
}

func TestRunTest_PublishSuccessPromptsForEffortAndRealEffort(t *testing.T) {
  // simulate missing fields, confirm create + update,
  // assert Effort and Real Effort prompts and final success line
}

func TestRunTest_CreateFailurePrintsFullBashFallbackBlock(t *testing.T) {
  // assert warning tree, raw Azure error detail, Campos tentados na criacao:,
  // AreaPath, IterationPath, Priority, Custom.Team,
  // Custom.ProgramasAgrotrace, AssignedTo, Parent,
  // and the manual fallback guidance line
}
```

- [ ] **Step 2: Run the `prt test` transcript tests and confirm they fail**

Run:

```bash
go test ./internal/cli -run 'TestRunTest_RendersBashGenerationTranscript|TestRunTest_PublishCancelAndNonInteractiveMatchBash|TestRunTest_PublishSuccessPromptsForEffortAndRealEffort|TestRunTest_CreateFailurePrintsFullBashFallbackBlock' -count=1
```

Expected: FAIL because `runTest` still uses the Go-specific transcript and auto-create behavior.

- [ ] **Step 3: Rebuild `runTest` around the Bash transcript**

Update `apps/cli-go/internal/cli/test.go` to emit the Bash sequence exactly.

Generation phase:

```go
ui.Title(stderr, "Gerando card de teste...")
// Validando dependencias
// Carregando configuracao
// Validando Azure PAT
// Validando API keys
// Detectando contexto git
// Resolvendo contexto Azure DevOps
// Resolvendo PR
// Resolvendo work item
// Buscando alteracoes do PR
// Buscando exemplos de test case
// Preparando campos de criacao
// Gerando card via LLM
```

Publish phase rules:

- show `Publicar no Azure DevOps` only when the Bash flow would attempt creation
- interactive: blank `│` separator, confirmation prompt, optional `(cancelado)`
- non-interactive: print `Ambiente não interativo; pulando criacao automatica do Test Case. Rode interativamente para confirmar a criacao.` and stop
- only prompt `Atualizar o work item #<id> para Test QA?` if the test case was actually created
- after `Test case criado: #<id>`, print the created Azure DevOps URL on the next tree line
- if the update is confirmed, fetch the current work item first, only prompt when `Effort` / `Custom.RealEffort` are missing, and default `Real Effort` to the chosen `Effort` value exactly like Bash
- if the update is confirmed and fields are missing, prompt:

```text
Effort (horas decimais, ex: 0.5) [0.5]:
Real Effort (horas decimais) [<default>]:
```

- on create failure, print the full Bash fallback block including `Campos tentados na criacao:` and all attempted fields

- [ ] **Step 4: Run the focused `prt test` tests**

Run:

```bash
go test ./internal/cli -run 'TestRunTest_' -count=1
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/cli-go/internal/cli/test.go apps/cli-go/internal/cli/test_test.go
git commit -m "feat(test): restore bash transcript and publish flow"
```

### Task 8: Full verification and cleanup

**Files:**
- Modify only if a preceding verification step reveals a defect.

- [ ] **Step 1: Run package-level verification**

Run from `apps/cli-go`:

```bash
go test ./internal/ui ./internal/azure ./internal/cli -count=1
```

Expected: PASS.

- [ ] **Step 2: Run the module test suite**

Run:

```bash
go test ./... -count=1
```

Expected: PASS.

- [ ] **Step 3: Run CLI smoke checks that do not require credentials**

Run:

```bash
go test ./internal/cli -run 'TestNewRootCmdBuildsStableMetadata|TestRunDesc_DryRunUsesBashTranscript|TestRunTest_RendersBashGenerationTranscript' -count=1
```

Expected: PASS.

- [ ] **Step 4: If credentials are available, do interactive manual smoke checks**

Run from `apps/cli-go` with a real terminal:

```bash
go run . desc --dry-run
go run . test --pr <id>
printf '' | go run . test --pr <id>
```

Expected:

- `desc` shows the Bash-style title, steps, and dry-run transcript
- interactive `test` shows the Bash-style publish prompt and allows cancel/success flows
- non-interactive `test` prints the Bash warning `Ambiente não interativo; pulando criacao automatica do Test Case. Rode interativamente para confirmar a criacao.`

- [ ] **Step 5: Commit the verification fixes if any were needed**

```bash
git add <only files changed during verification>
git commit -m "test(cli): close remaining ui parity gaps"
```
