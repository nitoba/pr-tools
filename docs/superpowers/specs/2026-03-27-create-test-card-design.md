# Design: create-test-card

Date: 2026-03-27

## Goal

Add a new Bash CLI command, `create-test-card`, that intelligently discovers Azure DevOps context from the current branch when possible, resolves the most relevant pull request and linked work item automatically, uses an LLM to generate a Markdown test card in Brazilian Portuguese, and then attempts to create a child `Test Case` work item in Azure DevOps.

The command should follow the same operational style as `create-pr-description`: Bash-first, `.env`-based configuration, provider fallback, `--dry-run`, clear terminal output, and graceful degradation when network calls or Azure DevOps process rules block automation.

## Non-goals

- Do not merge this behavior into `create-pr-description`.
- Do not introduce another implementation language.
- Do not attempt to fully model all Azure DevOps custom fields for every process.
- Do not require successful Azure DevOps creation to make the command useful.

## User experience

The new command is a sibling CLI. It should support both explicit and intelligent usage:

```bash
create-test-card
create-test-card --work-item 11796 --pr 10513
```

Expected behavior:

1. Validate required dependencies and configuration.
2. Detect the current branch when running inside a git repository.
3. Resolve Azure DevOps organization, project, and repository.
4. Resolve the pull request, using explicit flags when provided and intelligent lookup otherwise.
5. Resolve the parent work item, using explicit flags when provided and linked PR work items otherwise.
6. Fetch work item and pull request context.
7. Fetch PR changes and a small set of example test cases.
8. Build an LLM prompt focused on QA/test design.
9. Generate a Markdown test card.
10. Print the result clearly in the terminal.
11. Attempt to create a child `Test Case` work item in Azure DevOps.
12. If creation fails, preserve useful output and explain the failure clearly.

## CLI interface

### Optional flags

- `--work-item <id>`: parent work item ID override.
- `--pr <id>`: Azure DevOps pull request ID override.
- `--org <name>`: override autodetected Azure DevOps organization.
- `--project <name>`: override autodetected Azure DevOps project.
- `--repo <name>`: override autodetected Azure DevOps repository.
- `--area-path <path>`: override the default Azure DevOps area path for created test cases.
- `--assigned-to <email>`: override the default assignee for the created test case.
- `--examples <n>`: number of test case examples to include in prompt, default `2`.
- `--no-create`: generate output but skip Azure DevOps creation.
- `--dry-run`: print prompts and an example creation preview without calling the LLM or creating a work item.
- `--raw`: print only the Markdown body with minimal framing.
- `--debug`: print diagnostic context such as selected examples, API decisions, and creation fields.
- `--init`: reuse the existing config bootstrap flow if needed.
- `--help`
- `--version`
- `--update`

### Output behavior

Normal mode should show:

- command summary
- selected provider/model
- generated title
- generated Markdown card
- creation status
- created test case URL if successful
- actionable fallback guidance if creation fails

`--raw` should print only the generated Markdown plus, if creation succeeds, a short trailing success line to stderr rather than mixing metadata into stdout.

### Flag validation rules

- `--work-item`, when provided, must be a positive integer.
- `--pr`, when provided, must be a positive integer.
- `--examples` must be an integer between `0` and `5`.
- `--area-path`, when provided, must be a non-empty string.
- `--assigned-to`, when provided, must be a non-empty string and is expected to be an Azure-valid identity email or display value.
- `--dry-run` always implies no LLM call and no Azure DevOps creation.
- `--no-create` is ignored when `--dry-run` is active because creation is already disabled.
- If `--pr` is omitted, the command should try to discover the PR from the current branch.
- If `--work-item` is omitted, the command should try to resolve a linked work item from the PR.
- The command should fail only after autodetection and fallback resolution paths are exhausted.

## Architecture

## Script layout

Create a new file:

- `bin/create-test-card`

Follow the same structure conventions as `bin/create-pr-description`:

- constants and global state near top
- helper log functions
- argument parsing
- config loading
- Azure DevOps helpers
- LLM provider helpers
- output formatting
- `main`

Keep the script mostly self-contained for the first version. Only extract shared helpers if duplication becomes large enough to materially harm maintainability.

## Global state

Expected state values:

- Azure DevOps routing: org, project, repo, repo ID
- branch context: current branch, source branch candidates
- input IDs: work item ID, PR ID
- creation defaults: area path, assignee
- fetched artifacts: work item JSON, PR JSON, change summary, examples summary
- generated artifacts: prompt, title, Markdown body
- creation result: created work item ID/URL, failure reason
- provider metadata: provider/model used

## Data acquisition design

## Execution prerequisites

The command should work both inside and outside a git repository.

- Inside a git repo, it should use current branch and `origin` remote parsing as primary convenience signals.
- Outside a git repo, it must still work if enough Azure routing data is available from flags. PR metadata is only available after routing is already established.

Minimum routing behavior:

- if `--org` and `--project` are provided, the command can operate without git remote detection
- if `--repo` is omitted, the command may resolve repository from the PR lookup
- if `--pr` is provided, the script must still resolve `--org` and `--project` from flags or git remote before fetching the PR
- if `--pr` is omitted and the command is not inside a git repo, the user must provide `--pr`
- if neither git remote nor explicit routing can resolve required Azure context, the command must fail clearly

## Resolution order

Split routing into two phases.

### Pre-PR discovery routing

Use deterministic routing precedence before PR lookup:

1. explicit flags: `--org`, `--project`, `--repo`
2. git remote parsing

At this phase, `org` and `project` must be resolvable before branch-based PR discovery can begin.

### Post-PR validation and enrichment

After a PR is resolved, use PR metadata to:

- confirm repository identity
- validate that the resolved PR belongs to the expected project/repository context
- enrich missing repository information when `--repo` was not known before discovery

## PR resolution order

Use deterministic PR resolution precedence:

1. explicit `--pr`
2. current branch -> search active PRs whose `sourceRefName` matches the branch
3. current branch -> search recent PRs whose source branch matches if no active PR is found

Branch normalization rules:

- read current branch from git as a short branch name such as `feature/foo`
- normalize comparisons against Azure DevOps as `refs/heads/<branch>`
- if git is in detached HEAD state, branch-based PR autodetection is unavailable
- remote-tracking names such as `origin/feature/foo` must be normalized back to `feature/foo` before comparison
- fork-specific behavior is out of scope for v1; branch-based autodetection assumes the PR source branch is in the same Azure DevOps repository context

If multiple PRs match the current branch:

- prefer `active`
- otherwise prefer the best available Azure DevOps update signal for the PR, using a deterministic timestamp derived from PR review activity/details when a dedicated PR-level `lastUpdatedDate` field is not available from the API responses
- emit a warning in `--debug` output that multiple PRs matched and which one was chosen

If no PR can be resolved, fail clearly.

## Work item resolution order

Use deterministic work item resolution precedence:

1. explicit `--work-item`
2. work items linked directly to the resolved PR
3. if multiple linked work items exist, prefer the lowest-ID linked item whose type is not `Test Case`
4. if all linked items are `Test Case`, prefer the lowest-ID linked item and warn in `--debug`
5. branch token parsing may be shown only as diagnostics and must not be used as a resolution path in v1

If no parent work item can be resolved, fail clearly with guidance to pass `--work-item` explicitly.

## Fatal vs non-fatal steps

- work item lookup: fatal on failure
- PR lookup: fatal on failure
- project/org/repo resolution: fatal if final routing is incomplete
- PR autodetection from branch: warning during failed attempts, fatal only if no PR is ultimately resolved
- work item autodetection from PR links: warning during failed attempts, fatal only if no work item is ultimately resolved
- PR changes lookup: warning-only if unavailable; continue with reduced context
- example test case lookup: warning-only if unavailable; continue without examples
- repository ID lookup: warning-only unless creation is attempted; fatal only for the create step
- Azure DevOps create call: warning/failure in final result, but never discard generated Markdown

### 1. Work item lookup

Fetch the parent work item first. Extract at minimum:

- `System.Id`
- `System.Title`
- `System.Description`
- `System.AreaPath`
- `System.IterationPath`
- `Microsoft.VSTS.Common.Priority` when present
- existing relations if useful for diagnostics

This becomes the primary functional context and the source of area/iteration defaults for creation.

However, for `Test Case` creation, area path does not default to the parent work item's area. The command should use a dedicated test default described below.

### 2. Pull request lookup

Fetch the PR metadata. Extract at minimum:

- PR title
- PR description
- repository identity
- source and target refs
- status when present
- linked work items if available from the PR response or companion PR endpoints

Lookup contract:

- if `--pr` is available, first use the project-scoped Azure DevOps API that fetches a PR by ID without requiring repository ID/name
- if `--pr` is omitted, first resolve `org` and `project` from flags or git remote
- then resolve `repo` from flags or git remote when available
- then use the normalized current branch name to search PRs by `sourceRefName`
- prefer a repo-scoped PR listing/search endpoint when repository is known; otherwise use a project-scoped PR lookup strategy if available
- derive repository identity from that PR response
- only after repository identity is known should the script call repository-specific endpoints such as PR changes or repository ID lookup

If `--repo` is not provided, use the repository derived from the PR response. If the PR response and explicit `--repo` disagree, fail with a clear validation error.

### 2.1 Linked work item lookup

After resolving the PR, inspect the PR-linked work items and select the parent work item using the work item resolution order above.

Extract at minimum:

- work item ID
- title
- work item type
- state

If the PR has no linked work items, the command should fail clearly and instruct the user to pass `--work-item`.

### 3. Pull request changes

Fetch the changed files and diff summary for the PR.

The script should not blindly dump arbitrarily large diffs into the prompt. Instead it should:

- include changed file paths in full, up to `200` files
- include truncated patch content up to `4000` lines or `120000` characters, whichever limit is hit first
- note when truncation occurred

The goal is to preserve enough implementation detail for QA coverage without making the prompt fragile or excessively expensive.

### 4. Example test cases

Fetch a small set of example `Test Case` work items to anchor style.

Selection heuristic:

1. Prefer same project.
2. Prefer same `AreaPath` as parent work item when possible.
3. Prefer recent work items whose type is `Test Case`.
4. Prefer titles containing terms like `Teste`, `Verificar`, `Validar`, or `Teste |`.
5. Fall back to generic project examples if area-specific examples are unavailable.

Retrieval contract:

- use a WIQL query scoped to the current project for `System.WorkItemType = 'Test Case'`
- sort by `System.ChangedDate DESC`
- fetch candidate IDs first, then hydrate up to `15` candidates via work item detail lookups
- rank candidates by:
  1. same `AreaPath` as parent work item
  2. title contains `Teste |`, `Teste`, `Validar`, or `Verificar`
  3. most recently changed
- fetch up to `15` candidate work items
- keep up to `--examples` final examples, default `2`
- summarize only compact fields for prompt context:
  - title
  - state
  - area path when present
  - description presence
  - steps presence
- if example retrieval fails, log a warning and continue without examples

### 5. Identity and repo resolution

Reuse the existing remote parsing approach from `create-pr-description` where possible:

- detect Azure DevOps from git remote
- allow explicit overrides from flags
- fetch `repositoryId` using Azure DevOps REST when needed
- cache `repositoryId` if that helps reuse existing behavior

## Creation defaults

### Default area path

By default, created test cases should use:

- `AGROTRACE\Devops`

This default is project-specific for `AGROTRACE` and takes precedence over the parent work item's `AreaPath`.

If the resolved project is not `AGROTRACE`, the command must not assume `AGROTRACE\Devops` is valid. In that case it should require either:

- `--area-path`, or
- `TEST_CARD_AREA_PATH`

Override order:

1. explicit `--area-path`
2. process environment `TEST_CARD_AREA_PATH`
3. `.env` value `TEST_CARD_AREA_PATH`
4. built-in default `AGROTRACE\Devops`, only when resolved project is `AGROTRACE`

### Default assignee

Created test cases should also be assigned to a configurable person.

For v1, this is a single default assignee model, not a rule engine.

Override order:

1. explicit `--assigned-to`
2. process environment `TEST_CARD_ASSIGNED_TO`
3. `.env` value `TEST_CARD_ASSIGNED_TO`

If no assignee is configured, the command may still generate Markdown and may still attempt creation without assignment, but it should warn clearly that no default assignee was configured.

The configuration style should mirror the existing reviewer configuration pattern in `.env`, similar to `PR_REVIEWER_DEV` and `PR_REVIEWER_SPRINT`, but with a single key for v1 rather than conditional routing.

## Config precedence

For all new config keys in this command:

- CLI flags override process environment
- process environment overrides `~/.config/pr-tools/.env`
- `.env` overrides built-in defaults

## Prompt design

## System prompt

Use a dedicated QA-oriented system prompt that instructs the model to:

- act as a technical QA analyst
- write in Brazilian Portuguese
- produce a realistic Azure DevOps test card
- avoid inventing behavior not supported by the work item or PR
- include functional, validation, and regression coverage when justified

Required response format:

```text
TITULO: <titulo curto>

## Objetivo
...

## Cenario base
...

## Checklist de testes
1. ...

## Resultado esperado
...
```

## User prompt

Build the user prompt with sections for:

- parent work item metadata
- parent work item description
- PR metadata
- changed files
- truncated diff summary
- example test cases
- explicit instruction to produce Markdown and stay grounded in the provided context

The prompt should be inspectable via `--dry-run`, just like the current PR tool.

## Prompt size controls

To keep behavior stable and testable:

- max changed files listed: `200`
- max diff content included: `4000` lines or `120000` characters
- max example count accepted from CLI: `5`
- default examples in prompt: `2`

Whenever content is truncated, the prompt must explicitly say so.

## Response parsing

Parse the LLM response into:

- title from first line after `TITULO:`
- Markdown body from the remaining content

If the model fails to follow format:

- try a light recovery pass in-shell, such as extracting the first heading or using the first non-empty line as title
- never silently discard the generated content
- warn clearly in output/debug mode when recovery was needed

For `--dry-run`, because no LLM call occurs, the command should print:

- the system prompt
- the user prompt
- a preview of the Azure DevOps creation fields with placeholder values:
  - title: `<from LLM response>`
  - description: `<from LLM response converted to HTML>`

This avoids pretending a real payload exists before generation while still exposing the intended create shape.

## Azure DevOps creation strategy

## Creation target

Create a child work item of type `Test Case` under the provided parent work item.

Use Azure DevOps parent relation type:

- `System.LinkTypes.Hierarchy-Reverse`

## Initial fields to send

Send only fields that are broadly safe and likely available:

- Title
- Description
- Parent relation
- Area path using the test-card default or override
- Iteration path from parent work item when available
- Priority when available
- Assigned To when configured

Do not assume custom fields beyond these.

## Description format

The generated content is Markdown, but Azure DevOps work item descriptions typically accept HTML more reliably.

For v1:

- keep Markdown as the primary user-facing output
- convert Markdown to a minimal HTML representation for the create request using a conservative converter:
  - `## Heading` -> `<h2>`
  - unordered lists -> `<ul><li>`
  - ordered lists -> `<ol><li>`
  - paragraphs -> `<p>`
  - `**bold**` -> `<b>`
- do not attempt full rich conversion or structured `Microsoft.VSTS.TCM.Steps` yet

This keeps the implementation modest while still enabling useful automation.

When building the create payload:

- omit fields that are empty or null
- do not send blank strings for optional Azure fields
- send only validated, non-empty values

## Failure handling

Creation may fail because of:

- required custom fields such as `Team`
- restricted picklist values
- process-specific rules
- insufficient permissions
- bad repository/project resolution

On failure, the command must:

- preserve the generated title and Markdown output
- display the Azure DevOps error message clearly
- show the exact creation fields attempted in `--debug` or `--dry-run`
- explain that manual creation may be required due to process rules

This fallback behavior is essential. The command remains useful even when the organization process blocks direct creation.

## Configuration

Reuse the existing config directory:

- `~/.config/pr-tools/.env`

Likely additions:

- optional test-card-specific model overrides in the future, but not required for v1
- reuse existing provider keys and Azure PAT
- add `TEST_CARD_AREA_PATH` for default test area path
- add `TEST_CARD_ASSIGNED_TO` for default assignee

Avoid fragmenting config unless there is a clear need. Prefer shared provider settings with sensible defaults.

## Documentation changes

Update `README.md` to include:

- what `create-test-card` does
- required dependencies and Azure prerequisites
- sample commands
- expected output
- explanation that Azure DevOps creation may fail on process-specific required fields, with fallback to generated Markdown

## Validation plan

Minimum validation:

- `bash -n bin/create-test-card`
- `bin/create-test-card --help`
- `bin/create-test-card --version`
- `bin/create-test-card --dry-run` from a feature branch with an open PR and linked work item
- `bin/create-test-card --dry-run --work-item 11796 --pr 10513`
- `bin/create-test-card --dry-run --work-item 11796 --pr 10513 --raw`
- one `--dry-run` execution outside a git repo using explicit `--org`, `--project`, and `--pr`

If credentials and environment allow, also perform:

- generation-only test with `--no-create`
- real create attempt against a known work item/PR pair

Any claim of successful Azure DevOps creation must only be made if an actual create call succeeds.

## Risks and trade-offs

- There will be some duplication with `create-pr-description` in v1.
  - Accepted to keep scope under control.
- Azure DevOps process customization means creation cannot be guaranteed.
  - Mitigated by strong fallback output and debug visibility.
- LLM prompt size can grow too much if PR diffs are large.
  - Mitigated by truncation and summaries.
- Markdown-to-HTML conversion can be lossy.
  - Accepted for v1; Markdown remains the source of truth shown to the user.

## Recommended implementation approach

Implement `create-test-card` as a separate sibling CLI that mirrors the current project conventions, reuses the existing provider and Azure DevOps patterns where practical, and prioritizes resilience over perfect Azure DevOps process coverage.

The first release should optimize for:

- high-quality Markdown generation
- transparent prompt inspection
- best-effort Azure DevOps creation
- explicit fallback when enterprise process rules block automation
