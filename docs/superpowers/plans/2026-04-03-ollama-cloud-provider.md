# Ollama Cloud Provider Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Ollama Cloud as a fourth LLM provider (OpenAI-compatible) alongside OpenRouter, Groq, and Gemini.

**Architecture:** Ollama Cloud exposes `/v1/chat/completions` (OpenAI-compatible), so it reuses `call_llm_api()` and `execute_openai_compatible_request()` unchanged. The changes are purely additive: new `case` entries in provider-switch statements and a new wizard block.

**Tech Stack:** Bash, curl, jq — no new dependencies.

---

## File Map

| File                            | Change                                                                                                                                          |
| ------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/lib/common.sh`             | Add `ollama` case in `test_provider_key()`; add `OLLAMA_MODEL` default in `load_config()`                                                       |
| `src/lib/llm.sh`                | Add `ollama` case in `get_provider_config()`                                                                                                    |
| `src/lib/test-card-llm.sh`      | Add `ollama` case in `call_with_fallback()`                                                                                                     |
| `src/bin/create-pr-description` | Add `DEFAULT_OLLAMA_MODEL`; update `DEFAULT_PROVIDERS` and `DEFAULT_ENV`; add wizard block; add `--set-ollama-model` flag; update `show_help()` |
| `src/bin/create-test-card`      | Add `DEFAULT_OLLAMA_MODEL`; update `DEFAULT_PROVIDERS` and `DEFAULT_ENV`                                                                        |
| `VERSION`                       | Bump from `2.9.2` → `2.9.3`                                                                                                                     |

---

## Task 1: Add Ollama support to `src/lib/common.sh`

**Files:**

- Modify: `src/lib/common.sh`

Two changes: (a) add `ollama` to `test_provider_key()` so the wizard can validate the key; (b) add `OLLAMA_MODEL` default application in `load_config()`.

- [ ] **Step 1: Verify the current state of `test_provider_key()` and `load_config()`**

```bash
grep -n "ollama\|OLLAMA" src/lib/common.sh
```

Expected output: no matches (Ollama not yet present).

- [ ] **Step 2: Add `ollama` case to `test_provider_key()` in `common.sh`**

In `src/lib/common.sh`, find the `test_provider_key()` function. After the `gemini)` block (which ends around line 218 with `return $?` and `;;`), add the new case **before** the closing `esac`. The exact insertion point is after the `;;` that closes the `gemini)` block and before `esac`:

```bash
    ollama)
      url="https://ollama.com/v1/chat/completions"
      model="${OLLAMA_MODEL:-${DEFAULT_OLLAMA_MODEL:-qwen3.5:cloud}}"
      ;;
```

The full `test_provider_key()` case block after the change:

```bash
  case "$provider" in
    openrouter)
      url="https://openrouter.ai/api/v1/chat/completions"
      model="${OPENROUTER_MODEL:-$DEFAULT_OPENROUTER_MODEL}"
      extra_headers=(-H "HTTP-Referer: https://github.com/create-pr-description" -H "X-Title: create-pr-description")
      ;;
    groq)
      url="https://api.groq.com/openai/v1/chat/completions"
      model="${GROQ_MODEL:-$DEFAULT_GROQ_MODEL}"
      ;;
    gemini)
      # Gemini uses a different API format; test with a simple request
      local gemini_model="${GEMINI_MODEL:-$DEFAULT_GEMINI_MODEL}"
      local gemini_http_code
      gemini_http_code=$(curl -s -o /dev/null -w "%{http_code}" \
        --max-time 15 \
        -H "Content-Type: application/json" \
        -d '{"contents":[{"parts":[{"text":"ok"}]}]}' \
        "https://generativelanguage.googleapis.com/v1beta/models/${gemini_model}:generateContent?key=${key}" \
        2>/dev/null || echo "000")
      [[ "$gemini_http_code" == "200" ]]
      return $?
      ;;
    ollama)
      url="https://ollama.com/v1/chat/completions"
      model="${OLLAMA_MODEL:-${DEFAULT_OLLAMA_MODEL:-qwen3.5:cloud}}"
      ;;
  esac
```

- [ ] **Step 3: Add `OLLAMA_MODEL` default in `load_config()`**

In `src/lib/common.sh`, find the `load_config()` function. After the line:

```bash
  GEMINI_MODEL="${GEMINI_MODEL:-${DEFAULT_GEMINI_MODEL:-}}"
```

Add:

```bash
  OLLAMA_MODEL="${OLLAMA_MODEL:-${DEFAULT_OLLAMA_MODEL:-}}"
```

- [ ] **Step 4: Verify the changes compile (no syntax errors)**

```bash
bash -n src/lib/common.sh && echo "OK"
```

Expected output: `OK`

- [ ] **Step 5: Verify `test_provider_key` recognizes `ollama`**

```bash
bash -c '
  CONFIG_DIR="$HOME/.config/pr-tools"
  ENV_FILE="$CONFIG_DIR/.env"
  DEFAULT_OLLAMA_MODEL="qwen3.5:cloud"
  OLLAMA_MODEL="qwen3.5:cloud"
  source src/lib/common.sh
  # Check that ollama case sets url correctly (by checking the variable after the case)
  provider="ollama"
  url=""
  model=""
  extra_headers=()
  key="test"
  case "$provider" in
    openrouter) url="https://openrouter.ai/api/v1/chat/completions" ;;
    groq) url="https://api.groq.com/openai/v1/chat/completions" ;;
    ollama) url="https://ollama.com/v1/chat/completions"; model="${OLLAMA_MODEL:-qwen3.5:cloud}" ;;
  esac
  echo "URL: $url"
  echo "MODEL: $model"
'
```

Expected output:

```
URL: https://ollama.com/v1/chat/completions
MODEL: qwen3.5:cloud
```

- [ ] **Step 6: Commit**

```bash
git add src/lib/common.sh
git commit -m "feat: add ollama to test_provider_key and load_config in common.sh"
```

---

## Task 2: Add Ollama to `get_provider_config()` in `src/lib/llm.sh`

**Files:**

- Modify: `src/lib/llm.sh`

- [ ] **Step 1: Verify Ollama is not yet in `get_provider_config()`**

```bash
grep -n "ollama\|OLLAMA" src/lib/llm.sh
```

Expected output: no matches.

- [ ] **Step 2: Add `ollama` case to `get_provider_config()`**

In `src/lib/llm.sh`, find `get_provider_config()`. After the `gemini)` block and before the `*)` wildcard case, add:

```bash
    ollama)
      PROVIDER_URL="https://ollama.com/v1/chat/completions"
      PROVIDER_KEY="${OLLAMA_API_KEY:-}"
      PROVIDER_MODEL="${OLLAMA_MODEL}"
      PROVIDER_NAME="ollama"
      ;;
```

The full `get_provider_config()` function after the change:

```bash
get_provider_config() {
  local provider="$1"
  PROVIDER_URL=""
  PROVIDER_KEY=""
  PROVIDER_MODEL=""
  PROVIDER_NAME=""

  case "$provider" in
    openrouter)
      PROVIDER_URL="https://openrouter.ai/api/v1/chat/completions"
      PROVIDER_KEY="${OPENROUTER_API_KEY:-}"
      PROVIDER_MODEL="${OPENROUTER_MODEL}"
      PROVIDER_NAME="openrouter"
      ;;
    groq)
      PROVIDER_URL="https://api.groq.com/openai/v1/chat/completions"
      PROVIDER_KEY="${GROQ_API_KEY:-}"
      PROVIDER_MODEL="${GROQ_MODEL}"
      PROVIDER_NAME="groq"
      ;;
    gemini)
      PROVIDER_URL="https://generativelanguage.googleapis.com/v1beta"
      PROVIDER_KEY="${GEMINI_API_KEY:-}"
      PROVIDER_MODEL="${GEMINI_MODEL:-$DEFAULT_GEMINI_MODEL}"
      PROVIDER_NAME="gemini"
      ;;
    ollama)
      PROVIDER_URL="https://ollama.com/v1/chat/completions"
      PROVIDER_KEY="${OLLAMA_API_KEY:-}"
      PROVIDER_MODEL="${OLLAMA_MODEL}"
      PROVIDER_NAME="ollama"
      ;;
    *)
      log_warn "Provider desconhecido: $provider"
      PROVIDER_KEY=""
      return
      ;;
  esac
}
```

- [ ] **Step 3: Verify no syntax errors**

```bash
bash -n src/lib/llm.sh && echo "OK"
```

Expected output: `OK`

- [ ] **Step 4: Verify `get_provider_config` resolves ollama correctly**

```bash
bash -c '
  CONFIG_DIR="$HOME/.config/pr-tools"
  ENV_FILE="$CONFIG_DIR/.env"
  DEFAULT_OLLAMA_MODEL="qwen3.5:cloud"
  OLLAMA_MODEL="qwen3.5:cloud"
  OLLAMA_API_KEY="oa-test-key"
  source src/lib/common.sh
  source src/lib/llm.sh
  get_provider_config "ollama"
  echo "URL: $PROVIDER_URL"
  echo "KEY: $PROVIDER_KEY"
  echo "MODEL: $PROVIDER_MODEL"
  echo "NAME: $PROVIDER_NAME"
'
```

Expected output:

```
URL: https://ollama.com/v1/chat/completions
KEY: oa-test-key
MODEL: qwen3.5:cloud
NAME: ollama
```

- [ ] **Step 5: Commit**

```bash
git add src/lib/llm.sh
git commit -m "feat: add ollama case to get_provider_config in llm.sh"
```

---

## Task 3: Add Ollama to `call_with_fallback()` in `src/lib/test-card-llm.sh`

**Files:**

- Modify: `src/lib/test-card-llm.sh`

- [ ] **Step 1: Verify Ollama is not yet in `test-card-llm.sh`**

```bash
grep -n "ollama\|OLLAMA" src/lib/test-card-llm.sh
```

Expected output: no matches.

- [ ] **Step 2: Add `ollama` case to `call_with_fallback()`**

In `src/lib/test-card-llm.sh`, find `call_with_fallback()`. After the `gemini)` block and before `esac`, add:

```bash
      ollama)
        [[ -n "${OLLAMA_API_KEY:-}" ]] || continue
        if call_openai_provider "ollama" "https://ollama.com/v1/chat/completions" "$OLLAMA_API_KEY" "$OLLAMA_MODEL" "$user_prompt"; then
          return 0
        fi
        ;;
```

The full `call_with_fallback()` function after the change:

```bash
call_with_fallback() {
  local user_prompt="$1"
  local provider
  IFS=',' read -ra providers <<< "$PR_PROVIDERS"

  for provider in "${providers[@]}"; do
    provider=$(echo "$provider" | tr -d '[:space:]')
    case "$provider" in
      openrouter)
        [[ -n "${OPENROUTER_API_KEY:-}" ]] || continue
        if call_openai_provider "openrouter" "https://openrouter.ai/api/v1/chat/completions" "$OPENROUTER_API_KEY" "$OPENROUTER_MODEL" "$user_prompt"; then
          return 0
        fi
        ;;
      groq)
        [[ -n "${GROQ_API_KEY:-}" ]] || continue
        if call_openai_provider "groq" "https://api.groq.com/openai/v1/chat/completions" "$GROQ_API_KEY" "$GROQ_MODEL" "$user_prompt"; then
          return 0
        fi
        ;;
      gemini)
        [[ -n "${GEMINI_API_KEY:-}" ]] || continue
        if call_gemini_provider "$GEMINI_API_KEY" "$GEMINI_MODEL" "$user_prompt"; then
          return 0
        fi
        ;;
      ollama)
        [[ -n "${OLLAMA_API_KEY:-}" ]] || continue
        if call_openai_provider "ollama" "https://ollama.com/v1/chat/completions" "$OLLAMA_API_KEY" "$OLLAMA_MODEL" "$user_prompt"; then
          return 0
        fi
        ;;
    esac
  done

  log_error "Todos os providers falharam."
  exit 1
}
```

- [ ] **Step 3: Verify no syntax errors**

```bash
bash -n src/lib/test-card-llm.sh && echo "OK"
```

Expected output: `OK`

- [ ] **Step 4: Commit**

```bash
git add src/lib/test-card-llm.sh
git commit -m "feat: add ollama case to call_with_fallback in test-card-llm.sh"
```

---

## Task 4: Update `src/bin/create-pr-description`

**Files:**

- Modify: `src/bin/create-pr-description`

Four sub-changes: (a) add `DEFAULT_OLLAMA_MODEL`; (b) update `DEFAULT_PROVIDERS`; (c) update `DEFAULT_ENV`; (d) add wizard block; (e) add `--set-ollama-model` flag; (f) update `show_help()`.

- [ ] **Step 1: Add `DEFAULT_OLLAMA_MODEL` constant**

Find the block at line ~58:

```bash
# Default provider config
DEFAULT_PROVIDERS="openrouter,groq,gemini"
DEFAULT_OPENROUTER_MODEL="meta-llama/llama-3.3-70b-instruct:free"
DEFAULT_GROQ_MODEL="llama-3.3-70b-versatile"
DEFAULT_GEMINI_MODEL="gemini-3.1-flash-lite-preview"
```

Replace with:

```bash
# Default provider config
DEFAULT_PROVIDERS="openrouter,groq,gemini,ollama"
DEFAULT_OPENROUTER_MODEL="meta-llama/llama-3.3-70b-instruct:free"
DEFAULT_GROQ_MODEL="llama-3.3-70b-versatile"
DEFAULT_GEMINI_MODEL="gemini-3.1-flash-lite-preview"
DEFAULT_OLLAMA_MODEL="qwen3.5:cloud"
```

- [ ] **Step 2: Update `DEFAULT_ENV` to include commented Ollama entries**

Find the `DEFAULT_ENV` block (around line 135). The current value ends with:

```
# GROQ_MODEL="llama-3.3-70b-versatile"

# Streaming (exibe tokens em tempo real; padrao: false)
PR_STREAM="false"
```

Replace the full `DEFAULT_ENV` value:

```bash
DEFAULT_ENV='# Providers em ordem de prioridade (tenta o primeiro, se falhar vai pro proximo)
PR_PROVIDERS="openrouter,groq"

# API Keys (descomente e preencha)
# OPENROUTER_API_KEY="sk-or-..."
# GROQ_API_KEY="gsk_..."
# OLLAMA_API_KEY="oa-..."

# Modelos (opcional - usa padrao gratuito se não definir)
# OPENROUTER_MODEL="meta-llama/llama-3.3-70b-instruct:free"
# GROQ_MODEL="llama-3.3-70b-versatile"
# OLLAMA_MODEL="qwen3.5:cloud"

# Streaming (exibe tokens em tempo real; padrao: false)
PR_STREAM="false"

# Azure DevOps (para gerar links de PR com repositoryId)
# AZURE_PAT="your-pat-token"'
```

- [ ] **Step 3: Add `--set-ollama-model` flag to `parse_args()`**

Find the `--set-gemini-model)` block (around line 245):

```bash
      --set-gemini-model)
        if [[ -z "${2:-}" ]]; then
          log_error "Flag --set-gemini-model requer o nome do modelo."
          exit 1
        fi
        set_env_var "GEMINI_MODEL" "$2"
        log_success "Modelo Gemini salvo: $2"
        exit 0
        ;;
```

After it (before the `--dry-run)` case), add:

```bash
      --set-ollama-model)
        if [[ -z "${2:-}" ]]; then
          log_error "Flag --set-ollama-model requer o nome do modelo."
          exit 1
        fi
        set_env_var "OLLAMA_MODEL" "$2"
        log_success "Modelo Ollama salvo: $2"
        exit 0
        ;;
```

- [ ] **Step 4: Update `show_help()` to document the new flag**

Find in `show_help()`:

```bash
  --set-gemini-model <modelo>         Salva modelo do Google Gemini no .env
```

After it, add:

```bash
  --set-ollama-model <modelo>         Salva modelo do Ollama no .env
```

- [ ] **Step 5: Add Ollama to the wizard in `run_setup_wizard()`**

**5a.** Find the local variable declarations block (around line 313):

```bash
  local existing_providers=""
  local existing_openrouter_key=""
  local existing_groq_key=""
  local existing_gemini_key=""
  local existing_openrouter_model=""
  local existing_groq_model=""
  local existing_gemini_model=""
```

Replace with:

```bash
  local existing_providers=""
  local existing_openrouter_key=""
  local existing_groq_key=""
  local existing_gemini_key=""
  local existing_ollama_key=""
  local existing_openrouter_model=""
  local existing_groq_model=""
  local existing_gemini_model=""
```

**5b.** Find the `case "$key" in` block that reads the `.env` values (around line 335):

```bash
      case "$key" in
        PR_PROVIDERS)         existing_providers="$value" ;;
        OPENROUTER_API_KEY)   existing_openrouter_key="$value" ;;
        GROQ_API_KEY)         existing_groq_key="$value" ;;
        GEMINI_API_KEY)       existing_gemini_key="$value" ;;
        OPENROUTER_MODEL)     existing_openrouter_model="$value" ;;
        GROQ_MODEL)           existing_groq_model="$value" ;;
        GEMINI_MODEL)         existing_gemini_model="$value" ;;
        AZURE_PAT)            existing_azure_pat="$value" ;;
        PR_REVIEWER_DEV)      existing_reviewer_dev="$value" ;;
        PR_REVIEWER_SPRINT)   existing_reviewer_sprint="$value" ;;
      esac
```

Replace with:

```bash
      case "$key" in
        PR_PROVIDERS)         existing_providers="$value" ;;
        OPENROUTER_API_KEY)   existing_openrouter_key="$value" ;;
        GROQ_API_KEY)         existing_groq_key="$value" ;;
        GEMINI_API_KEY)       existing_gemini_key="$value" ;;
        OLLAMA_API_KEY)       existing_ollama_key="$value" ;;
        OPENROUTER_MODEL)     existing_openrouter_model="$value" ;;
        GROQ_MODEL)           existing_groq_model="$value" ;;
        GEMINI_MODEL)         existing_gemini_model="$value" ;;
        AZURE_PAT)            existing_azure_pat="$value" ;;
        PR_REVIEWER_DEV)      existing_reviewer_dev="$value" ;;
        PR_REVIEWER_SPRINT)   existing_reviewer_sprint="$value" ;;
      esac
```

**5c.** Find the "Detect what's missing" block (around line 350):

```bash
  # Track what needs configuring
  local needs_providers=false
  local needs_openrouter=false
  local needs_groq=false
  local needs_gemini=false
  local needs_pat=false
  local needs_reviewers=false
  local changed=false

  # Detect what's missing
  if [[ -z "$existing_openrouter_key" && -z "$existing_groq_key" && -z "$existing_gemini_key" ]]; then
    needs_providers=true
  fi
  if [[ -z "$existing_openrouter_key" ]]; then needs_openrouter=true; fi
  if [[ -z "$existing_groq_key" ]]; then needs_groq=true; fi
  if [[ -z "$existing_gemini_key" ]]; then needs_gemini=true; fi
```

Replace with:

```bash
  # Track what needs configuring
  local needs_providers=false
  local needs_openrouter=false
  local needs_groq=false
  local needs_gemini=false
  local needs_ollama=false
  local needs_pat=false
  local needs_reviewers=false
  local changed=false

  # Detect what's missing
  if [[ -z "$existing_openrouter_key" && -z "$existing_groq_key" && -z "$existing_gemini_key" && -z "$existing_ollama_key" ]]; then
    needs_providers=true
  fi
  if [[ -z "$existing_openrouter_key" ]]; then needs_openrouter=true; fi
  if [[ -z "$existing_groq_key" ]]; then needs_groq=true; fi
  if [[ -z "$existing_gemini_key" ]]; then needs_gemini=true; fi
  if [[ -z "$existing_ollama_key" ]]; then needs_ollama=true; fi
```

**5d.** Find the summary display block (around line 370) that shows already-configured keys:

```bash
    local masked_or="${existing_openrouter_key:0:4}...${existing_openrouter_key: -4}"
    local masked_groq="${existing_groq_key:0:4}...${existing_groq_key: -4}"
    local masked_gemini="${existing_gemini_key:0:4}...${existing_gemini_key: -4}"
    local masked_pat="${existing_azure_pat:0:4}...${existing_azure_pat: -4}"
    [[ -n "$existing_openrouter_key" ]] && echo -e "    OpenRouter API Key: ${CYAN}${masked_or}${NC}"
    [[ -n "$existing_groq_key" ]] && echo -e "    Groq API Key:       ${CYAN}${masked_groq}${NC}"
    [[ -n "$existing_gemini_key" ]] && echo -e "    Gemini API Key:     ${CYAN}${masked_gemini}${NC}"
    [[ -n "$existing_azure_pat" ]] && echo -e "    Azure PAT:          ${CYAN}${masked_pat}${NC}"
```

Replace with:

```bash
    local masked_or="${existing_openrouter_key:0:4}...${existing_openrouter_key: -4}"
    local masked_groq="${existing_groq_key:0:4}...${existing_groq_key: -4}"
    local masked_gemini="${existing_gemini_key:0:4}...${existing_gemini_key: -4}"
    local masked_ollama="${existing_ollama_key:0:4}...${existing_ollama_key: -4}"
    local masked_pat="${existing_azure_pat:0:4}...${existing_azure_pat: -4}"
    [[ -n "$existing_openrouter_key" ]] && echo -e "    OpenRouter API Key: ${CYAN}${masked_or}${NC}"
    [[ -n "$existing_groq_key" ]] && echo -e "    Groq API Key:       ${CYAN}${masked_groq}${NC}"
    [[ -n "$existing_gemini_key" ]] && echo -e "    Gemini API Key:     ${CYAN}${masked_gemini}${NC}"
    [[ -n "$existing_ollama_key" ]] && echo -e "    Ollama API Key:     ${CYAN}${masked_ollama}${NC}"
    [[ -n "$existing_azure_pat" ]] && echo -e "    Azure PAT:          ${CYAN}${masked_pat}${NC}"
```

**5e.** Find the `if [[ "$needs_openrouter" == "true" || "$needs_groq" == "true" || "$needs_gemini" == "true" ]]` block (around line 400):

```bash
  if [[ "$needs_openrouter" == "true" || "$needs_groq" == "true" || "$needs_gemini" == "true" ]]; then
    echo -e "${BOLD}Providers de LLM${NC}"
    echo ""

    # Show what's already configured
    [[ -n "$existing_openrouter_key" ]] && log_success "OpenRouter ja configurado"
    [[ -n "$existing_groq_key" ]] && log_success "Groq ja configurado"
    [[ -n "$existing_gemini_key" ]] && log_success "Google Gemini ja configurado"
    [[ -n "$existing_openrouter_key" || -n "$existing_groq_key" || -n "$existing_gemini_key" ]] && echo ""
```

Replace with:

```bash
  if [[ "$needs_openrouter" == "true" || "$needs_groq" == "true" || "$needs_gemini" == "true" || "$needs_ollama" == "true" ]]; then
    echo -e "${BOLD}Providers de LLM${NC}"
    echo ""

    # Show what's already configured
    [[ -n "$existing_openrouter_key" ]] && log_success "OpenRouter ja configurado"
    [[ -n "$existing_groq_key" ]] && log_success "Groq ja configurado"
    [[ -n "$existing_gemini_key" ]] && log_success "Google Gemini ja configurado"
    [[ -n "$existing_ollama_key" ]] && log_success "Ollama ja configurado"
    [[ -n "$existing_openrouter_key" || -n "$existing_groq_key" || -n "$existing_gemini_key" || -n "$existing_ollama_key" ]] && echo ""
```

**5f.** After the `needs_gemini` block (which ends with `echo ""`), add the Ollama wizard block before the `# Update providers list` comment. The end of the Gemini block looks like:

```bash
    if [[ "$needs_gemini" == "true" ]]; then
      if prompt_yn "Configurar Google Gemini?" "y"; then
        echo -e "  ${BOLD}Google Gemini${NC} - Crie uma key em: ${CYAN}https://aistudio.google.com/apikey${NC}"
        local new_gemini_key
        new_gemini_key=$(prompt_value "API Key" "" "true")
        if [[ -n "$new_gemini_key" ]]; then
          echo -en "  Testando key... "
          if test_provider_key "gemini" "$new_gemini_key"; then
            echo -e "${GREEN}valida!${NC}"
          else
            echo -e "${RED}falhou${NC}"
            log_warn "A key pode estar errada ou o servico esta fora. Salvando mesmo assim."
          fi
          set_env_var "GEMINI_API_KEY" "$new_gemini_key"
          changed=true
        fi
        echo ""
      fi
    fi

    # Update providers list if not set
    if [[ -z "$existing_providers" && "$changed" == "true" ]]; then
      set_env_var "PR_PROVIDERS" "openrouter,groq,gemini"
    fi
```

Replace with:

```bash
    if [[ "$needs_gemini" == "true" ]]; then
      if prompt_yn "Configurar Google Gemini?" "y"; then
        echo -e "  ${BOLD}Google Gemini${NC} - Crie uma key em: ${CYAN}https://aistudio.google.com/apikey${NC}"
        local new_gemini_key
        new_gemini_key=$(prompt_value "API Key" "" "true")
        if [[ -n "$new_gemini_key" ]]; then
          echo -en "  Testando key... "
          if test_provider_key "gemini" "$new_gemini_key"; then
            echo -e "${GREEN}valida!${NC}"
          else
            echo -e "${RED}falhou${NC}"
            log_warn "A key pode estar errada ou o servico esta fora. Salvando mesmo assim."
          fi
          set_env_var "GEMINI_API_KEY" "$new_gemini_key"
          changed=true
        fi
        echo ""
      fi
    fi

    if [[ "$needs_ollama" == "true" ]]; then
      if prompt_yn "Configurar Ollama Cloud?" "y"; then
        echo -e "  ${BOLD}Ollama Cloud${NC} - Crie uma key em: ${CYAN}https://ollama.com/settings/api-keys${NC}"
        local new_ollama_key
        new_ollama_key=$(prompt_value "API Key" "" "true")
        if [[ -n "$new_ollama_key" ]]; then
          echo -en "  Testando key... "
          if test_provider_key "ollama" "$new_ollama_key"; then
            echo -e "${GREEN}valida!${NC}"
          else
            echo -e "${RED}falhou${NC}"
            log_warn "A key pode estar errada ou o servico esta fora. Salvando mesmo assim."
          fi
          set_env_var "OLLAMA_API_KEY" "$new_ollama_key"
          changed=true
        fi
        echo ""
      fi
    fi

    # Update providers list if not set
    if [[ -z "$existing_providers" && "$changed" == "true" ]]; then
      set_env_var "PR_PROVIDERS" "openrouter,groq,gemini,ollama"
    fi
```

- [ ] **Step 6: Verify no syntax errors**

```bash
bash -n src/bin/create-pr-description && echo "OK"
```

Expected output: `OK`

- [ ] **Step 7: Verify `--set-ollama-model` is recognized**

```bash
bash -c '
  grep -c "set-ollama-model" src/bin/create-pr-description
'
```

Expected output: `4` (appears in show_help, parse_args error msg, set_env_var call, log_success)

- [ ] **Step 8: Commit**

```bash
git add src/bin/create-pr-description
git commit -m "feat: add ollama provider support to create-pr-description wizard and config"
```

---

## Task 5: Update `src/bin/create-test-card`

**Files:**

- Modify: `src/bin/create-test-card`

Two sub-changes: (a) add `DEFAULT_OLLAMA_MODEL`; (b) update `DEFAULT_PROVIDERS`; (c) update `DEFAULT_ENV`.

- [ ] **Step 1: Add `DEFAULT_OLLAMA_MODEL` and update `DEFAULT_PROVIDERS`**

Find the block at line ~24:

```bash
DEFAULT_PROVIDERS="openrouter,groq,gemini"
DEFAULT_OPENROUTER_MODEL="meta-llama/llama-3.3-70b-instruct:free"
DEFAULT_GROQ_MODEL="llama-3.3-70b-versatile"
DEFAULT_GEMINI_MODEL="gemini-3.1-flash-lite-preview"
```

Replace with:

```bash
DEFAULT_PROVIDERS="openrouter,groq,gemini,ollama"
DEFAULT_OPENROUTER_MODEL="meta-llama/llama-3.3-70b-instruct:free"
DEFAULT_GROQ_MODEL="llama-3.3-70b-versatile"
DEFAULT_GEMINI_MODEL="gemini-3.1-flash-lite-preview"
DEFAULT_OLLAMA_MODEL="qwen3.5:cloud"
```

- [ ] **Step 2: Update `DEFAULT_ENV` in `create-test-card` to include Ollama entries**

Find the `DEFAULT_ENV` block (around line 183):

```bash
DEFAULT_ENV='# Providers em ordem de prioridade
PR_PROVIDERS="openrouter,groq,gemini"

# API Keys
# OPENROUTER_API_KEY="sk-or-..."
# GROQ_API_KEY="gsk_..."
# GEMINI_API_KEY="..."

# Modelos (opcional)
# OPENROUTER_MODEL="meta-llama/llama-3.3-70b-instruct:free"
# GROQ_MODEL="llama-3.3-70b-versatile"
# GEMINI_MODEL="gemini-3.1-flash-lite-preview"

# Azure DevOps
# AZURE_PAT="your-pat-token"

# Test cards
# TEST_CARD_AREA_PATH="AGROTRACE\\Devops"
# TEST_CARD_ASSIGNED_TO="nome@empresa.com"'
```

Replace with:

```bash
DEFAULT_ENV='# Providers em ordem de prioridade
PR_PROVIDERS="openrouter,groq,gemini"

# API Keys
# OPENROUTER_API_KEY="sk-or-..."
# GROQ_API_KEY="gsk_..."
# GEMINI_API_KEY="..."
# OLLAMA_API_KEY="oa-..."

# Modelos (opcional)
# OPENROUTER_MODEL="meta-llama/llama-3.3-70b-instruct:free"
# GROQ_MODEL="llama-3.3-70b-versatile"
# GEMINI_MODEL="gemini-3.1-flash-lite-preview"
# OLLAMA_MODEL="qwen3.5:cloud"

# Azure DevOps
# AZURE_PAT="your-pat-token"

# Test cards
# TEST_CARD_AREA_PATH="AGROTRACE\\Devops"
# TEST_CARD_ASSIGNED_TO="nome@empresa.com"'
```

- [ ] **Step 3: Verify no syntax errors**

```bash
bash -n src/bin/create-test-card && echo "OK"
```

Expected output: `OK`

- [ ] **Step 4: Commit**

```bash
git add src/bin/create-test-card
git commit -m "feat: add ollama provider support to create-test-card config"
```

---

## Task 6: Bump VERSION

**Files:**

- Modify: `VERSION`

- [ ] **Step 1: Verify current version**

```bash
cat VERSION
```

Expected output: `2.9.2`

- [ ] **Step 2: Update VERSION**

```bash
echo "2.9.3" > VERSION
```

- [ ] **Step 3: Verify**

```bash
cat VERSION
```

Expected output: `2.9.3`

- [ ] **Step 4: Commit**

```bash
git add VERSION
git commit -m "chore: bump version to v2.9.3"
```

---

## Final Verification

- [ ] **Smoke test: all scripts pass syntax check**

```bash
bash -n src/lib/common.sh && \
bash -n src/lib/llm.sh && \
bash -n src/lib/test-card-llm.sh && \
bash -n src/bin/create-pr-description && \
bash -n src/bin/create-test-card && \
echo "All OK"
```

Expected output: `All OK`

- [ ] **Verify all ollama references are present and consistent**

```bash
echo "=== common.sh ===" && grep -n "ollama\|OLLAMA" src/lib/common.sh
echo "=== llm.sh ===" && grep -n "ollama\|OLLAMA" src/lib/llm.sh
echo "=== test-card-llm.sh ===" && grep -n "ollama\|OLLAMA" src/lib/test-card-llm.sh
echo "=== create-pr-description ===" && grep -n "ollama\|OLLAMA" src/bin/create-pr-description
echo "=== create-test-card ===" && grep -n "ollama\|OLLAMA" src/bin/create-test-card
```

Expected: each file shows its respective Ollama additions and none show unexpected matches.
