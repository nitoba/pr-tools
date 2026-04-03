# Modular Decomposition Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Decompose `bin/create-pr-description` (2401 lines) into reusable modules under `lib/`, and update `bin/create-test-card` to share common utilities.

**Architecture:** Extract shared functions into `lib/common.sh`, LLM provider logic into `lib/llm.sh`, and Azure DevOps integration into `lib/azure.sh`. Both scripts source the libs and become thin orchestrators. Global variables continue to work via shared shell scope.

**Tech Stack:** Bash, `source` for module loading, relative path resolution via `BASH_SOURCE`.

**Spec:** `docs/superpowers/specs/2026-04-01-modular-decomposition-design.md`

---

### Task 1: Create lib/common.sh — shared utilities

**Files:**

- Create: `lib/common.sh`

This file contains all logging, prompting, env persistence, config loading, validation, and update functions shared between both scripts.

- [ ] **Step 1: Create `lib/common.sh`**

Create `lib/common.sh` by assembling functions from both scripts. Use `create-test-card`'s versions where they're more robust (set_env_var, log_info with RAW_OUTPUT). Key functions:

- Source guard: `_PR_TOOLS_COMMON_SH`
- Colors: RED, YELLOW, GREEN, CYAN, BOLD, DIM, NC (with NO_COLOR support)
- `log_error()`, `log_warn()`, `log_info()` (with RAW_OUTPUT check), `log_success()`, `debug_log()`
- `set_env_var()` — from create-test-card (lines 226-255, does mkdir+touch)
- `ensure_env_key_comment()` — from create-test-card (lines 257-265)
- `confirm_overwrite()` — from create-pr-description (lines 375-386)
- `prompt_value()` — from create-pr-description (lines 390-420)
- `prompt_yn()` — from create-pr-description (lines 423-437)
- `prompt_choice()` — from create-pr-description (lines 441-459)
- `test_provider_key()` — from create-pr-description (lines 462-518)
- `test_azure_pat()` — from create-pr-description (lines 521-533)
- `validate_dependencies()` — from create-pr-description (lines 840-851)
- `load_config()` — from create-pr-description (lines 900-943). Keep the STREAM_MODE logic; it's a no-op if the variable doesn't exist.
- `do_update()` — from create-pr-description (lines 215-260), **parameterized**: takes `script_name` and `current_version` as arguments so both scripts can reuse it. The function resolves `$0` to find the script path and uses `REPO_URL` global.

Read the source lines from each script, copy them verbatim (adjusting only `do_update` to accept parameters), and assemble into a single file with the guard at the top.

- [ ] **Step 2: Syntax check**

Run: `bash -n lib/common.sh`
Expected: no output (success)

- [ ] **Step 3: Commit**

```bash
git add lib/common.sh
git commit -m "refactor: extract shared utilities into lib/common.sh"
```

---

### Task 2: Create lib/llm.sh — LLM provider logic

**Files:**

- Create: `lib/llm.sh`

Extract all LLM-related functions from `bin/create-pr-description`. These functions depend on globals (`STREAM_MODE`, `PR_PROVIDERS`, API keys, model names, `USED_PROVIDER`, `USED_MODEL`, `LLM_RESULT`) which remain defined in the orchestrator.

- [ ] **Step 1: Create `lib/llm.sh`**

Source guard: `_PR_TOOLS_LLM_SH`. Source `common.sh` for logging.

Extract these functions verbatim from `bin/create-pr-description`:

- `get_provider_config()` (lines 1301-1333)
- `build_openai_compatible_payload()` (lines 1335-1378)
- `execute_openai_compatible_request()` (lines 1380-1401)
- `normalize_llm_content()` (lines 1407-1409)
- `parse_openai_sse_stream()` (lines 1413-1459)
- `parse_gemini_sse_stream()` (lines 1461-1496)
- `execute_openai_compatible_stream_request()` (lines 1498-1539)
- `execute_gemini_stream_request()` (lines 1541-1570)
- `is_groq_reasoning_format_retryable_error()` (lines 1572-1591)
- `call_llm_api()` (lines 1593-1733)
- `call_gemini_api()` (lines 1735-1857)
- `call_with_fallback()` (lines 1859-1900)

No modifications needed — these functions already communicate via globals.

- [ ] **Step 2: Syntax check**

Run: `bash -n lib/llm.sh`
Expected: no output (success)

- [ ] **Step 3: Commit**

```bash
git add lib/llm.sh
git commit -m "refactor: extract LLM provider logic into lib/llm.sh"
```

---

### Task 3: Create lib/azure.sh — Azure DevOps integration

**Files:**

- Create: `lib/azure.sh`

Extract Azure DevOps functions from `bin/create-pr-description`. These handle remote URL parsing, repo ID caching, PR link building, and PR creation via API.

- [ ] **Step 1: Create `lib/azure.sh`**

Source guard: `_PR_TOOLS_AZURE_SH`. Source `common.sh` for logging and prompts.

Extract these functions verbatim from `bin/create-pr-description`:

- `parse_azure_remote()` (lines 1153-1201)
- `get_cached_repo_id()` (lines 1203-1210)
- `cache_repo_id()` (lines 1212-1221)
- `fetch_repo_id()` (lines 1223-1268)
- `build_pr_links()` (lines 1270-1297)
- `resolve_reviewer_id()` (lines 1929-1975)
- `create_pr_via_api()` (lines 1977-2076)
- `offer_pr_creation()` (lines 2078-2124)

No modifications needed — all communicate via globals defined by the orchestrator.

- [ ] **Step 2: Syntax check**

Run: `bash -n lib/azure.sh`
Expected: no output (success)

- [ ] **Step 3: Commit**

```bash
git add lib/azure.sh
git commit -m "refactor: extract Azure DevOps integration into lib/azure.sh"
```

---

### Task 4: Rewrite bin/create-pr-description as orchestrator

**Files:**

- Modify: `bin/create-pr-description`

Replace the 2401-line monolith with a ~700-line orchestrator that sources the three libs and keeps only script-specific logic.

- [ ] **Step 1: Rewrite the script**

The new script structure:

```bash
#!/usr/bin/env bash
set -euo pipefail

# ============================================================
# create-pr-description — orchestrator
# Sources shared libs for common, LLM, and Azure functionality.
# ============================================================

VERSION="2.7.0"
CONFIG_DIR="$HOME/.config/pr-tools"
ENV_FILE="$CONFIG_DIR/.env"
TEMPLATE_FILE="$CONFIG_DIR/pr-template.md"
CACHE_FILE="$CONFIG_DIR/.cache"
REPO_URL="https://raw.githubusercontent.com/nitoba/pr-tools/main"

# ---- Source libs ----
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB_DIR="${SCRIPT_DIR}/../lib/pr-tools"
if [[ ! -d "$LIB_DIR" ]]; then
  LIB_DIR="${SCRIPT_DIR}/../lib"
fi

for _lib in common.sh llm.sh azure.sh; do
  if [[ ! -f "$LIB_DIR/$_lib" ]]; then
    echo "[ERRO] Biblioteca nao encontrada: $LIB_DIR/$_lib" >&2
    echo "[ERRO] Execute o instalador novamente ou verifique a estrutura do projeto." >&2
    exit 1
  fi
  source "$LIB_DIR/$_lib"
done

# ---- Default provider config ----
DEFAULT_PROVIDERS="openrouter,groq,gemini"
DEFAULT_OPENROUTER_MODEL="meta-llama/llama-3.3-70b-instruct:free"
DEFAULT_GROQ_MODEL="llama-3.3-70b-versatile"
DEFAULT_GEMINI_MODEL="gemini-3.1-flash-lite-preview"

# ---- Global state ----
# (all existing globals from lines 31-50 and 139-143)
...

# ---- Default Template (lines 72-116, verbatim) ----
# ---- Default .env (lines 120-135, verbatim) ----

# ---- Script-specific functions (kept in this file) ----
# show_help()           — lines 145-175
# parse_args()          — lines 262-371
# run_setup_wizard()    — lines 538-790
# do_init()             — lines 792-829 (calls lib's do_update via wrapper)
# validate_git_repo()   — lines 833-838
# validate_not_base_branch() — lines 853-876
# validate_config()     — lines 878-884
# validate_api_keys()   — lines 886-896
# collect_git_context() — lines 947-1070
# detect_sprint()       — lines 1074-1117
# detect_work_item()    — lines 1121-1149
# detect_clipboard()    — lines 1904-1916
# copy_to_clipboard()   — lines 1918-1925
# detect_md_renderer()  — lines 2128-2143
# render_markdown()     — lines 2145-2156
# parse_title_and_body() — lines 2160-2223
# print_output()        — lines 2225-2296
# main()                — lines 2300-2397

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi
```

Key changes:

1. Add lib sourcing preamble (~15 lines)
2. Remove all functions now in lib/common.sh, lib/llm.sh, lib/azure.sh
3. Keep all script-specific functions verbatim
4. `do_init()` stays but uses `do_update` from common.sh (parameterized)
5. Bump VERSION to 2.7.0

The do_init function should call `do_update` from common like this — the `--update` case in `parse_args` changes to:

```bash
--update)
  do_update "create-pr-description" "$VERSION" "$REPO_URL"
  exit 0
  ;;
```

- [ ] **Step 2: Syntax check**

Run: `bash -n bin/create-pr-description`
Expected: no output (success)

- [ ] **Step 3: Functional check**

Run: `bin/create-pr-description --help`
Expected: help text displays correctly

Run: `bin/create-pr-description --version`
Expected: `create-pr-description v2.7.0`

- [ ] **Step 4: Commit**

```bash
git add bin/create-pr-description
git commit -m "refactor: rewrite create-pr-description as orchestrator sourcing lib modules"
```

---

### Task 5: Update bin/create-test-card to source lib/common.sh

**Files:**

- Modify: `bin/create-test-card`

Replace duplicated utility functions with sourced versions from `lib/common.sh`. Keep the script's own LLM and Azure API code unchanged.

- [ ] **Step 1: Add lib sourcing preamble**

After the global variable declarations (after line ~52), add:

```bash
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB_DIR="${SCRIPT_DIR}/../lib/pr-tools"
if [[ ! -d "$LIB_DIR" ]]; then
  LIB_DIR="${SCRIPT_DIR}/../lib"
fi

if [[ -f "$LIB_DIR/common.sh" ]]; then
  source "$LIB_DIR/common.sh"
fi
```

Note: `create-test-card` uses a soft source (if exists) so it can still work standalone if lib isn't available — the functions are defined locally as fallbacks.

- [ ] **Step 2: Remove duplicated functions**

Remove these functions from `create-test-card` since they now come from `lib/common.sh`:

- `log_error()` (lines 166-168)
- `log_warn()` (lines 170-172)
- `log_info()` (lines 174-180)
- `log_success()` (lines 182-184)
- `debug_log()` (lines 186-190)
- `set_env_var()` (lines 226-255)
- `ensure_env_key_comment()` (lines 257-265)
- `prompt_value()` (lines 306-335)
- `prompt_yn()` (lines 337-351)
- `test_azure_pat()` (lines 353-362)
- `test_provider_key()` (lines 364-408)
- `validate_dependencies()` (lines 558-570)
- `load_config()` (lines 572-597)

Keep `do_update()` but refactor it to call `do_update` from common.sh:

```bash
do_update() {
  do_update "create-test-card" "$VERSION" "$REPO_URL"
}
```

Wait — this creates a recursive call. Instead rename the local wrapper or call the common function directly in parse_args:

```bash
--update)
  do_update "create-test-card" "$VERSION" "$REPO_URL"
  exit 0
  ;;
```

And remove the local `do_update()` function entirely.

Bump VERSION to 0.2.0.

- [ ] **Step 3: Syntax check**

Run: `bash -n bin/create-test-card`
Expected: no output (success)

- [ ] **Step 4: Functional check**

Run: `bin/create-test-card --help`
Expected: help text displays correctly

Run: `bin/create-test-card --version`
Expected: `create-test-card v0.2.0`

- [ ] **Step 5: Commit**

```bash
git add bin/create-test-card
git commit -m "refactor: use lib/common.sh for shared utilities in create-test-card"
```

---

### Task 6: Update install.sh

**Files:**

- Modify: `install.sh`

Add download and installation of `lib/` files to `~/.local/lib/pr-tools/`.

- [ ] **Step 1: Add lib installation**

After the existing script download section (after line 77), add:

```bash
# Download libs
LIB_INSTALL_DIR="$HOME/.local/lib/pr-tools"
mkdir -p "$LIB_INSTALL_DIR"
log_info "Diretorio de libs: $LIB_INSTALL_DIR"

for lib_file in common.sh llm.sh azure.sh; do
  log_info "Baixando lib/$lib_file..."
  tmp_lib=$(mktemp)
  if curl -fsSL "$RAW_URL/lib/$lib_file" -o "$tmp_lib"; then
    mv "$tmp_lib" "$LIB_INSTALL_DIR/$lib_file"
    log_success "Lib instalada: $LIB_INSTALL_DIR/$lib_file"
  else
    rm -f "$tmp_lib"
    log_error "Falha ao baixar lib/$lib_file."
    exit 1
  fi
done
```

- [ ] **Step 2: Syntax check**

Run: `bash -n install.sh`
Expected: no output (success)

- [ ] **Step 3: Commit**

```bash
git add install.sh
git commit -m "refactor: install lib/ modules alongside scripts"
```

---

### Task 7: End-to-end verification

- [ ] **Step 1: Syntax check all files**

```bash
bash -n lib/common.sh && bash -n lib/llm.sh && bash -n lib/azure.sh && bash -n bin/create-pr-description && bash -n bin/create-test-card && bash -n install.sh && echo "ALL OK"
```

Expected: `ALL OK`

- [ ] **Step 2: Verify create-pr-description**

```bash
bin/create-pr-description --help
bin/create-pr-description --version
```

Expected: help output and `create-pr-description v2.7.0`

- [ ] **Step 3: Verify create-test-card**

```bash
bin/create-test-card --help
bin/create-test-card --version
```

Expected: help output and `create-test-card v0.2.0`

- [ ] **Step 4: Verify dry-run (if in a git repo with a feature branch)**

```bash
bin/create-pr-description --dry-run
```

Expected: shows prompt without calling LLM

- [ ] **Step 5: Final commit if needed**

If any fixes were required during verification, commit them:

```bash
git add -A
git commit -m "fix: post-refactor adjustments from verification"
```
