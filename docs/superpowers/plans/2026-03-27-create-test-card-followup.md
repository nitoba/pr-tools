# Create Test Card Follow-up Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix `create-test-card` so AGROTRACE `Test Case` creation uses the real required defaults and the command reports visible progress in normal mode.

**Architecture:** Keep the current script shape and apply a focused follow-up: adjust creation defaults/payload fields, add normal-mode progress logs around each long-running stage, and improve failure output so users can see which create fields were attempted. Avoid broader refactors in this pass.

**Tech Stack:** Bash, curl, jq, Azure DevOps REST APIs.

---

## File structure

- Modify: `bin/create-test-card` - apply creation-default fixes, add progress logs, improve failure summary.
- Modify: `README.md` - document the new AGROTRACE defaults and the visible progress behavior.
- Reference: `docs/superpowers/specs/2026-03-27-create-test-card-followup-design.md`

## Chunk 1: Fix Azure DevOps create defaults

### Task 1: Stop inheriting invalid create fields from the parent item

**Files:**

- Modify: `bin/create-test-card`

- [ ] **Step 1: Identify the create payload fields currently inherited from the parent**

Check the existing payload construction and list which fields come from the parent work item versus explicit defaults.

- [ ] **Step 2: Replace parent-priority inheritance with AGROTRACE default priority**

Update the payload logic so `Test Case` creation uses:

- `Microsoft.VSTS.Common.Priority = 2`

for `AZURE_PROJECT=AGROTRACE`, instead of inheriting the parent work item priority.

- [ ] **Step 3: Add required AGROTRACE custom fields to the create payload**

Always send, for `AZURE_PROJECT=AGROTRACE`:

- `Custom.Team = DevOps`
- `Custom.ProgramasAgrotrace = Agrotrace`

- [ ] **Step 4: Preserve existing assignment and area-path behavior**

Keep:

- `System.AreaPath = AGROTRACE\Devops` unless overridden
- `System.AssignedTo` from CLI/env when configured

- [ ] **Step 5: Store explicit attempted create defaults in one place**

Compute and store the final attempted create fields once, so the same values are reused consistently by:

- create payload building
- dry-run preview
- failure summary output

- [ ] **Step 6: Verify shell syntax after payload changes**

Run:

```bash
bash -n bin/create-test-card
```

Expected: no output

## Chunk 2: Add visible progress logs

### Task 2: Make long-running stages visible in normal mode

**Files:**

- Modify: `bin/create-test-card`

- [ ] **Step 1: Add normal-mode start logs for each major phase**

Add `log_info` messages before these stages:

- resolving Azure DevOps context
- resolving PR
- resolving parent work item
- fetching PR changes
- fetching example test cases
- generating card via LLM
- creating the Test Case in Azure DevOps

- [ ] **Step 2: Preserve `--raw` behavior**

Ensure raw mode keeps stdout clean. If needed, direct progress diagnostics to stderr for `--raw`.

- [ ] **Step 3: Keep `--debug` additive only**

Do not move basic progress into `--debug`; only extra technical detail belongs there.

- [ ] **Step 4: Verify dry-run output still works**

Run:

```bash
bin/create-test-card --dry-run --work-item 11904 --pr 10521 --org ibsbiosistemico --project AGROTRACE
```

Expected: progress logs plus prompt preview

## Chunk 3: Improve failure output for create attempts

### Task 3: Make Azure DevOps validation failures easier to understand

**Files:**

- Modify: `bin/create-test-card`

- [ ] **Step 1: Add a concise create-field summary for failures**

When create fails, print at minimum:

- `AreaPath`
- `IterationPath` when present
- `Priority`
- `Custom.Team`
- `Custom.ProgramasAgrotrace`
- `AssignedTo`
- parent work item target

Use a shared formatter so the same summary can be shown in:

- dry-run create preview
- real create failure output

- [ ] **Step 2: Keep generated Markdown visible on failure**

Do not change the existing fallback contract where the generated card remains available for manual creation.

- [ ] **Step 3: Keep the Azure DevOps error visible on failure**

Do not hide or replace the Azure validation error; show it alongside the summarized attempted fields.

- [ ] **Step 4: Preserve `--raw` stdout cleanliness on failure paths**

Ensure any failure diagnostics or field summaries go to stderr in `--raw`, so stdout remains reserved for Markdown.

- [ ] **Step 5: Verify summary output through the shared preview/failure formatter**

Run a command that reaches the dry-run create preview and inspect the summary fields that will also be used by the real failure path. The formatter/output contract must still preserve the Azure error when the real create path fails.

## Chunk 4: Update docs and verify behavior

### Task 4: Document the follow-up defaults and progress behavior

**Files:**

- Modify: `README.md`

- [ ] **Step 1: Document AGROTRACE create defaults**

Add a short note that AGROTRACE `Test Case` creation uses:

- priority `2`
- `Team = DevOps`
- `Programas Agrotrace = Agrotrace`

- [ ] **Step 2: Document visible progress logs**

Add a short note that the command now reports progress in normal mode during API/LLM phases.

- [ ] **Step 3: Run final verification commands**

Run:

```bash
bash -n bin/create-test-card && bin/create-test-card --help >/dev/null && bin/create-test-card --version
```

Expected: success and version output

- [ ] **Step 4: Run explicit dry-run verification**

Run:

```bash
bin/create-test-card --dry-run --work-item 11904 --pr 10521 --org ibsbiosistemico --project AGROTRACE
```

Expected: visible progress logs and create preview showing the AGROTRACE defaults

- [ ] **Step 5: Run raw-mode verification**

Run:

```bash
bin/create-test-card --dry-run --raw --work-item 11904 --pr 10521 --org ibsbiosistemico --project AGROTRACE >/tmp/test-card-raw.out
```

Expected: stdout remains preview/raw-only without human-mode progress noise mixed into the markdown-oriented output contract
