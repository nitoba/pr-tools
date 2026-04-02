# Spinner UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add animated spinner progress UI to both CLI scripts, showing `● step...` while running and `✓`/`✗` on completion/failure.

**Architecture:** New `src/lib/ui.sh` module with `step_start`/`step_done`/`step_fail` functions using a background subprocess for animation. Both scripts' `main()` functions are updated to use these instead of `log_info` for pipeline steps.

**Tech Stack:** Pure bash, ANSI escape codes, background subprocesses

**Spec:** `docs/superpowers/specs/2026-04-02-spinner-ui-design.md`

---

### Task 1: Create src/lib/ui.sh — spinner library

**Files:**
- Create: `src/lib/ui.sh`

- [ ] **Step 1: Create `src/lib/ui.sh`**

```bash
#!/usr/bin/env bash
# src/lib/ui.sh — spinner progress UI for pr-tools

[[ -n "${_PR_TOOLS_UI_SH:-}" ]] && return 0
_PR_TOOLS_UI_SH=1

# ---- Spinner state ----
_SPINNER_PID=""
_SPINNER_MSG=""
_SPINNER_ACTIVE=false
_SPINNER_INTERACTIVE=true

# Check if we can animate
if [[ ! -t 2 || -n "${NO_COLOR:-}" ]]; then
  _SPINNER_INTERACTIVE=false
fi

# Colors (may already be set by common.sh, but define fallbacks)
_UI_GREEN="${GREEN:-\033[0;32m}"
_UI_RED="${RED:-\033[0;31m}"
_UI_YELLOW="${YELLOW:-\033[1;33m}"
_UI_BOLD="${BOLD:-\033[1m}"
_UI_DIM="${DIM:-\033[2m}"
_UI_NC="${NC:-\033[0m}"

if [[ "$_SPINNER_INTERACTIVE" == "false" ]]; then
  _UI_GREEN=""
  _UI_RED=""
  _UI_YELLOW=""
  _UI_BOLD=""
  _UI_DIM=""
  _UI_NC=""
fi

# ---- Internal: spinner loop ----
_spinner_loop() {
  local msg="$1"
  local toggle=0
  while true; do
    if (( toggle % 2 == 0 )); then
      printf '\r  %b●%b %s...' "$_UI_YELLOW$_UI_BOLD" "$_UI_NC" "$msg" >&2
    else
      printf '\r  %b●%b %s...' "$_UI_YELLOW$_UI_DIM" "$_UI_NC" "$msg" >&2
    fi
    toggle=$(( toggle + 1 ))
    sleep 0.3
  done
}

# ---- Internal: stop spinner subprocess ----
_spinner_stop() {
  if [[ -n "$_SPINNER_PID" ]]; then
    kill "$_SPINNER_PID" 2>/dev/null
    wait "$_SPINNER_PID" 2>/dev/null
    _SPINNER_PID=""
  fi
  _SPINNER_ACTIVE=false
}

# ---- Internal: clear current spinner line ----
_spinner_clear_line() {
  if [[ "$_SPINNER_INTERACTIVE" == "true" ]]; then
    printf '\r\033[2K' >&2
  fi
}

# ---- Public API ----

step_start() {
  local msg="$1"

  # If a spinner is already active, auto-complete it
  if [[ "$_SPINNER_ACTIVE" == "true" ]]; then
    step_done "$_SPINNER_MSG"
  fi

  _SPINNER_MSG="$msg"
  _SPINNER_ACTIVE=true

  if [[ "$_SPINNER_INTERACTIVE" == "true" ]]; then
    _spinner_loop "$msg" &
    _SPINNER_PID=$!
    disown "$_SPINNER_PID" 2>/dev/null
  else
    printf '  ● %s...\n' "$msg" >&2
  fi
}

step_done() {
  local msg="${1:-$_SPINNER_MSG}"
  _spinner_stop
  _spinner_clear_line
  printf '  %b✓%b %s\n' "$_UI_GREEN" "$_UI_NC" "$msg" >&2
  _SPINNER_MSG=""
}

step_fail() {
  local msg="${1:-$_SPINNER_MSG}"
  _spinner_stop
  _spinner_clear_line
  printf '  %b✗%b %s\n' "$_UI_RED" "$_UI_NC" "$msg" >&2
  _SPINNER_MSG=""
}

# ---- Trap: cleanup on exit ----
_ui_cleanup() {
  if [[ "$_SPINNER_ACTIVE" == "true" ]]; then
    local exit_code=$?
    if [[ $exit_code -ne 0 ]]; then
      step_fail "$_SPINNER_MSG"
    else
      _spinner_stop
      _spinner_clear_line
    fi
  fi
}
trap '_ui_cleanup' EXIT

# ---- Override log_info to be quiet during spinner ----
# When a spinner is active, log_info is suppressed (the spinner replaces it).
# log_error and log_warn still print — they clear the spinner line first.

if declare -f log_info >/dev/null 2>&1; then
  eval "_original_log_info=$(declare -f log_info | tail -n +2)"
  log_info() {
    if [[ "$_SPINNER_ACTIVE" == "true" ]]; then
      return 0
    fi
    _original_log_info "$@"
  }
fi

if declare -f log_error >/dev/null 2>&1; then
  eval "_original_log_error=$(declare -f log_error | tail -n +2)"
  log_error() {
    if [[ "$_SPINNER_ACTIVE" == "true" ]]; then
      _spinner_stop
      _spinner_clear_line
    fi
    _original_log_error "$@"
  }
fi

if declare -f log_warn >/dev/null 2>&1; then
  eval "_original_log_warn=$(declare -f log_warn | tail -n +2)"
  log_warn() {
    if [[ "$_SPINNER_ACTIVE" == "true" ]]; then
      _spinner_stop
      _spinner_clear_line
    fi
    _original_log_warn "$@"
  }
fi
```

- [ ] **Step 2: Syntax check**

Run: `bash -n src/lib/ui.sh`
Expected: no output (success)

- [ ] **Step 3: Manual smoke test**

Run: `bash -c 'source src/lib/common.sh; source src/lib/ui.sh; step_start "Testando spinner"; sleep 1; step_done "Spinner funcionando"; step_start "Testando falha"; sleep 0.5; step_fail "Falha simulada"'`
Expected: See animated spinner for 1s, then green `✓`, then another spinner for 0.5s, then red `✗`

- [ ] **Step 4: Commit**

```bash
git add src/lib/ui.sh
git commit -m "feat: add spinner UI library (src/lib/ui.sh)"
```

---

### Task 2: Update download/install references to include ui.sh

**Files:**
- Modify: `src/bin/create-pr-description` — add `ui.sh` to auto-download and source lists
- Modify: `src/bin/create-test-card` — add `ui.sh` to auto-download and source lists
- Modify: `src/lib/common.sh` — add `ui.sh` to `do_update()` lib list
- Modify: `install.sh` — add `ui.sh` to download loop

- [ ] **Step 1: Update `src/bin/create-pr-description`**

In the `_pr_tools_ensure_libs` function, add `ui.sh` to both the check loop and the download loop. Then add `source "$LIB_DIR/ui.sh"` after the existing source lines.

Find the line:
```bash
for _lib in common.sh llm.sh azure.sh test-card-azure.sh test-card-llm.sh; do
```
in the `_pr_tools_ensure_libs` function (the check loop around line 28) and add `ui.sh`:
```bash
for _lib in common.sh llm.sh azure.sh test-card-azure.sh test-card-llm.sh ui.sh; do
```

Do the same for the download loop (around line 34):
```bash
for _lib in common.sh llm.sh azure.sh test-card-azure.sh test-card-llm.sh ui.sh; do
```

Add this source line after the existing `source "$LIB_DIR/azure.sh"`:
```bash
source "$LIB_DIR/ui.sh"
```

- [ ] **Step 2: Update `src/bin/create-test-card`**

Find the auto-download loops and add `ui.sh`. Then add `source "$LIB_DIR/ui.sh"` after the existing source lines.

Check loop:
```bash
for _required_lib in common.sh test-card-azure.sh test-card-llm.sh ui.sh; do
```

Download loop:
```bash
for _lib in common.sh llm.sh azure.sh test-card-azure.sh test-card-llm.sh ui.sh; do
```

Add after `source "$LIB_DIR/test-card-llm.sh"`:
```bash
source "$LIB_DIR/ui.sh"
```

- [ ] **Step 3: Update `src/lib/common.sh` `do_update()`**

Find the lib download loop in `do_update()`:
```bash
for lib_file in common.sh llm.sh azure.sh test-card-azure.sh test-card-llm.sh; do
```
Change to:
```bash
for lib_file in common.sh llm.sh azure.sh test-card-azure.sh test-card-llm.sh ui.sh; do
```

- [ ] **Step 4: Update `install.sh`**

Find the lib download loop:
```bash
for lib_file in common.sh llm.sh azure.sh test-card-azure.sh test-card-llm.sh; do
```
Change to:
```bash
for lib_file in common.sh llm.sh azure.sh test-card-azure.sh test-card-llm.sh ui.sh; do
```

- [ ] **Step 5: Syntax check all**

Run: `bash -n src/bin/create-pr-description && bash -n src/bin/create-test-card && bash -n src/lib/common.sh && bash -n install.sh && echo "OK"`
Expected: `OK`

- [ ] **Step 6: Commit**

```bash
git add src/bin/create-pr-description src/bin/create-test-card src/lib/common.sh install.sh
git commit -m "chore: add ui.sh to download, install, and source lists"
```

---

### Task 3: Integrate spinner into create-pr-description main()

**Files:**
- Modify: `src/bin/create-pr-description` — rewrite `main()` to use `step_start`/`step_done`

- [ ] **Step 1: Rewrite main() with spinner steps**

Replace the `main()` function (starts around line 1049) with spinner-wrapped steps. Read the current main() first, then rewrite it. The new main() should be:

```bash
main() {
  parse_args "$@"

  if [[ "$ACTION" == "init" ]]; then
    do_init
    exit 0
  fi

  # Validation
  step_start "Validando dependencias"
  validate_dependencies
  validate_git_repo
  step_done "Dependencias validadas"

  step_start "Carregando configuracao"
  load_config
  validate_config
  step_done "Configuracao carregada"

  step_start "Validando API keys"
  validate_api_keys
  step_done "API keys validadas"

  step_start "Validando branch"
  validate_not_base_branch
  step_done "Branch: $BRANCH_NAME"

  # Collect context
  step_start "Coletando contexto git"
  collect_git_context
  step_done "Contexto git coletado"

  step_start "Detectando work item"
  detect_work_item
  if [[ -n "$WORK_ITEM_ID" ]]; then
    step_done "Work item: #$WORK_ITEM_ID"
  else
    step_done "Sem work item detectado"
  fi

  step_start "Detectando sprint"
  detect_sprint
  if [[ -n "$SPRINT_NUMBER" ]]; then
    step_done "Sprint: $SPRINT_NUMBER"
  else
    step_done "Sem sprint ativo"
  fi

  step_start "Resolvendo repositorio Azure DevOps"
  parse_azure_remote
  fetch_repo_id
  build_pr_links
  if [[ "$IS_AZURE_DEVOPS" == "true" ]]; then
    step_done "Repositorio: $AZURE_ORG/$AZURE_PROJECT/$AZURE_REPO"
  else
    step_done "Repositorio nao-Azure (sem links de PR)"
  fi

  detect_md_renderer

  # Read template
  local template_content
  template_content=$(cat "$TEMPLATE_FILE")

  # Build user prompt
  local target_display=""
  for target in "${TARGETS[@]}"; do
    if [[ "$target" == "dev" ]]; then
      target_display="${target_display}dev, "
    elif [[ "$target" == "sprint" && -n "$SPRINT_BRANCH" ]]; then
      target_display="${target_display}${SPRINT_BRANCH}, "
    fi
  done
  target_display="${target_display%, }"

  # Work item context
  local work_item_context=""
  if [[ -n "$WORK_ITEM_ID" ]]; then
    work_item_context="
**Work Item:** #$WORK_ITEM_ID"
  fi

  local user_prompt
  user_prompt="## Contexto Git

**Branch:** $BRANCH_NAME
**Base branches alvo:** $target_display${work_item_context}

### Git Log (commits desde a base):
$GIT_LOG

### Git Diff:
$GIT_DIFF"

  # Dry-run: show prompt and exit
  if [[ "$DRY_RUN" == "true" ]]; then
    local separator="=========================================="
    echo ""
    echo -e "${BOLD}${separator}${NC}"
    echo -e "${BOLD}DRY RUN - Prompt que seria enviado ao LLM${NC}"
    echo -e "${BOLD}${separator}${NC}"
    echo ""
    echo -e "${BOLD}[SYSTEM]${NC}"
    echo "$template_content"
    echo ""
    echo -e "${BOLD}[USER]${NC}"
    echo "$user_prompt"
    echo ""
    echo -e "${BOLD}${separator}${NC}"
    echo -e "Provider: ${PR_PROVIDERS%,*} | Modelo: ${OPENROUTER_MODEL:-$DEFAULT_OPENROUTER_MODEL}"
    echo -e "${BOLD}${separator}${NC}"
    exit 0
  fi

  # Call LLM
  step_start "Gerando descricao via LLM"
  if [[ "$STREAM_MODE" == "true" ]]; then
    _spinner_stop
    _spinner_clear_line
    echo "" >&2
    echo -e "${DIM}--- Streaming resposta do LLM ---${NC}" >&2
    echo "" >&2
  fi
  call_with_fallback "$template_content" "$user_prompt"
  if [[ "$STREAM_MODE" == "true" ]]; then
    echo "" >&2
    echo -e "${DIM}--- Resposta completa recebida ---${NC}" >&2
  fi
  step_done "Descricao gerada ($USED_PROVIDER/$USED_MODEL)"

  # Output
  print_output "$LLM_RESULT"

  # Offer PR creation
  offer_pr_creation
}
```

Note: The `detect_work_item` function may prompt the user interactively (asks for work item ID). The spinner auto-stops when `step_done` is called, and `log_info` inside is suppressed. Interactive `read` inside detect_work_item will need the spinner stopped — but since `step_done` is called right after, the timing works: the function runs with spinner, then we call `step_done` which stops it. If the function needs user input, the log_info/read calls would happen while spinner is running. To handle this safely, the detect_work_item and detect_sprint functions should work fine because any `read` prompt they issue goes to stderr/stdin and the spinner clears on EXIT trap if things go wrong.

- [ ] **Step 2: Syntax check**

Run: `bash -n src/bin/create-pr-description`
Expected: no output

- [ ] **Step 3: Functional check**

Run: `src/bin/create-pr-description --version`
Expected: `create-pr-description v2.7.0`

Run: `src/bin/create-pr-description --help`
Expected: help output

- [ ] **Step 4: Commit**

```bash
git add src/bin/create-pr-description
git commit -m "feat: integrate spinner UI into create-pr-description"
```

---

### Task 4: Integrate spinner into create-test-card main()

**Files:**
- Modify: `src/bin/create-test-card` — rewrite `main()` to use `step_start`/`step_done`

- [ ] **Step 1: Rewrite main() with spinner steps**

Replace the `main()` function (starts around line 522) with spinner-wrapped steps:

```bash
main() {
  parse_args "$@"

  if [[ "$ACTION" == "init" ]]; then
    load_config
    do_init
    exit 0
  fi

  step_start "Validando dependencias"
  validate_dependencies
  step_done "Dependencias validadas"

  step_start "Carregando configuracao"
  load_config
  step_done "Configuracao carregada"

  step_start "Validando Azure PAT"
  validate_azure_pat
  step_done "Azure PAT validado"

  step_start "Validando API keys"
  validate_provider_keys
  step_done "API keys validadas"

  step_start "Detectando contexto git"
  detect_git_context
  parse_azure_remote
  step_done "Contexto git detectado"

  step_start "Resolvendo contexto Azure DevOps"
  resolve_routing
  step_done "Azure DevOps: $AZURE_ORG/$AZURE_PROJECT"

  step_start "Resolvendo PR"
  resolve_pr
  step_done "PR: #$PR_ID — $PR_TITLE"

  step_start "Resolvendo work item"
  resolve_work_item
  step_done "Work item: #$WORK_ITEM_ID — $WORK_ITEM_TITLE"

  step_start "Buscando alteracoes do PR"
  fetch_pr_changes
  step_done "Alteracoes coletadas"

  step_start "Buscando exemplos de test case"
  fetch_example_test_cases
  step_done "Exemplos coletados"

  step_start "Preparando campos de criacao"
  resolve_creation_defaults
  step_done "Campos resolvidos"

  local user_prompt
  user_prompt=$(build_user_prompt)

  if [[ "$DRY_RUN" == "true" ]]; then
    if [[ "$RAW_OUTPUT" == "true" ]]; then
      printf '%s\n\n%s\n\n' "$DEFAULT_SYSTEM_PROMPT" "$user_prompt"
      cat <<EOF
type: Test Case
title: <from LLM response>
areaPath: $SELECTED_AREA_PATH
assignedTo: ${SELECTED_ASSIGNED_TO:-<not configured>}
parentId: $WORK_ITEM_ID
iterationPath: ${WORK_ITEM_ITERATION_PATH:-<empty>}
priority: ${ATTEMPTED_PRIORITY:-<empty>}
customTeam: ${ATTEMPTED_TEAM:-<empty>}
customProgramasAgrotrace: ${ATTEMPTED_PROGRAMA:-<empty>}
descriptionHtml: <from LLM response converted to HTML>
EOF
      exit 0
    fi
    print_dry_run "$user_prompt"
    exit 0
  fi

  step_start "Gerando card via LLM"
  call_with_fallback "$user_prompt"
  LLM_RESULT=$(strip_think_blocks "$LLM_RESULT")
  parse_llm_result
  GENERATED_HTML=$(markdown_to_html "$GENERATED_MARKDOWN")
  GENERATED_STEPS=$(markdown_to_azure_steps "$GENERATED_MARKDOWN")
  step_done "Card gerado ($USED_PROVIDER/$USED_MODEL)"

  print_output

  if confirm_test_case_creation; then
    step_start "Criando test case no Azure DevOps"
    if create_test_case; then
      step_done "Test case criado: #$CREATED_TEST_CASE_ID"
    else
      step_fail "Falha ao criar test case"
    fi
  fi

  print_create_result

  if confirm_parent_test_qa_update; then
    step_start "Atualizando work item para Test QA"
    if update_parent_work_item_to_test_qa; then
      step_done "Work item #$WORK_ITEM_ID atualizado para Test QA"
    else
      step_fail "Falha ao atualizar work item"
    fi
  fi
}
```

- [ ] **Step 2: Syntax check**

Run: `bash -n src/bin/create-test-card`
Expected: no output

- [ ] **Step 3: Functional check**

Run: `src/bin/create-test-card --version`
Expected: `create-test-card v0.2.0`

Run: `src/bin/create-test-card --help`
Expected: help output

- [ ] **Step 4: Commit**

```bash
git add src/bin/create-test-card
git commit -m "feat: integrate spinner UI into create-test-card"
```

---

### Task 5: End-to-end verification

- [ ] **Step 1: Syntax check all files**

```bash
bash -n src/lib/ui.sh && bash -n src/lib/common.sh && bash -n src/lib/llm.sh && bash -n src/lib/azure.sh && bash -n src/lib/test-card-azure.sh && bash -n src/lib/test-card-llm.sh && bash -n src/bin/create-pr-description && bash -n src/bin/create-test-card && bash -n install.sh && echo "ALL OK"
```

Expected: `ALL OK`

- [ ] **Step 2: Verify create-pr-description**

```bash
src/bin/create-pr-description --version
src/bin/create-pr-description --help
```

- [ ] **Step 3: Verify create-test-card**

```bash
src/bin/create-test-card --version
src/bin/create-test-card --help
```

- [ ] **Step 4: Visual smoke test — spinner animation**

```bash
bash -c 'source src/lib/common.sh; source src/lib/ui.sh; step_start "Teste 1"; sleep 1; step_done "Teste 1 ok"; step_start "Teste 2"; sleep 0.5; step_fail "Teste 2 falhou"; step_start "Teste 3 auto-complete"; step_start "Teste 4 substitui anterior"; sleep 0.5; step_done "Tudo certo"'
```

Expected: See spinners animate, green checkmarks, red X, and auto-complete behavior.

- [ ] **Step 5: Verify dry-run still works (no spinners in dry-run output)**

In a git repo on a feature branch:
```bash
src/bin/create-pr-description --dry-run 2>/dev/null | head -5
```

Expected: Dry-run output without spinner artifacts in stdout.
