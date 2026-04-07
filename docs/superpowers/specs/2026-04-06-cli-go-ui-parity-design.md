# CLI Go UI Parity with Bash v2.9.8 - Design Spec

## Problem

The Go CLI refactor preserved the core command behavior of `prt desc` and `prt test`, but the terminal experience no longer matches the Bash implementation shipped in `v2.9.8`.

The regressions are not limited to colors or a spinner. The Bash UI had a stateful transcript with:

- a live title row whose `✦` kept animating while steps were running
- a hierarchical connector layout using `│` and `└`
- step, info, warning, error, and success messages rendered inside that same visual tree
- command-specific wording and step order
- a second visual phase in `prt test` for publishing to Azure DevOps

The current Go implementation only reproduces part of that behavior. It uses a static title animation, a separate step animation, different message ordering, and different terminal wording.

## Goal

Restore the observable terminal experience of the Bash implementation from tag `v2.9.8` for:

- `prt desc`
- `prt test`

Parity means matching:

- UI structure
- title and step animations
- visible terminal text
- order of status messages
- interactive prompt placement
- non-interactive degradation behavior

## Non-Goals

- Rewriting business logic to behave exactly like Bash where the current Go logic is already correct
- Matching dynamic output byte-for-byte when the underlying data differs
- Reworking unrelated commands outside `prt desc` and `prt test`
- Replacing the Go architecture with a shell-style implementation

## Source of Truth

The Bash implementation in tag `v2.9.8` is the source of truth for terminal behavior.

Primary references:

- `src/lib/ui.sh`
- `src/bin/create-pr-description`
- `src/bin/create-test-card`

The Go implementation should use those files as transcript references while preserving Go domain logic where appropriate.

## Current Root Cause

The mismatch comes from two layers drifting at the same time.

First, the UI renderer diverged:

- `apps/cli-go/internal/ui/ui.go` animates the title only during startup instead of while a step is active
- title and step rendering do not share a single stateful loop
- `TitleDone()` prints a closing line that the Bash version did not print automatically
- info and status lines are not fully aware of title state and line offsets

Second, the command transcript diverged:

- `apps/cli-go/internal/cli/desc.go` does not emit the same step order and wording as Bash `create-pr-description`
- `apps/cli-go/internal/cli/test.go` does not emit the same step order, wording, or second-phase publish block as Bash `create-test-card`

Restoring parity therefore requires both a UI-state fix and command transcript remapping.

## Architecture

### UI State Model

`apps/cli-go/internal/ui/ui.go` becomes a stateful terminal renderer modeled after Bash `src/lib/ui.sh`.

It keeps a single shared UI session state containing:

- whether output is interactive
- whether color is enabled
- whether a title is active
- the current title message
- how many lines have been printed below the title
- whether a step spinner is active
- the current step message

The implementation must serialize screen updates so that animation repaint and normal log output do not interleave.

### Unified Animation Loop

When both a title and a step are active, one loop is responsible for repainting both.

The loop mirrors Bash behavior:

- the step line toggles the `●` between bold and dim states
- the title line cycles through the same four sparkle frames used by Bash: `✦`, `✧`, `✦`, `·`
- the loop temporarily moves the cursor upward to repaint the title, then restores the cursor to the step line
- animation only runs in interactive mode

### Title Lifecycle

`Title(w, msg)` starts a titled visual block.

In interactive mode it prints the initial title row and records the title as active.

`Title` must not emit implicit leading or trailing blank lines. Caller-owned spacing must match the Bash transcript.

`TitleDone(w)` only clears title state. It does not print an automatic closing row. Closing rows such as `└` must be emitted by the caller at the exact points where the Bash transcript showed them.

### Status Line Behavior

When a title is active:

- `Info` renders as `  │ <dim message>`
- `Warn` renders as `  │ ⚠ <message>`
- `Error` renders as `  │ ✗ <message>`
- `Success` renders as `  │ ✓ <message>`

Each of these increments the line count below the title so the animation loop knows how far to move the cursor when repainting.

When no title is active, the functions render their standalone format.

### Step Lifecycle

`Step(w, msg)` keeps the same public shape so current callers remain simple.

It starts a spinner line using the title-aware layout when a title is active. Its stop function clears the animated line and replaces it with either:

- `  │ ✓ <message>`
- `  │ ✗ <message>`

or the standalone equivalent if no title is active.

Stopping a step while a title is active increments the line count below the title.

### Non-Interactive Degradation

When output is not a terminal, or color is disabled, the renderer keeps:

- the same wording
- the same tree structure where applicable
- the same step completion messages

But it does not use:

- carriage-return repaint
- cursor save/restore
- cursor-up movement
- animated timing loops

## Command Transcript Mapping

### `prt desc`

`apps/cli-go/internal/cli/desc.go` should emit the Bash `v2.9.8` transcript order and wording.

Required top-level title:

- `Gerando descrição do PR...`

Required step sequence:

1. `Validando dependencias` -> `Dependencias validadas`
2. `Carregando configuracao` -> `Configuracao carregada`
3. `Validando API keys` -> `API keys validadas`
4. `Validando branch` -> `Branch validada`
5. `Coletando contexto git` -> `Contexto git coletado (<branch>)`
6. `Detectando work item` -> `Work item: #<id>` or `Sem work item detectado`
7. `Detectando sprint` -> `Sprint: <n>` or `Sem sprint ativo`
8. `Resolvendo repositório Azure DevOps` -> `Repositório: <org>/<project>/<repo>` or `Repositório não-Azure (sem links de PR)`
9. `Gerando descrição via LLM` -> `Descrição gerada (<provider>/<model>)`

Required interactive work-item fallback when no ID is derived automatically:

- while the `Detectando work item` step is active, emit `Não foi possivel extrair o work item ID da branch '<branch>'.`
- prompt `ID do work item (Enter para pular):`
- finish the step as `Work item: #<id>` when the user enters a value
- otherwise finish as `Sem work item detectado`

Required auxiliary messages inside the same visual tree:

- `Diff truncado: ...`
- `Tentando provider: ...`
- `Provider ... falhou. Tentando próximo...`
- `Todos os providers falharam`

Required result block:

- title row `PR — <branch>` using the same title styling as Bash
- `Target`, `Provider`, and optional `Work Item` summary lines
- optional Azure DevOps work-item edit URL under a second `Work Item:` subtree when Azure routing is available
- `Abrir PR:` subtree with per-target rows and URLs when links are available
- PR links rendered in the same tree layout
- clipboard success line `✓ Descrição copiada para o clipboard`
- clipboard follow-up line `Título disponível acima para copiar manualmente.`
- clipboard warning line `⚠ Clipboard não disponível (pbcopy/xclip/xsel não encontrado)` when copy is unavailable

Interactive PR creation must also follow the Bash visual structure:

- second titled block `Publicar no Azure DevOps`
- blank tree separator row before the confirmation question
- prompt `Criar PR(s) no Azure DevOps?`
- cancel path rendered as `(cancelado)` inside the tree
- for each target, a blank tree separator row followed by `→ PR para <target>`
- prompt `Reviewer (email)` with the Bash placement inside the flow
- step `Criando PR → <target>`
- completion `PR criado → <target>` or `Falha ao criar PR → <target>`
- successful PR URL printed under the step line

### `prt test`

`apps/cli-go/internal/cli/test.go` should emit the Bash `v2.9.8` transcript order and wording.

The current Go requirement that `--work-item` must always be provided must be removed. Bash parity requires this resolution order:

1. explicit `--work-item`
2. linked work item resolved from the PR

If neither exists, the command should fail with a Bash-equivalent message.

Required first title:

- `Gerando card de teste...`

Required first-phase step sequence:

1. `Validando dependencias` -> `Dependencias validadas`
2. `Carregando configuracao` -> `Configuracao carregada`
3. `Validando Azure PAT` -> `Azure PAT validado`
4. `Validando API keys` -> `API keys validadas`
5. `Detectando contexto git` -> `Contexto git detectado`
6. `Resolvendo contexto Azure DevOps` -> `Azure DevOps: <org>/<project>`
7. `Resolvendo PR` -> `PR: #<id> — <title>`
8. `Resolvendo work item` -> `Work item: #<id> — <title>`
9. `Buscando alteracoes do PR` -> `Alteracoes coletadas`
10. `Buscando exemplos de test case` -> `Exemplos coletados`
11. `Preparando campos de criacao` -> `Campos resolvidos`
12. `Gerando card via LLM` -> `Card gerado (<provider>/<model>)`

Required card output block:

- title row `Test Card — PR #<id>`
- summary rows for `Provider`, `Work Item`, `AreaPath`, optional `Responsável`, and `Título`
- markdown body printed after the summary as it is today

Required second title block:

- `Publicar no Azure DevOps`

Required publish-phase behavior:

- blank tree separator row before `Criar este Test Case no Azure DevOps?`
- prompt `Criar este Test Case no Azure DevOps?`
- cancel path rendered as `(cancelado)` inside the tree when the user declines creation
- `Criando test case no Azure DevOps` -> `Test case criado: #<id>` or `Falha ao criar test case`
- only if a test case was actually created, emit a blank tree separator row before the second confirmation
- only if a test case was actually created, prompt `Atualizar o work item #<id> para Test QA?`
- if the user declines the second confirmation, render `(cancelado)` inside the tree
- if the update is confirmed and the work item does not already contain the required values, prompt using the Bash wording:
- `Effort (horas decimais, ex: 0.5) [0.5]:`
- `Real Effort (horas decimais) [<default>]:`
- optional final status `Work item #<id> atualizado para Test QA` or `Falha ao atualizar work item`

Required warnings and fallbacks must use the Bash wording where applicable, including the full manual fallback block when automatic creation fails:

- `⚠ Não foi possivel criar o Test Case automaticamente`
- raw Azure error detail on the next tree line when available
- `Campos tentados na criacao:` followed by the attempted field list
- `Use o Markdown acima para criar o card manualmente no Azure DevOps.`

The attempted field list must include the same labels used by Bash:

- `AreaPath`
- `IterationPath`
- `Priority`
- `Custom.Team`
- `Custom.ProgramasAgrotrace`
- `AssignedTo`
- `Parent`

Required non-interactive publish behavior:

- when automatic creation would otherwise be attempted, the `Publicar no Azure DevOps` title block still appears
- in that path there is no confirmation prompt
- emit `Ambiente não interativo; pulando criacao automatica do Test Case. Rode interativamente para confirmar a criacao.` inside the tree
- no publish action is attempted
- when `--no-create` is set, keep the Bash behavior: skip all publish prompts and actions
- when `--dry-run` is set, exit before the publish phase entirely

## Compatibility Rules

- Preserve current Go business logic unless the visible terminal behavior depends on a different sequencing point
- Preserve `--raw`
- Preserve `--dry-run` with Bash-equivalent visible output
- Preserve clean behavior for `NO_COLOR`
- Preserve non-interactive usage without animation
- Do not add compatibility shims for commands outside `desc` and `test`

## Dry-Run Requirements

`--dry-run` is part of the visible transcript contract and must match the Bash wording closely.

For `prt desc`, the dry-run output must remain equivalent to the Bash structure:

- separator line
- `DRY RUN - Prompt que seria enviado ao LLM`
- `[SYSTEM]`
- `[USER]`
- provider/model summary line

It must not enter the PR publication flow.

For `prt test`, the dry-run output must remain equivalent to the Bash structure:

- `[SYSTEM]`
- `[USER]`
- `[CREATE PREVIEW]`

Raw dry-run mode must keep the Bash-style plain text variant and must not enter the publish flow.

## Testing Strategy

### UI Tests

Add focused tests for `apps/cli-go/internal/ui` covering:

- title state lifecycle
- step success and failure rendering
- title-aware `Info`, `Warn`, `Error`, and `Success`
- no automatic closing row from `TitleDone()`
- non-interactive rendering without animation escape control

### Command Transcript Tests

Add transcript-oriented tests for `apps/cli-go/internal/cli/desc.go` and `apps/cli-go/internal/cli/test.go` that capture `stdout` and `stderr` and assert:

- step ordering
- visible wording
- presence of the right titled blocks
- presence of Bash-style summary sections
- publish-phase transcript for `prt test`

### Manual Verification

Run the narrowest safe checks after implementation:

- `go test ./apps/cli-go/internal/ui ./apps/cli-go/internal/cli`
- interactive smoke check for `prt desc`
- interactive smoke check for `prt test`

If live services or credentials are unavailable, manual verification should still cover the non-network transcript paths as far as possible.

## Success Criteria

The work is successful when `prt desc` and `prt test` in the Go CLI present a terminal experience that is observably aligned with Bash `v2.9.8` in:

- structure
- animation behavior
- message text
- message order
- prompt placement
- summary layout

without regressing raw or non-interactive usage.
