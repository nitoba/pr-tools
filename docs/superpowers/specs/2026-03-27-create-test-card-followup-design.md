# Design: create-test-card follow-up fixes

Date: 2026-03-27

## Goal

Adjust `create-test-card` so it can create Azure DevOps `Test Case` items using the actual required defaults for the AGROTRACE process, and make the command visibly report progress during long-running startup/network phases.

## Problem summary

Two concrete problems were observed in real usage:

1. `Test Case` creation failed because the script inherited unsupported or incomplete work item fields from the parent item.
2. The command takes noticeable time before producing output, which makes it unclear whether it is working or stuck.

## Required behavior changes

### 1. Stop inheriting invalid create fields from the parent work item

For AGROTRACE `Test Case` creation, the script must not copy the parent work item's priority directly.

These defaults are specific to `AZURE_PROJECT=AGROTRACE`. They should not be blindly assumed for other projects.

Instead, it must use these defaults for the created `Test Case`:

- `Microsoft.VSTS.Common.Priority = 2`
- `Custom.Team = DevOps`
- `Custom.ProgramasAgrotrace = Agrotrace`

These should be treated as creation defaults for this workflow, alongside the already established defaults:

- `System.AreaPath = AGROTRACE\Devops`
- `System.AssignedTo` from CLI or env config when available

### 2. Add visible progress logs in normal mode

The command must emit progress logs in normal mode, not only in `--debug` mode.

Logging contract:

- emit one start log for each major phase
- keep existing warn/error behavior when a phase degrades or fails
- `--debug` remains additive and can show extra technical detail
- `--raw` must keep stdout clean; in `--raw`, any progress/failure diagnostics must go to stderr or be suppressed if that better preserves the raw contract

At minimum, it should log each major phase:

- resolving Azure DevOps context
- resolving PR
- resolving parent work item
- fetching PR changes
- fetching example test cases
- generating the card via LLM
- creating the `Test Case` in Azure DevOps

These logs should use the existing normal info/warn output style so users can understand what the command is doing while it waits on network/API calls.

`--debug` remains for extra diagnostics and raw troubleshooting detail.

## Payload changes

The Azure DevOps create payload for `Test Case` must always include, for this AGROTRACE workflow:

- `/fields/System.Title`
- `/fields/System.Description`
- `/fields/System.AreaPath = AGROTRACE\Devops` unless overridden
- `/fields/System.IterationPath` when available
- `/fields/Microsoft.VSTS.Common.Priority = 2`
- `/fields/Custom.Team = DevOps`
- `/fields/Custom.ProgramasAgrotrace = Agrotrace`
- `/fields/System.AssignedTo` when configured
- parent relation via `System.LinkTypes.Hierarchy-Reverse`

## Failure reporting

If creation still fails, the command should:

- keep the generated Markdown visible
- keep the Azure error visible
- include a short field summary of the create defaults it attempted, at minimum:
  - `AreaPath`
  - `IterationPath` when present
  - `Priority`
  - `Custom.Team`
  - `Custom.ProgramasAgrotrace`
  - `AssignedTo`
  - parent work item target
so the user can identify which process-specific field may still be missing or invalid

## Scope limits

This follow-up should not redesign the whole script.

It should focus on:

- correcting the required create defaults
- improving normal-mode observability
- keeping behavior aligned with the current AGROTRACE process conventions shared by the user
