# Spinner UI for pr-tools — Design Spec

## Problem

Both CLI scripts (`create-pr-description` and `create-test-card`) currently output plain `[INFO]` log lines during execution. The user wants a modern, Claude Code-inspired progress UI with animated spinners and status icons.

## Goal

Add a spinner-based progress UI to both scripts, showing animated feedback during each pipeline step with clear success/failure indicators.

## Architecture

### New module: `src/lib/ui.sh`

A UI library exposing 3 functions:

- **`step_start "message"`** — Start an animated spinner: `● Message...` (yellow, pulsing dot)
- **`step_done "message"`** — Stop spinner, replace with: `✓ Message` (green)
- **`step_fail "message"`** — Stop spinner, replace with: `✗ Message` (red)

### Spinner implementation

- Background subprocess that toggles the `●` dot between bold and dim every 200ms
- Uses `\r` (carriage return) to rewrite the current line — no full screen redraw
- Stores spinner PID in global `_SPINNER_PID`
- `trap` on EXIT/INT/TERM to cleanup spinner subprocess on unexpected exit
- When non-interactive (`! -t 1` or `NO_COLOR` set), degrades to static output: just prints the final line without animation
- `step_start` automatically stops any previously running spinner (calls `step_done` implicitly if a spinner is active without being closed)
- While spinner is active, `log_error`/`log_warn` should still work — they need to clear the spinner line first, print, then resume

### Step mapping

**create-pr-description main() steps:**

1. `step_start "Validando dependencias"` → `step_done "Dependencias validadas"`
2. `step_start "Carregando configuracao"` → `step_done "Configuracao carregada"`
3. `step_start "Validando API keys"` → `step_done "API keys validadas"`
4. `step_start "Coletando contexto git"` → `step_done "Contexto git coletado"`
5. `step_start "Detectando work item"` → `step_done "Work item: #ID"`
6. `step_start "Detectando sprint"` → `step_done "Sprint: N"` or `step_done "Sem sprint ativo"`
7. `step_start "Resolvendo repositorio Azure DevOps"` → `step_done "Repositorio resolvido"`
8. `step_start "Gerando descricao via LLM"` → `step_done "Descricao gerada (provider)"`
9. Output formatting (no spinner — just prints)
10. (optional) `step_start "Criando PR no Azure DevOps"` → `step_done "PR criado"`

**create-test-card main() steps:**

1. `step_start "Validando dependencias"` → `step_done "Dependencias validadas"`
2. `step_start "Carregando configuracao"` → `step_done "Configuracao carregada"`
3. `step_start "Validando Azure PAT"` → `step_done "Azure PAT validado"`
4. `step_start "Validando API keys"` → `step_done "API keys validadas"`
5. `step_start "Resolvendo contexto Azure DevOps"` → `step_done "Contexto resolvido"`
6. `step_start "Resolvendo PR"` → `step_done "PR: #ID — Title"`
7. `step_start "Resolvendo work item"` → `step_done "Work item: #ID — Title"`
8. `step_start "Buscando alteracoes do PR"` → `step_done "Alteracoes coletadas (N arquivos)"`
9. `step_start "Buscando exemplos de test case"` → `step_done "Exemplos coletados (N)"`
10. `step_start "Gerando card via LLM"` → `step_done "Card gerado (provider)"`
11. Output formatting (no spinner — just prints)
12. (optional) `step_start "Criando test case no Azure DevOps"` → `step_done "Test case criado: #ID"`
13. (optional) `step_start "Atualizando work item para Test QA"` → `step_done "Work item atualizado"`

### Failure handling

- If a step function calls `exit 1`, the EXIT trap fires and calls `step_fail` with the current step message
- `log_error` inside a step is fine — it prints the error detail, then the step itself shows `✗`

### Non-interactive degradation

When `! -t 1` (piped output, CI, etc.):
- `step_start` prints: `  ● Message...`
- `step_done` prints: `  ✓ Message`
- `step_fail` prints: `  ✗ Message`
- No animation, no cursor manipulation

### Integration

- Both scripts source `src/lib/ui.sh` alongside the other libs
- The `main()` functions in both scripts are modified to wrap each pipeline stage with `step_start`/`step_done`
- Internal functions (`validate_dependencies`, `collect_git_context`, etc.) are NOT modified — they keep their internal `log_info`/`log_warn` calls, which will be suppressed during spinner mode (redirected to stderr or buffered)
- `log_info` calls inside step functions should be suppressed (the spinner replaces them). Only `log_error` and `log_warn` should break through.

### Files changed

- Create: `src/lib/ui.sh` (~80-100 lines)
- Modify: `src/bin/create-pr-description` — main() function only
- Modify: `src/bin/create-test-card` — main() function only
- Modify: `src/lib/common.sh` — update `log_info` to check for active spinner
- Modify: `install.sh` — add ui.sh to download list
- Modify: `src/lib/common.sh` `do_update()` — add ui.sh to lib list
- Modify: `src/bin/create-pr-description` `_pr_tools_ensure_libs` — add ui.sh
- Modify: `src/bin/create-test-card` auto-download — add ui.sh

## Constraints

- Pure bash — no external dependencies (no npm, no python)
- Must work on Linux and macOS terminals
- Must degrade gracefully when non-interactive
- Must not interfere with `--dry-run` or `--raw` output modes
- Must cleanup spinner subprocess on any exit path
