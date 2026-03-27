# Create Test Card Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a new Bash CLI, `create-test-card`, that autodetects the current Azure DevOps PR and linked work item when possible, generates a Markdown test card with an LLM, and attempts to create a child `Test Case` assigned to a configurable owner.

**Architecture:** Add a new sibling script in `bin/` that mirrors `create-pr-description` patterns for CLI parsing, config loading, provider fallback, and Azure DevOps REST calls. The command should gather branch/PR/work item context, build a QA-focused prompt, parse the model response into title and Markdown, convert Markdown to minimal HTML for Azure DevOps, and perform best-effort work item creation with strong fallback behavior.

**Tech Stack:** Bash, curl, jq, git, Azure DevOps REST APIs, existing `.env` configuration model.

---

## File structure

- Create: `bin/create-test-card` - main CLI for test card generation and Azure DevOps creation.
- Modify: `README.md` - document installation, flags, autodetection, defaults, and examples.
- Modify: `install.sh` - ensure the new script is installed alongside `create-pr-description`.
- Modify: `bin/create-pr-description` only if a tiny shared helper extraction becomes unavoidable; otherwise do not touch it.
- Reference: `docs/superpowers/specs/2026-03-27-create-test-card-design.md` - approved design source of truth.

## Chunk 1: Scaffold the new CLI and config surface

### Task 1: Create the command skeleton

**Files:**
- Create: `bin/create-test-card`
- Reference: `bin/create-pr-description`

- [ ] **Step 1: Copy the structural shell of the existing CLI**

Create `bin/create-test-card` with:

- shebang `#!/usr/bin/env bash`
- `set -euo pipefail`
- version constant
- config paths under `~/.config/pr-tools`
- log helpers matching the existing style

- [ ] **Step 2: Add minimal global state declarations**

Define globals for:

- branch name and git context flags
- Azure org/project/repo/repo ID
- resolved PR ID and work item ID
- fetched JSON/context blobs
- provider/model/result metadata
- default area path and default assignee
- create result ID/URL/error

- [ ] **Step 3: Add a basic `show_help` function**

Include usage for:

- `--work-item`
- `--pr`
- `--org`
- `--project`
- `--repo`
- `--area-path`
- `--assigned-to`
- `--examples`
- `--no-create`
- `--dry-run`
- `--raw`
- `--debug`
- `--init`
- `--help`
- `--version`
- `--update`

- [ ] **Step 4: Add `--version`, `--update`, and `--init` control flow stubs**

Match the command behavior pattern from `bin/create-pr-description`:

- `--version` prints the script version and exits 0
- `--update` downloads the latest script version and exits 0
- `--init` bootstraps config and exits 0

Copy the relevant implementation pattern from the argument handling and update/init sections of `bin/create-pr-description` instead of inventing a new style.

- [ ] **Step 5: Add a parser skeleton with no behavior yet**

Implement `parse_args` that stores flag values and validates:

- positive integer for `--work-item`
- positive integer for `--pr`
- default `--examples` to `2` when omitted
- integer `0..5` for `--examples`
- non-empty strings for `--area-path` and `--assigned-to`

- [ ] **Step 6: Run shell syntax validation**

Run: `bash -n bin/create-test-card`
Expected: no output

### Task 2: Add startup dependency validation

**Files:**
- Modify: `bin/create-test-card`
- Reference: `bin/create-pr-description`

- [ ] **Step 1: Add required command checks**

Validate presence of:

- `curl`
- `jq`

Treat `git` as conditionally required:

- required for branch autodetection and git-remote routing
- not required when the command is run outside a repo with explicit routing and PR inputs

- [ ] **Step 2: Reuse the dependency-check style from the PR tool**

Follow the concrete validation pattern already used in `bin/create-pr-description` for required commands and user-facing error messages.

- [ ] **Step 3: Add a missing-tool negative check**

Verify that when a required command is unavailable, the script exits non-zero with a targeted message instead of a shell stack trace.

- [ ] **Step 4: Run shell syntax validation**

Run: `bash -n bin/create-test-card`
Expected: no output

## Chunk 2: Reuse environment/config conventions

### Task 3: Load `.env` and defaults in the same style as the PR tool

**Files:**
- Modify: `bin/create-test-card`
- Reference: `bin/create-pr-description`

- [ ] **Step 1: Reuse config bootstrap conventions**

Implement config loading for:

- provider list
- provider API keys
- Azure PAT
- `TEST_CARD_AREA_PATH`
- `TEST_CARD_ASSIGNED_TO`

Copy the concrete `.env` loading and init-writing approach from:

- env persistence section in `bin/create-pr-description`
- init/config wizard section in `bin/create-pr-description`

- [ ] **Step 2: Apply precedence rules**

Implement:

- CLI flags override process env
- process env override `.env`
- `.env` override built-in defaults

- [ ] **Step 3: Extend init/config support for the new keys**

Update the new script's init flow so that it can create or preserve:

- `TEST_CARD_AREA_PATH`
- `TEST_CARD_ASSIGNED_TO`

Use the same prompt/save conventions as the PR tool.

- [ ] **Step 4: Encode the built-in area default rule**

Implement logic:

- if project is `AGROTRACE` and no override exists, use `AGROTRACE\Devops`
- if project is not `AGROTRACE`, require configured area path or explicit flag before create step

- [ ] **Step 5: Warn when assignee is missing**

Add a non-fatal warning path when `TEST_CARD_ASSIGNED_TO`/`--assigned-to` is not available.

- [ ] **Step 6: Add a negative config-path check**

Run outside a configured environment or with the needed env vars temporarily unset:

```bash
bin/create-test-card --dry-run --pr 10513 --org ibsbiosistemico --project AGROTRACE
```

Expected: clear warning or config error about missing provider key / Azure PAT, without shell crash

- [ ] **Step 7: Run syntax validation again**

Run: `bash -n bin/create-test-card`
Expected: no output

## Chunk 3: Git context and Azure routing

### Task 4: Add branch detection and git-optional execution behavior

**Files:**
- Modify: `bin/create-test-card`

- [ ] **Step 1: Detect whether the command is inside a git repo**

Implement a helper equivalent to `validate_git_repo`, but do not fail just because git is unavailable.

- [ ] **Step 2: Add a negative detached-HEAD check design path**

If git is present but branch name cannot be resolved and `--pr` is absent, the command should fail with a targeted error.

- [ ] **Step 3: Detect current branch when inside git**

Capture:

- short branch name
- detached HEAD state
- normalized source ref `refs/heads/<branch>` when available

- [ ] **Step 4: Parse Azure remote when possible**

Mirror the remote parsing logic from `create-pr-description` for:

- HTTPS Azure remotes
- SSH Azure remotes

Reuse the concrete parsing patterns from the `parse_azure_remote` section of `bin/create-pr-description`.

- [ ] **Step 5: Implement pre-PR routing resolution**

Resolve in order:

1. flags
2. git remote

At minimum, ensure `org` and `project` are available before any PR fetch.

- [ ] **Step 6: Add failure messaging for impossible routing**

Examples:

- outside git repo with no `--org/--project`
- detached HEAD with no `--pr`

Also cover:

- git repo with non-Azure remote and no explicit `--org/--project`

- [ ] **Step 7: Validate with positive and negative routing checks**

Run:

```bash
bash -n bin/create-test-card && bin/create-test-card --help >/dev/null
```

Expected: exit code 0

Then run a negative case outside a git repo without routing flags and verify it exits non-zero with a clear routing message.

## Chunk 4: Resolve PR intelligently

### Task 5: Implement PR lookup by explicit ID or current branch

**Files:**
- Modify: `bin/create-test-card`

- [ ] **Step 1: Add project-scoped PR-by-ID lookup**

Implement a function that fetches a PR by ID after `org` and `project` are known.

Use the exact Azure DevOps pattern from the approved spec: project-scoped PR lookup first, repository-specific calls only after repository identity is known.

Use the project-scoped PR endpoint shape documented in the spec, then parse at minimum:

- PR ID
- title
- description
- source ref name
- target ref name
- status
- repository name / repository ID fields when present

- [ ] **Step 2: Add branch-based PR search**

Implement branch-based lookup using normalized `refs/heads/<branch>`.

Behavior:

- prefer active PRs
- otherwise prefer most recently updated
- warn in debug mode when multiple PRs match

Normalize branch comparisons as `refs/heads/<branch>` and handle `origin/<branch>` cleanup before comparison.

- [ ] **Step 3: Derive repository identity from PR response**

Extract repository name and any available repository ID/reference info.

- [ ] **Step 4: Validate explicit repo overrides**

If `--repo` was provided and differs from the PR repository, fail clearly.

- [ ] **Step 5: Add fetch of repo ID when needed**

Mirror the caching/fetching strategy from `create-pr-description` if practical.

Mark this path as non-fatal until the create step actually needs repository ID.

- [ ] **Step 6: Add a narrow dry-run debug path for PR resolution**

Run:

```bash
bin/create-test-card --dry-run --pr 10513 --org ibsbiosistemico --project AGROTRACE --debug
```

Expected: prints PR resolution details and no create attempt

- [ ] **Step 7: Add a negative PR-resolution check**

Run with a branch/PR combination that should not resolve and verify the command fails with a clear PR resolution error instead of a generic shell/network error.

## Chunk 5: Resolve parent work item intelligently

### Task 6: Implement work item resolution from CLI or PR links

**Files:**
- Modify: `bin/create-test-card`

- [ ] **Step 1: Add linked work item fetch for resolved PR**

Implement a helper to retrieve work items linked to the PR.

Use a concrete Azure DevOps PR-linked-work-items endpoint/call shape and parse at minimum:

- linked work item ID
- title
- work item type
- state

- [ ] **Step 2: Add deterministic selection rules**

If `--work-item` is absent:

- prefer lowest-ID linked work item whose type is not `Test Case`
- otherwise prefer lowest-ID linked work item
- warn in debug mode when multiple items exist

- [ ] **Step 3: Fetch full parent work item details**

Extract:

- title
- description
- area path
- iteration path
- priority

- [ ] **Step 4: Add failure guidance**

If no work item can be resolved, fail with a message telling the user to pass `--work-item` explicitly.

- [ ] **Step 5: Add a dry-run resolution check**

Run:

```bash
bin/create-test-card --dry-run --pr 10513 --org ibsbiosistemico --project AGROTRACE --debug
```

Expected: prints chosen parent work item or a clear resolution error

- [ ] **Step 6: Add a negative no-linked-work-item check**

Verify the command exits non-zero with explicit guidance when the PR has no linked work items and `--work-item` was not provided.

## Chunk 6: Fetch PR changes and example test cases

### Task 7: Build prompt context inputs

**Files:**
- Modify: `bin/create-test-card`

- [ ] **Step 1: Add PR changes retrieval**

Fetch changed files and patch/diff summary from the resolved PR.

This path must be warning-only if unavailable; generation should continue with reduced context.

- [ ] **Step 2: Add truncation helpers**

Implement prompt limits:

- max changed files listed: `200`
- max diff included: `4000` lines or `120000` chars

- [ ] **Step 3: Add example test case WIQL lookup**

Implement:

- query recent `Test Case` IDs in the current project
- sort by `System.ChangedDate DESC`
- hydrate up to `15` candidates

Default the final included examples to `2` when the user does not pass `--examples`.

- [ ] **Step 4: Rank and summarize example test cases**

Rank by:

1. same area path as parent work item
2. title contains `Teste |`, `Teste`, `Validar`, or `Verificar`
3. most recently changed

Summarize only compact fields.

- [ ] **Step 5: Make examples non-fatal**

If example lookup fails, warn and continue.

- [ ] **Step 6: Add debug inspection output**

Run:

```bash
bin/create-test-card --dry-run --pr 10513 --work-item 11796 --org ibsbiosistemico --project AGROTRACE --debug
```

Expected: shows files/diff truncation summary and selected examples

- [ ] **Step 7: Add a degraded-context check**

Simulate or force a PR-changes/examples failure path and verify the command logs a warning and still reaches prompt generation in `--dry-run`.

## Chunk 7: Prompt generation and LLM integration

### Task 8: Generate Markdown test cards with the existing provider model

**Files:**
- Modify: `bin/create-test-card`
- Reference: `bin/create-pr-description`

- [ ] **Step 1: Add a QA-specific default system prompt**

Require this output shape:

```text
TITULO: <titulo>

## Objetivo
...

## Cenario base
...

## Checklist de testes
1. ...

## Resultado esperado
...
```

- [ ] **Step 2: Build the user prompt from gathered context**

Include:

- branch/PR/work item summary
- changed files
- diff summary
- example test cases
- explicit Markdown instruction

- [ ] **Step 3: Reuse provider fallback pattern**

Mirror the provider selection and request flow from `create-pr-description`:

- OpenRouter
- Groq
- Gemini

Copy the concrete payload-building and provider-call patterns from the relevant sections of `bin/create-pr-description` rather than re-designing provider integration.

Specifically follow the helper structure/patterns used for:

- provider config lookup
- OpenAI-compatible payload building
- provider fallback loop
- Gemini request handling
- streaming/non-streaming output handling if retained

- [ ] **Step 4: Implement `--dry-run` prompt output**

Dry-run should print:

- system prompt
- user prompt
- placeholder create preview

- [ ] **Step 5: Add normal-mode output formatting**

Implement the standard successful output contract from the spec:

- command summary
- selected provider/model
- generated title
- generated Markdown card
- creation status
- created URL when successful
- fallback guidance when creation fails

- [ ] **Step 6: Parse the LLM response**

Extract:

- title from `TITULO:`
- Markdown body from the remainder

Include a recovery path when the format is imperfect.

- [ ] **Step 7: Smoke test non-network path**

Run:

```bash
bin/create-test-card --dry-run --pr 10513 --work-item 11796 --org ibsbiosistemico --project AGROTRACE
```

Expected: prints prompts and preview payload, without API create call

- [ ] **Step 8: Add a malformed-response recovery check**

Test the parser helper with a mocked malformed response string and verify it preserves useful output and emits a warning in debug mode.

## Chunk 8: Convert Markdown to HTML and create the Test Case

### Task 9: Implement best-effort Azure DevOps creation

**Files:**
- Modify: `bin/create-test-card`

- [ ] **Step 1: Add a minimal Markdown-to-HTML converter**

Support only the planned subset:

- `##` headings
- ordered lists
- unordered lists
- paragraphs
- `**bold**`

- [ ] **Step 2: Build the create payload**

Include only non-empty fields:

- title
- description HTML
- parent relation `System.LinkTypes.Hierarchy-Reverse`
- area path
- iteration path
- priority
- assigned to

- [ ] **Step 3: Implement Azure DevOps create call**

Use a JSON Patch-compatible create request for `Test Case` work items.

Keep repository ID lookup out of the critical path unless the create flow or diagnostics explicitly need it.

- [ ] **Step 4: Add creation fallback behavior**

If create fails:

- keep Markdown/title visible
- show Azure error message
- show attempted fields in debug mode

Also preserve the final generated output contract in normal mode and avoid losing stdout usefulness when the Azure call fails.

- [ ] **Step 5: Implement `--no-create`**

Ensure generation succeeds without any create request when `--no-create` is active.

- [ ] **Step 6: Add raw-output behavior**

Run:

```bash
bin/create-test-card --dry-run --pr 10513 --work-item 11796 --org ibsbiosistemico --project AGROTRACE --raw
```

Expected: stdout contains only the body-oriented dry-run output shape with minimal framing

- [ ] **Step 7: Add full normal-mode `--raw` behavior**

Implement the real non-dry-run raw contract from the spec:

- stdout contains only generated Markdown
- if create succeeds, emit only a short success/status line to stderr
- if create fails, keep stdout as Markdown and emit the failure/status details to stderr

- [ ] **Step 8: Add a no-create positive check**

Run:

```bash
bin/create-test-card --dry-run --no-create --pr 10513 --work-item 11796 --org ibsbiosistemico --project AGROTRACE
```

Expected: dry-run still behaves consistently and never attempts create logic

## Chunk 9: Install and document the new command

### Task 10: Wire the command into the repo experience

**Files:**
- Modify: `install.sh`
- Modify: `README.md`

- [ ] **Step 1: Update installer behavior**

Ensure `install.sh` installs `create-test-card` alongside `create-pr-description`.

- [ ] **Step 2: Document configuration keys**

Add docs for:

- `TEST_CARD_AREA_PATH`
- `TEST_CARD_ASSIGNED_TO`

Also document config precedence: CLI > process env > `.env` > built-in defaults.

- [ ] **Step 3: Document intelligent defaults**

Add examples for:

- `create-test-card`
- `create-test-card --pr 10513`
- `create-test-card --work-item 11796 --pr 10513`
- `create-test-card --no-create`

- [ ] **Step 4: Document failure behavior**

Explain that process-specific Azure rules may block creation and the command will fall back to generated Markdown.

- [ ] **Step 5: Document prerequisites and install path**

Document:

- required tools: `git`, `curl`, `jq`
- Azure DevOps PAT requirement for real API usage
- that the script is installed as `~/.local/bin/create-test-card`

- [ ] **Step 6: Run syntax validation for installer and script**

Run:

```bash
bash -n bin/create-test-card && bash -n install.sh
```

Expected: no output

## Chunk 10: Final verification

### Task 11: Verify the full delivery surface

**Files:**
- Modify: `bin/create-test-card`
- Modify: `README.md`
- Modify: `install.sh`

- [ ] **Step 1: Verify help and version**

Run:

```bash
bin/create-test-card --help >/dev/null && bin/create-test-card --version
```

Expected: help exits 0 and version prints the script version

- [ ] **Step 2: Verify dry-run with explicit IDs**

Run:

```bash
bin/create-test-card --dry-run --work-item 11796 --pr 10513 --org ibsbiosistemico --project AGROTRACE
```

Expected: successful prompt preview

- [ ] **Step 3: Verify raw-output path**

Run:

```bash
bin/create-test-card --dry-run --work-item 11796 --pr 10513 --org ibsbiosistemico --project AGROTRACE --raw
```

Expected: minimal stdout framing consistent with the raw contract

- [ ] **Step 4: Verify dry-run autodetection path**

Run from a feature branch with an open Azure DevOps PR and linked work item:

```bash
bin/create-test-card --dry-run --debug
```

Expected: branch, PR, and parent work item are autodetected

- [ ] **Step 5: Verify outside-git explicit-routing path**

Run outside a git repo:

```bash
create-test-card --dry-run --pr 10513 --work-item 11796 --org ibsbiosistemico --project AGROTRACE
```

Expected: succeeds without git context

- [ ] **Step 6: Verify detached-HEAD / missing-routing negative behavior**

Run representative negative cases and verify they fail with targeted, user-facing errors.

- [ ] **Step 7: Verify create attempt behavior if credentials allow**

Run:

```bash
bin/create-test-card --pr 10513 --work-item 11796 --debug
```

Expected:

- either a created `Test Case` ID and URL
- or a clear Azure DevOps error with preserved Markdown output
