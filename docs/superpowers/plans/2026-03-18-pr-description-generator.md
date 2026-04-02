# PR Description Generator - Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a shell script that generates PR descriptions using LLM APIs (OpenRouter/Groq) from git context, with clickable Azure DevOps PR creation links.

**Architecture:** Single bash script (`create-pr-description`) + external template (`pr-template.md`) + env config (`.env`) + cache file (`.cache`). The script collects git diff/log, calls an OpenAI-compatible REST API via curl, and outputs formatted PR description with Azure DevOps links.

**Tech Stack:** Bash, curl, jq, git

**Spec:** `docs/superpowers/specs/2026-03-18-pr-description-generator-design.md`

---

## File Structure

| File                                 | Responsibility                                                                                            |
| ------------------------------------ | --------------------------------------------------------------------------------------------------------- |
| `~/.local/bin/create-pr-description` | Main script: CLI parsing, validation, git context collection, LLM API calls, output formatting, clipboard |
| `~/.config/pr-tools/pr-template.md`  | LLM system prompt with PR description format instructions                                                 |
| `~/.config/pr-tools/.env`            | API keys, provider config, Azure PAT                                                                      |
| `~/.config/pr-tools/.cache`          | Cached repositoryId per remote URL                                                                        |

The script is a single file (~400 lines). It is organized into clearly named functions, each with one responsibility. No external dependencies beyond `bash`, `curl`, `jq`, and `git`.

---

## Chunk 1: Core script skeleton, CLI parsing, validation, and --init

### Task 1: Create script file with shebang, constants, and helper functions

**Files:**

- Create: `~/.local/bin/create-pr-description`

- [ ] **Step 1: Create the script file with base structure**

```bash
#!/usr/bin/env bash
set -euo pipefail

# ============================================================
# create-pr-description
# Generates PR descriptions using LLM APIs (OpenRouter/Groq)
# ============================================================

VERSION="1.0.0"
CONFIG_DIR="$HOME/.config/pr-tools"
ENV_FILE="$CONFIG_DIR/.env"
TEMPLATE_FILE="$CONFIG_DIR/pr-template.md"
CACHE_FILE="$CONFIG_DIR/.cache"

# Default provider config
DEFAULT_PROVIDERS="openrouter,groq"
DEFAULT_OPENROUTER_MODEL="google/gemini-2.5-flash:free"
DEFAULT_GROQ_MODEL="llama-3.3-70b-versatile"

# Colors for output
RED='\033[0;31m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

# ---- Helper functions ----

log_error() {
  echo -e "${RED}[ERRO]${NC} $1" >&2
}

log_warn() {
  echo -e "${YELLOW}[AVISO]${NC} $1" >&2
}

log_info() {
  echo -e "${CYAN}[INFO]${NC} $1"
}

log_success() {
  echo -e "${GREEN}[OK]${NC} $1"
}
```

- [ ] **Step 2: Make it executable**

Run: `chmod +x ~/.local/bin/create-pr-description`
Expected: No output, file is now executable.

- [ ] **Step 3: Verify it loads without syntax errors**

Run: `bash -n ~/.local/bin/create-pr-description`
Expected: No output (no syntax errors). The script has no main logic yet, so actually running it would fail — this only checks syntax.

- [ ] **Step 4: Commit**

```bash
git add ~/.local/bin/create-pr-description
git commit -m "feat: create script skeleton with constants and helper functions"
```

---

### Task 2: Implement CLI argument parsing (--init, --target, --help)

**Files:**

- Modify: `~/.local/bin/create-pr-description`

- [ ] **Step 1: Add argument parsing and --help**

Append to the script:

```bash
# ---- CLI Argument Parsing ----

ACTION=""
TARGETS=()

show_help() {
  cat <<'HELP'
create-pr-description [opcoes]

Gera descrição de PR automaticamente a partir do contexto git,
usando LLM (OpenRouter/Groq) com fallback configuravel.

Opcoes:
  --init               Inicializa arquivos de configuracao
  --target <branch>    Target do PR: dev, sprint (pode repetir; padrao: ambos)
  --help               Mostra esta ajuda
  --version            Mostra a versao

Exemplos:
  create-pr-description                      # PR para dev + sprint
  create-pr-description --target dev         # PR apenas para dev
  create-pr-description --target sprint      # PR apenas para sprint
HELP
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --init)
        ACTION="init"
        shift
        ;;
      --target)
        if [[ -z "${2:-}" ]]; then
          log_error "Flag --target requer um argumento (dev ou sprint)."
          exit 1
        fi
        if [[ "$2" != "dev" && "$2" != "sprint" ]]; then
          log_error "Valor invalido para --target: '$2'. Use 'dev' ou 'sprint'."
          exit 1
        fi
        TARGETS+=("$2")
        shift 2
        ;;
      --help)
        show_help
        exit 0
        ;;
      --version)
        echo "create-pr-description v$VERSION"
        exit 0
        ;;
      *)
        log_error "Opcao desconhecida: $1"
        show_help
        exit 1
        ;;
    esac
  done

  # Default: both targets
  if [[ ${#TARGETS[@]} -eq 0 ]]; then
    TARGETS=("dev" "sprint")
  fi

  # Remove duplicates
  TARGETS=($(echo "${TARGETS[@]}" | tr ' ' '\n' | sort -u | tr '\n' ' '))
}
```

- [ ] **Step 2: Add main entry point at the bottom of the script**

```bash
# ---- Main ----

main() {
  parse_args "$@"

  if [[ "$ACTION" == "init" ]]; then
    do_init
    exit 0
  fi

  # (remaining logic will be added in subsequent tasks)
  log_info "Targets: ${TARGETS[*]}"
}

main "$@"
```

- [ ] **Step 3: Test --help**

Run: `create-pr-description --help`
Expected: Shows usage help with options and examples.

- [ ] **Step 4: Test --target validation**

Run: `create-pr-description --target invalid`
Expected: Error message "Valor invalido para --target: 'invalid'..."

- [ ] **Step 5: Test --version**

Run: `create-pr-description --version`
Expected: "create-pr-description v1.0.0"

- [ ] **Step 6: Commit**

```bash
git add ~/.local/bin/create-pr-description
git commit -m "feat: add CLI argument parsing with --help, --target, --version"
```

---

### Task 3: Implement --init (create config files)

**Files:**

- Modify: `~/.local/bin/create-pr-description`
- Create (via script): `~/.config/pr-tools/pr-template.md`
- Create (via script): `~/.config/pr-tools/.env`
- Create (via script): `~/.config/pr-tools/.cache`

- [ ] **Step 1: Add the default template content as a heredoc**

Add before the `main()` function:

```bash
# ---- Default Template ----

DEFAULT_TEMPLATE='Analise o diff e log do git fornecidos e gere uma descrição de PR em portugues
brasileiro seguindo EXATAMENTE este formato:

---

## Descrição

<Resumo conciso em 1-2 frases do que a mudanca faz e por que>

## Alteracoes

### Componentes atualizados

<Para cada componente/arquivo modificado significativamente, liste:>
- **nome-do-componente**: <Descrição das mudancas neste componente, focando no
  que mudou funcionalmente, nao linha por linha>

### Correcoes / Melhorias tecnicas

<Se houver correcoes de bugs, refatoracoes ou melhorias tecnicas, liste aqui.
Se nao houver, omita esta secao.>

## Tipo de mudanca

<Marque com [x] os tipos que se aplicam, baseado na analise do diff:>

- [ ] Bug fix
- [ ] Nova feature
- [ ] Breaking change
- [ ] Refactoring

---

## Regras:
- Escreva em portugues brasileiro
- Seja tecnico mas conciso
- Foque no "o que" e "por que", nao no "como"
- Use nomes reais de componentes/arquivos do diff
- Se o diff for muito grande, agrupe mudancas relacionadas
- Nao invente mudancas que nao estao no diff'
```

- [ ] **Step 2: Add the default .env content as a heredoc**

```bash
DEFAULT_ENV='# Providers em ordem de prioridade (tenta o primeiro, se falhar vai pro proximo)
PR_PROVIDERS="openrouter,groq"

# API Keys
OPENROUTER_API_KEY=""
GROQ_API_KEY=""

# Modelos (opcional - usa padrao gratuito se nao definir)
# OPENROUTER_MODEL="google/gemini-2.5-flash:free"
# GROQ_MODEL="llama-3.3-70b-versatile"

# Azure DevOps (para gerar links de PR com repositoryId)
AZURE_PAT=""'
```

- [ ] **Step 3: Add the do_init function**

```bash
# ---- Init ----

confirm_overwrite() {
  local file="$1"
  if [[ -f "$file" ]]; then
    echo -n "Arquivo '$file' ja existe. Sobrescrever? [y/N] "
    read -r answer
    if [[ "$answer" != "y" && "$answer" != "Y" ]]; then
      log_info "Mantendo arquivo existente: $file"
      return 1
    fi
  fi
  return 0
}

do_init() {
  log_info "Inicializando configuracao em $CONFIG_DIR..."

  mkdir -p "$CONFIG_DIR"

  if confirm_overwrite "$TEMPLATE_FILE"; then
    echo "$DEFAULT_TEMPLATE" > "$TEMPLATE_FILE"
    log_success "Template criado: $TEMPLATE_FILE"
  fi

  if confirm_overwrite "$ENV_FILE"; then
    echo "$DEFAULT_ENV" > "$ENV_FILE"
    chmod 600 "$ENV_FILE"
    log_success "Arquivo .env criado: $ENV_FILE"
    log_warn "Edite $ENV_FILE e preencha suas API keys."
  fi

  if [[ ! -f "$CACHE_FILE" ]]; then
    touch "$CACHE_FILE"
    log_success "Cache criado: $CACHE_FILE"
  fi

  echo ""
  log_success "Inicializacao concluida!"
  echo "Proximo passo: edite $ENV_FILE com suas API keys."
}
```

- [ ] **Step 4: Test --init on clean system**

Run: `rm -rf ~/.config/pr-tools && create-pr-description --init`
Expected:

```
[INFO] Inicializando configuracao em /Users/.../.config/pr-tools...
[OK] Template criado: .../.config/pr-tools/pr-template.md
[OK] Arquivo .env criado: .../.config/pr-tools/.env
[OK] Cache criado: .../.config/pr-tools/.cache
[OK] Inicializacao concluida!
Proximo passo: edite .../.config/pr-tools/.env com suas API keys.
```

- [ ] **Step 5: Verify .env permissions**

Run: `stat -f "%Lp" ~/.config/pr-tools/.env` (macOS) or `stat -c "%a" ~/.config/pr-tools/.env` (Linux)
Expected: `600`

- [ ] **Step 6: Test --init with existing files (overwrite prompt)**

Run: `create-pr-description --init` (type 'n' when prompted)
Expected: "Mantendo arquivo existente" messages.

- [ ] **Step 7: Commit**

```bash
git add ~/.local/bin/create-pr-description
git commit -m "feat: implement --init to create config, template, and cache files"
```

---

### Task 4: Implement validation functions

**Files:**

- Modify: `~/.local/bin/create-pr-description`

- [ ] **Step 1: Add validation functions**

Add after the helper functions:

```bash
# ---- Validation ----

validate_git_repo() {
  if ! git rev-parse --is-inside-work-tree &>/dev/null; then
    log_error "Nao e um repositório git."
    exit 1
  fi
}

validate_dependencies() {
  local missing=()
  for cmd in curl jq git; do
    if ! command -v "$cmd" &>/dev/null; then
      missing+=("$cmd")
    fi
  done
  if [[ ${#missing[@]} -gt 0 ]]; then
    log_error "Dependencias nao encontradas: ${missing[*]}"
    exit 1
  fi
}

validate_not_base_branch() {
  local branch
  branch=$(git branch --show-current 2>/dev/null || echo "")
  if [[ "$branch" == "dev" || "$branch" == "main" || "$branch" == "master" ]]; then
    log_error "Voce esta na branch base ($branch). Mude para uma feature branch."
    exit 1
  fi
  if [[ -z "$branch" ]]; then
    log_error "Nao foi possivel determinar a branch atual (detached HEAD?)."
    exit 1
  fi
}

validate_config() {
  if [[ ! -f "$TEMPLATE_FILE" ]]; then
    log_error "Template nao encontrado em $TEMPLATE_FILE"
    log_error "Execute 'create-pr-description --init' para criar."
    exit 1
  fi
}

validate_api_keys() {
  local has_key=false
  if [[ -n "${OPENROUTER_API_KEY:-}" ]]; then has_key=true; fi
  if [[ -n "${GROQ_API_KEY:-}" ]]; then has_key=true; fi
  if [[ "$has_key" == "false" ]]; then
    log_error "Nenhuma API key configurada."
    log_error "Execute 'create-pr-description --init' e configure o .env."
    exit 1
  fi
}
```

- [ ] **Step 2: Add config loading function**

```bash
# ---- Config ----

load_config() {
  # Load .env file if it exists
  # Precedence: env var > .env > default
  # We source .env line by line, only setting vars that are not already set
  if [[ -f "$ENV_FILE" ]]; then
    while IFS='=' read -r key value; do
      # Skip comments and empty lines
      [[ "$key" =~ ^[[:space:]]*# ]] && continue
      [[ -z "$key" ]] && continue
      # Trim whitespace
      key=$(echo "$key" | xargs)
      # Remove surrounding quotes from value
      value=$(echo "$value" | sed 's/^["'\'']\(.*\)["'\''"]$/\1/')
      # Only set if not already defined in environment
      if [[ -z "${!key:-}" ]]; then
        export "$key=$value"
      fi
    done < "$ENV_FILE"
  fi

  # Apply defaults for unset vars
  PR_PROVIDERS="${PR_PROVIDERS:-$DEFAULT_PROVIDERS}"
  OPENROUTER_MODEL="${OPENROUTER_MODEL:-$DEFAULT_OPENROUTER_MODEL}"
  GROQ_MODEL="${GROQ_MODEL:-$DEFAULT_GROQ_MODEL}"
}
```

- [ ] **Step 3: Wire validations into main()**

Update the main function:

```bash
main() {
  parse_args "$@"

  if [[ "$ACTION" == "init" ]]; then
    do_init
    exit 0
  fi

  validate_dependencies
  validate_git_repo
  load_config
  validate_config
  validate_api_keys
  validate_not_base_branch

  log_info "Targets: ${TARGETS[*]}"
}
```

- [ ] **Step 4: Test validation - not a git repo**

Run: `cd /tmp && create-pr-description`
Expected: Error "Nao e um repositório git."

- [ ] **Step 5: Test validation - on base branch**

Run (from a git repo on dev): `git checkout dev && create-pr-description`
Expected: Error "Voce esta na branch base (dev)..."

- [ ] **Step 6: Commit**

```bash
git add ~/.local/bin/create-pr-description
git commit -m "feat: add validation and config loading functions"
```

---

## Chunk 2: Git context collection, Azure DevOps integration, and sprint detection

### Task 5: Collect git context (branch, diff, log)

**Files:**

- Modify: `~/.local/bin/create-pr-description`

- [ ] **Step 1: Add git context collection functions**

```bash
# ---- Git Context ----

collect_git_context() {
  BRANCH_NAME=$(git branch --show-current)
  log_info "Branch: $BRANCH_NAME"

  # Fetch remote (non-fatal on failure)
  if ! git fetch --prune origin 2>/dev/null; then
    log_warn "Falha ao fazer fetch do remote. Usando dados locais."
  fi

  # Collect diff with line limit
  local max_diff_lines=8000
  local raw_diff
  raw_diff=$(git diff dev...HEAD 2>/dev/null || git diff dev..HEAD 2>/dev/null || echo "")

  if [[ -z "$raw_diff" ]]; then
    log_error "Nenhuma alteracao encontrada em relacao a dev."
    exit 1
  fi

  local diff_lines
  diff_lines=$(echo "$raw_diff" | wc -l)

  if [[ "$diff_lines" -gt "$max_diff_lines" ]]; then
    GIT_DIFF=$(echo "$raw_diff" | head -n "$max_diff_lines")
    GIT_DIFF="$GIT_DIFF"$'\n\n[diff truncado, mostrando primeiras '"$max_diff_lines"' linhas de '"$diff_lines"' totais]'
    log_warn "Diff truncado: $diff_lines linhas -> $max_diff_lines linhas"
  else
    GIT_DIFF="$raw_diff"
  fi

  # Collect log (max 50 commits)
  GIT_LOG=$(git log dev...HEAD --oneline --max-count=50 2>/dev/null \
    || git log dev..HEAD --oneline --max-count=50 2>/dev/null \
    || echo "(log nao disponivel)")
}
```

- [ ] **Step 2: Wire into main()**

Add after `validate_not_base_branch`:

```bash
  collect_git_context
```

- [ ] **Step 3: Test from a feature branch with changes**

Run: `create-pr-description` (from a feature branch with commits ahead of dev)
Expected: Shows "Branch: <name>" and "Targets: dev sprint" without errors.

- [ ] **Step 4: Commit**

```bash
git add ~/.local/bin/create-pr-description
git commit -m "feat: add git context collection (diff, log, branch)"
```

---

### Task 6: Detect current sprint

**Files:**

- Modify: `~/.local/bin/create-pr-description`

- [ ] **Step 1: Add sprint detection function**

```bash
# ---- Sprint Detection ----

detect_sprint() {
  SPRINT_NUMBER=""
  SPRINT_BRANCH=""

  local sprint_num
  sprint_num=$(git branch -r 2>/dev/null \
    | grep 'origin/sprint/' \
    | sed 's|.*origin/sprint/||' \
    | sort -n \
    | tail -1 \
    | tr -d '[:space:]')

  if [[ -n "$sprint_num" ]]; then
    SPRINT_NUMBER="$sprint_num"
    SPRINT_BRANCH="sprint/$sprint_num"
    log_info "Sprint detectada: $SPRINT_BRANCH"
  else
    log_warn "Nenhuma branch sprint encontrada no remote."
    # Remove sprint from targets if it was requested
    local new_targets=()
    local sprint_was_explicit=false
    for t in "${TARGETS[@]}"; do
      if [[ "$t" == "sprint" ]]; then
        sprint_was_explicit=true
      else
        new_targets+=("$t")
      fi
    done

    if [[ "$sprint_was_explicit" == "true" && ${#new_targets[@]} -eq 0 ]]; then
      log_error "Flag --target sprint usada, mas nenhuma branch sprint/* encontrada."
      exit 1
    fi

    TARGETS=("${new_targets[@]}")
    if [[ ${#TARGETS[@]} -eq 0 ]]; then
      TARGETS=("dev")
    fi
    log_info "Usando apenas targets: ${TARGETS[*]}"
  fi
}
```

- [ ] **Step 2: Wire into main() after collect_git_context**

```bash
  detect_sprint
```

- [ ] **Step 3: Test sprint detection with sprint branches**

Run (from a repo that has `origin/sprint/*` branches): `create-pr-description`
Expected: Log shows "Sprint detectada: sprint/N" with the highest sprint number.

- [ ] **Step 4: Test sprint detection without sprint branches**

Run (from a repo without `origin/sprint/*` branches): `create-pr-description`
Expected: Warning "Nenhuma branch sprint encontrada no remote." and targets adjusted to just "dev".

- [ ] **Step 5: Test --target sprint when no sprint exists**

Run: `create-pr-description --target sprint` (from a repo without sprint branches)
Expected: Error "Flag --target sprint usada, mas nenhuma branch sprint/\* encontrada."

- [ ] **Step 6: Commit**

```bash
git add ~/.local/bin/create-pr-description
git commit -m "feat: add automatic sprint branch detection"
```

---

### Task 7: Parse Azure DevOps remote and get repositoryId

**Files:**

- Modify: `~/.local/bin/create-pr-description`

- [ ] **Step 1: Add remote URL parsing function**

```bash
# ---- Azure DevOps ----

parse_azure_remote() {
  AZURE_ORG=""
  AZURE_PROJECT=""
  AZURE_REPO=""
  IS_AZURE_DEVOPS=false

  local remote_url
  remote_url=$(git remote get-url origin 2>/dev/null || echo "")

  if [[ -z "$remote_url" ]]; then
    log_warn "Remote origin nao configurado. Links de PR nao serao gerados."
    return
  fi

  # HTTPS: https://dev.azure.com/{org}/{project}/_git/{repo}
  # HTTPS with user: https://{org}@dev.azure.com/{org}/{project}/_git/{repo}
  if [[ "$remote_url" =~ dev\.azure\.com[/:]([^/]+)/([^/]+)/_git/([^/]+) ]]; then
    local matched_org="${BASH_REMATCH[1]}"
    local matched_project="${BASH_REMATCH[2]}"
    AZURE_REPO="${BASH_REMATCH[3]}"

    # If URL has user@dev.azure.com, the first capture is the org in the path
    if [[ "$remote_url" =~ https://([^@]+)@dev\.azure\.com/ ]]; then
      # Format: https://{org}@dev.azure.com/{org}/{project}/_git/{repo}
      AZURE_ORG="$matched_org"
      AZURE_PROJECT="$matched_project"
    else
      # Format: https://dev.azure.com/{org}/{project}/_git/{repo}
      AZURE_ORG="$matched_org"
      AZURE_PROJECT="$matched_project"
    fi
    IS_AZURE_DEVOPS=true

  # SSH: git@ssh.dev.azure.com:v3/{org}/{project}/{repo}
  elif [[ "$remote_url" =~ ssh\.dev\.azure\.com:v3/([^/]+)/([^/]+)/([^/]+) ]]; then
    AZURE_ORG="${BASH_REMATCH[1]}"
    AZURE_PROJECT="${BASH_REMATCH[2]}"
    AZURE_REPO="${BASH_REMATCH[3]}"
    IS_AZURE_DEVOPS=true
  fi

  # Clean trailing .git if present
  AZURE_REPO="${AZURE_REPO%.git}"

  if [[ "$IS_AZURE_DEVOPS" == "true" ]]; then
    log_info "Azure DevOps: $AZURE_ORG/$AZURE_PROJECT/$AZURE_REPO"
  else
    log_warn "Remote nao e Azure DevOps. Links de PR nao serao gerados."
  fi
}
```

- [ ] **Step 2: Add repositoryId fetch and cache functions**

```bash
get_cached_repo_id() {
  local remote_url="$1"
  if [[ -f "$CACHE_FILE" ]]; then
    grep "^${remote_url}=" "$CACHE_FILE" 2>/dev/null | cut -d'=' -f2 || echo ""
  else
    echo ""
  fi
}

cache_repo_id() {
  local remote_url="$1"
  local repo_id="$2"
  # Remove old entry if exists, then add new
  if [[ -f "$CACHE_FILE" ]]; then
    grep -v "^${remote_url}=" "$CACHE_FILE" > "${CACHE_FILE}.tmp" 2>/dev/null || true
    mv "${CACHE_FILE}.tmp" "$CACHE_FILE"
  fi
  echo "${remote_url}=${repo_id}" >> "$CACHE_FILE"
}

fetch_repo_id() {
  AZURE_REPO_ID=""

  if [[ "$IS_AZURE_DEVOPS" != "true" ]]; then
    return
  fi

  local remote_url
  remote_url="https://dev.azure.com/$AZURE_ORG/$AZURE_PROJECT/_git/$AZURE_REPO"

  # Try cache first
  AZURE_REPO_ID=$(get_cached_repo_id "$remote_url")
  if [[ -n "$AZURE_REPO_ID" ]]; then
    log_info "repositoryId (cache): ${AZURE_REPO_ID:0:8}..."
    return
  fi

  # Fetch from API if PAT is available
  if [[ -z "${AZURE_PAT:-}" ]]; then
    log_warn "AZURE_PAT nao configurado. Links gerados sem repositoryId."
    return
  fi

  log_info "Buscando repositoryId via API..."
  local api_response
  api_response=$(curl -s --max-time 10 \
    -u ":$AZURE_PAT" \
    "https://dev.azure.com/$AZURE_ORG/$AZURE_PROJECT/_apis/git/repositories/$AZURE_REPO?api-version=7.0" \
    2>/dev/null || echo "")

  if [[ -z "$api_response" ]]; then
    log_warn "Falha ao obter repositoryId. Links gerados sem repositoryId."
    return
  fi

  local repo_id
  repo_id=$(echo "$api_response" | jq -r '.id // empty' 2>/dev/null || echo "")

  if [[ -n "$repo_id" && "$repo_id" != "null" ]]; then
    AZURE_REPO_ID="$repo_id"
    cache_repo_id "$remote_url" "$repo_id"
    log_info "repositoryId obtido e cacheado: ${repo_id:0:8}..."
  else
    log_warn "Falha ao obter repositoryId. Links gerados sem repositoryId."
  fi
}
```

- [ ] **Step 3: Add PR link builder function**

```bash
build_pr_links() {
  PR_LINKS=()

  if [[ "$IS_AZURE_DEVOPS" != "true" ]]; then
    return
  fi

  local base_url="https://dev.azure.com/$AZURE_ORG/$AZURE_PROJECT/_git/$AZURE_REPO/pullrequestcreate"
  local source_ref="$BRANCH_NAME"

  for target in "${TARGETS[@]}"; do
    local target_ref=""
    if [[ "$target" == "dev" ]]; then
      target_ref="dev"
    elif [[ "$target" == "sprint" && -n "$SPRINT_BRANCH" ]]; then
      target_ref="$SPRINT_BRANCH"
    else
      continue
    fi

    local link="${base_url}?sourceRef=${source_ref}&targetRef=${target_ref}"
    if [[ -n "${AZURE_REPO_ID:-}" ]]; then
      link="${link}&sourceRepositoryId=${AZURE_REPO_ID}&targetRepositoryId=${AZURE_REPO_ID}"
    fi

    PR_LINKS+=("$target_ref|$link")
  done
}
```

- [ ] **Step 4: Wire into main()**

Add after `detect_sprint`:

```bash
  parse_azure_remote
  fetch_repo_id
  build_pr_links
```

- [ ] **Step 5: Test with Azure DevOps HTTPS remote**

Run (from a repo with Azure DevOps HTTPS remote): `create-pr-description`
Expected: Log shows "Azure DevOps: {org}/{project}/{repo}" and PR links are generated.

- [ ] **Step 6: Test with non-Azure remote (e.g. GitHub)**

Run (from a repo with GitHub remote): `create-pr-description`
Expected: Warning "Remote nao e Azure DevOps. Links de PR nao serao gerados."

- [ ] **Step 7: Test repositoryId caching**

Run `create-pr-description` twice on the same repo (with AZURE_PAT configured).
Expected: First run shows "Buscando repositoryId via API...", second run shows "repositoryId (cache):..."
Verify: `cat ~/.config/pr-tools/.cache` shows a line with the remote URL and repo ID.

- [ ] **Step 8: Test without AZURE_PAT**

Temporarily clear AZURE_PAT in .env and run: `create-pr-description`
Expected: Warning "AZURE_PAT nao configurado. Links gerados sem repositoryId." and links without repositoryId params.

- [ ] **Step 9: Commit**

```bash
git add ~/.local/bin/create-pr-description
git commit -m "feat: add Azure DevOps remote parsing, repo ID fetch/cache, PR link builder"
```

---

## Chunk 3: LLM API call with provider fallback, output formatting, clipboard

### Task 8: Implement LLM API call with provider fallback

**Files:**

- Modify: `~/.local/bin/create-pr-description`

- [ ] **Step 1: Add provider-specific config resolver**

```bash
# ---- LLM Providers ----

get_provider_config() {
  local provider="$1"
  case "$provider" in
    openrouter)
      PROVIDER_URL="https://openrouter.ai/api/v1/chat/completions"
      PROVIDER_KEY="${OPENROUTER_API_KEY:-}"
      PROVIDER_MODEL="${OPENROUTER_MODEL}"
      PROVIDER_EXTRA_HEADERS='-H "HTTP-Referer: https://github.com/create-pr-description" -H "X-Title: create-pr-description"'
      ;;
    groq)
      PROVIDER_URL="https://api.groq.com/openai/v1/chat/completions"
      PROVIDER_KEY="${GROQ_API_KEY:-}"
      PROVIDER_MODEL="${GROQ_MODEL}"
      PROVIDER_EXTRA_HEADERS=""
      ;;
    *)
      log_warn "Provider desconhecido: $provider"
      PROVIDER_KEY=""
      return
      ;;
  esac
}
```

- [ ] **Step 2: Add the API call function**

```bash
call_llm_api() {
  local url="$1"
  local key="$2"
  local model="$3"
  local system_prompt="$4"
  local user_prompt="$5"
  local extra_headers="$6"

  # Build JSON payload using jq to properly escape content
  local payload
  payload=$(jq -n \
    --arg model "$model" \
    --arg system "$system_prompt" \
    --arg user "$user_prompt" \
    '{
      model: $model,
      messages: [
        { role: "system", content: $system },
        { role: "user", content: $user }
      ],
      temperature: 0.3
    }')

  # Build curl command
  local curl_args=(
    -s
    -w "\n%{http_code}"
    --max-time 60
    -H "Content-Type: application/json"
    -H "Authorization: Bearer $key"
  )

  # Add extra headers if present
  if [[ -n "$extra_headers" ]]; then
    if [[ "$extra_headers" == *"HTTP-Referer"* ]]; then
      curl_args+=(-H "HTTP-Referer: https://github.com/create-pr-description")
      curl_args+=(-H "X-Title: create-pr-description")
    fi
  fi

  curl_args+=(-d "$payload" "$url")

  local response
  response=$(curl "${curl_args[@]}" 2>/dev/null || echo -e "\n000")

  local http_code
  http_code=$(echo "$response" | tail -1)
  local body
  body=$(echo "$response" | sed '$d')

  if [[ "$http_code" == "000" ]]; then
    log_warn "Timeout no provider (sem resposta em 60s)"
    echo ""
    return 1
  elif [[ "$http_code" == "429" ]]; then
    log_warn "Rate limit (HTTP 429) de $url"
    echo ""
    return 1
  elif [[ "$http_code" != "200" ]]; then
    log_warn "HTTP $http_code de $url"
    echo ""
    return 1
  fi

  # Extract content
  local content
  content=$(echo "$body" | jq -r '.choices[0].message.content // empty' 2>/dev/null || echo "")

  if [[ -z "$content" || "$content" == "null" ]]; then
    log_warn "Resposta vazia ou invalida de $url"
    echo ""
    return 1
  fi

  echo "$content"
  return 0
}
```

- [ ] **Step 3: Add the provider iteration function**

```bash
call_with_fallback() {
  local system_prompt="$1"
  local user_prompt="$2"

  IFS=',' read -ra providers <<< "$PR_PROVIDERS"

  for provider in "${providers[@]}"; do
    provider=$(echo "$provider" | tr -d '[:space:]')
    get_provider_config "$provider"

    if [[ -z "$PROVIDER_KEY" ]]; then
      log_warn "API key nao configurada para $provider. Pulando..."
      continue
    fi

    log_info "Tentando provider: $provider ($PROVIDER_MODEL)..."
    local result
    if result=$(call_llm_api "$PROVIDER_URL" "$PROVIDER_KEY" "$PROVIDER_MODEL" "$system_prompt" "$user_prompt" "$PROVIDER_EXTRA_HEADERS"); then
      if [[ -n "$result" ]]; then
        USED_PROVIDER="$provider"
        USED_MODEL="$PROVIDER_MODEL"
        LLM_RESULT="$result"
        return 0
      fi
    fi
    log_warn "Provider $provider falhou. Tentando proximo..."
  done

  log_error "Todos os providers falharam. Verifique suas API keys e conexao."
  exit 1
}
```

- [ ] **Step 4: Commit**

```bash
git add ~/.local/bin/create-pr-description
git commit -m "feat: add LLM API call with multi-provider fallback"
```

---

### Task 9: Implement output formatting and clipboard

**Files:**

- Modify: `~/.local/bin/create-pr-description`

- [ ] **Step 1: Add clipboard detection function**

```bash
# ---- Clipboard ----

detect_clipboard() {
  if command -v pbcopy &>/dev/null; then
    CLIP_CMD="pbcopy"
  elif command -v xclip &>/dev/null; then
    CLIP_CMD="xclip -selection clipboard"
  elif command -v xsel &>/dev/null; then
    CLIP_CMD="xsel --clipboard --input"
  else
    CLIP_CMD=""
  fi
}

copy_to_clipboard() {
  local text="$1"
  if [[ -n "$CLIP_CMD" ]]; then
    echo "$text" | eval "$CLIP_CMD"
    return 0
  fi
  return 1
}
```

- [ ] **Step 2: Add output formatting function**

```bash
# ---- Output ----

print_output() {
  local description="$1"
  local separator="=========================================="

  echo ""
  echo -e "${BOLD}${separator}${NC}"
  echo -e "${BOLD}PR Description - ${CYAN}${BRANCH_NAME}${NC}"

  # Show target branches
  local target_display=""
  for target in "${TARGETS[@]}"; do
    if [[ "$target" == "dev" ]]; then
      target_display="${target_display}dev, "
    elif [[ "$target" == "sprint" && -n "$SPRINT_BRANCH" ]]; then
      target_display="${target_display}${SPRINT_BRANCH}, "
    fi
  done
  target_display="${target_display%, }"
  echo -e "${BOLD}Target branches: ${target_display}${NC}"
  echo -e "${BOLD}Provider: ${USED_PROVIDER} (${USED_MODEL})${NC}"
  echo -e "${BOLD}${separator}${NC}"
  echo ""
  echo "$description"
  echo ""

  # PR Links
  if [[ ${#PR_LINKS[@]} -gt 0 ]]; then
    echo -e "${BOLD}${separator}${NC}"
    echo -e "${BOLD}Abrir PR:${NC}"
    echo ""
    for link_entry in "${PR_LINKS[@]}"; do
      local target_ref="${link_entry%%|*}"
      local url="${link_entry#*|}"
      echo -e "  -> ${CYAN}${target_ref}${NC}:"
      echo -e "     ${url}"
      echo ""
    done
  fi

  # Clipboard
  detect_clipboard
  if copy_to_clipboard "$description"; then
    echo -e "${BOLD}Descrição copiada para o clipboard!${NC}"
  else
    log_warn "Nenhum comando de clipboard encontrado (pbcopy/xclip/xsel). Descrição exibida apenas no terminal."
  fi
  echo -e "${BOLD}${separator}${NC}"
}
```

- [ ] **Step 3: Test clipboard detection**

Run on macOS: Verify `pbcopy` is detected.
Run on Linux with xclip: Verify `xclip -selection clipboard` is detected.
Verify: After running the full script, `pbpaste` (macOS) or `xclip -o` (Linux) shows the PR description.

- [ ] **Step 4: Commit**

```bash
git add ~/.local/bin/create-pr-description
git commit -m "feat: add output formatting with PR links and clipboard support"
```

---

### Task 10: Wire everything together in main()

**Files:**

- Modify: `~/.local/bin/create-pr-description`

- [ ] **Step 1: Update main() with the complete flow**

Replace the `main()` function:

```bash
main() {
  parse_args "$@"

  if [[ "$ACTION" == "init" ]]; then
    do_init
    exit 0
  fi

  # Validation
  validate_dependencies
  validate_git_repo
  load_config
  validate_config
  validate_api_keys
  validate_not_base_branch

  # Collect context
  collect_git_context
  detect_sprint
  parse_azure_remote
  fetch_repo_id
  build_pr_links

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

  local user_prompt
  user_prompt="## Contexto Git

**Branch:** $BRANCH_NAME
**Base branches alvo:** $target_display

### Git Log (commits desde a base):
$GIT_LOG

### Git Diff:
$GIT_DIFF"

  # Call LLM
  log_info "Gerando descrição do PR..."
  call_with_fallback "$template_content" "$user_prompt"

  # Output
  print_output "$LLM_RESULT"
}

main "$@"
```

- [ ] **Step 2: End-to-end test**

Run (from a feature branch with changes, after configuring .env with a valid API key):

```bash
create-pr-description
```

Expected: Full output with description, PR links, clipboard copy confirmation.

- [ ] **Step 3: Test with --target dev only**

Run: `create-pr-description --target dev`
Expected: Only shows dev link, no sprint link.

- [ ] **Step 4: Test with --target sprint only**

Run: `create-pr-description --target sprint`
Expected: Only shows sprint link (or error if no sprint branch found).

- [ ] **Step 5: Final commit**

```bash
git add ~/.local/bin/create-pr-description
git commit -m "feat: wire complete flow - git context, LLM call, output with PR links"
```

---

## Chunk 4: Testing and polish

### Task 11: Manual test matrix

Run these tests to verify all error paths and edge cases work:

- [ ] **Step 1: Test outside git repo**

Run: `cd /tmp && create-pr-description`
Expected: Error "Nao e um repositório git."

- [ ] **Step 2: Test on base branch**

Run: `git checkout dev && create-pr-description`
Expected: Error "Voce esta na branch base (dev)."

- [ ] **Step 3: Test with no API keys**

Temporarily clear API keys in .env and run: `create-pr-description`
Expected: Error "Nenhuma API key configurada."

- [ ] **Step 4: Test with missing template**

Run: `mv ~/.config/pr-tools/pr-template.md /tmp/ && create-pr-description`
Expected: Error about missing template.
Cleanup: `mv /tmp/pr-template.md ~/.config/pr-tools/`

- [ ] **Step 5: Test --init overwrite flow**

Run: `create-pr-description --init` (answer 'n' to all, then again answer 'y' to all)
Expected: Correct behavior both times.

- [ ] **Step 6: Test provider fallback**

Set an invalid key for the first provider in .env, valid key for second.
Run: `create-pr-description`
Expected: Warning about first provider failing, success with second.

- [ ] **Step 7: Test --help and --version**

Run: `create-pr-description --help && create-pr-description --version`
Expected: Help text and version string.

- [ ] **Step 8: Test detached HEAD**

Run: `git checkout --detach && create-pr-description`
Expected: Error "Nao foi possivel determinar a branch atual (detached HEAD?)."
Cleanup: `git checkout -` to return to previous branch.

- [ ] **Step 9: Test multiple --target flags**

Run: `create-pr-description --target dev --target sprint`
Expected: Both dev and sprint links generated (same as default behavior).

- [ ] **Step 10: Test env var precedence over .env**

Set a different model in env var vs .env:
Run: `OPENROUTER_MODEL="different/model" create-pr-description`
Expected: Output shows "Provider: openrouter (different/model)" confirming env var took precedence.

- [ ] **Step 11: Test with non-Azure DevOps remote**

From a repo with a GitHub remote:
Run: `create-pr-description`
Expected: Warning "Remote nao e Azure DevOps. Links de PR nao serao gerados." and no PR links section in output.

- [ ] **Step 12: Test --target sprint without sprint branches**

From a repo without sprint branches:
Run: `create-pr-description --target sprint`
Expected: Error "Flag --target sprint usada, mas nenhuma branch sprint/\* encontrada."
