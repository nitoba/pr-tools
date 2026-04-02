#!/usr/bin/env bash
# lib/common.sh — shared utilities for pr-tools
# Sourced by bin/create-pr-description, bin/create-test-card, and other lib/ modules.

[[ -n "${_PR_TOOLS_COMMON_SH:-}" ]] && return 0
_PR_TOOLS_COMMON_SH=1

# ---- Colors ----

RED='\033[0;31m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
BOLD='\033[1m'
DIM='\033[2m'
NC='\033[0m'

if [[ ! -t 1 || -n "${NO_COLOR:-}" ]]; then
  RED=''
  YELLOW=''
  GREEN=''
  CYAN=''
  BOLD=''
  DIM=''
  NC=''
fi

# ---- Logging ----

log_error() {
  echo -e "${RED}[ERRO]${NC} $1" >&2
}

log_warn() {
  echo -e "${YELLOW}[AVISO]${NC} $1" >&2
}

log_info() {
  if [[ "${RAW_OUTPUT:-false}" == "true" ]]; then
    echo -e "${CYAN}[INFO]${NC} $1" >&2
  else
    echo -e "${CYAN}[INFO]${NC} $1"
  fi
}

log_success() {
  echo -e "${GREEN}[OK]${NC} $1"
}

debug_log() {
  if [[ "${DEBUG_MODE:-false}" == "true" ]]; then
    echo -e "${DIM}[DEBUG] $1${NC}" >&2
  fi
}

# ---- Env persistence ----

set_env_var() {
  local key="$1"
  local value="$2"
  local tmpfile found escaped

  mkdir -p "$CONFIG_DIR"
  touch "$ENV_FILE"
  chmod 600 "$ENV_FILE"

  found=false
  escaped=$(printf '%s' "$value" | sed 's/\\/\\\\/g; s/"/\\"/g')
  tmpfile="${ENV_FILE}.tmp"
  > "$tmpfile"

  while IFS= read -r line; do
    if [[ "$line" =~ ^[#[:space:]]*${key}= ]]; then
      echo "${key}=\"${escaped}\"" >> "$tmpfile"
      found=true
    else
      echo "$line" >> "$tmpfile"
    fi
  done < "$ENV_FILE"

  if [[ "$found" == "false" ]]; then
    echo "${key}=\"${escaped}\"" >> "$tmpfile"
  fi

  mv "$tmpfile" "$ENV_FILE"
  chmod 600 "$ENV_FILE"
}

ensure_env_key_comment() {
  local key="$1"
  local sample="$2"
  mkdir -p "$CONFIG_DIR"
  touch "$ENV_FILE"
  if ! grep -q "^[#[:space:]]*${key}=" "$ENV_FILE" 2>/dev/null; then
    printf '\n# %s="%s"\n' "$key" "$sample" >> "$ENV_FILE"
  fi
}

# ---- Prompts ----

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

# Read a value from user, with optional current value shown and hidden input
# Prompts go to stderr (visible on screen), only the result goes to stdout (captured by caller)
prompt_value() {
  local prompt_text="$1"
  local current_value="${2:-}"
  local is_secret="${3:-false}"
  local result=""

  if [[ -n "$current_value" ]]; then
    if [[ "$is_secret" == "true" ]]; then
      local masked="${current_value:0:4}...${current_value: -4}"
      echo -en "  ${prompt_text} [${CYAN}${masked}${NC}]: " >&2
    else
      echo -en "  ${prompt_text} [${CYAN}${current_value}${NC}]: " >&2
    fi
  else
    echo -en "  ${prompt_text}: " >&2
  fi

  if [[ "$is_secret" == "true" ]]; then
    read -rs result
    echo "" >&2  # newline after hidden input
  else
    read -r result
  fi

  # Keep current value if user pressed Enter without typing
  if [[ -z "$result" ]]; then
    result="$current_value"
  fi

  echo "$result"
}

# Ask a yes/no question
prompt_yn() {
  local question="$1"
  local default="${2:-n}"

  if [[ "$default" == "y" ]]; then
    echo -en "  ${question} [Y/n]: " >&2
  else
    echo -en "  ${question} [y/N]: " >&2
  fi

  read -r answer
  answer="${answer:-$default}"

  [[ "$answer" == "y" || "$answer" == "Y" ]]
}

# Choose from a numbered list, returns the chosen value
# Menu goes to stderr (visible), only the chosen value goes to stdout (captured)
prompt_choice() {
  local prompt_text="$1"
  shift
  local options=("$@")

  echo -e "  ${prompt_text}" >&2
  for i in "${!options[@]}"; do
    echo -e "    ${BOLD}$((i+1)))${NC} ${options[$i]}" >&2
  done
  echo -en "  Escolha [1-${#options[@]}]: " >&2
  read -r choice

  # Validate
  if [[ "$choice" =~ ^[0-9]+$ ]] && (( choice >= 1 && choice <= ${#options[@]} )); then
    echo "${options[$((choice-1))]}"
  else
    echo "${options[0]}"  # default to first
  fi
}

# ---- Provider validation ----

# Test if an API key works by making a simple call
test_provider_key() {
  local provider="$1"
  local key="$2"
  local url=""
  local model=""
  local extra_headers=()

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
  esac

  local payload
  payload=$(jq -n --arg model "$model" '{
    model: $model,
    messages: [{ role: "user", content: "Responda apenas: ok" }],
    max_tokens: 5
  }')

  local http_code
  http_code=$(curl -s -o /dev/null -w "%{http_code}" \
    --max-time 15 \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $key" \
    ${extra_headers[@]+"${extra_headers[@]}"} \
    -d "$payload" \
    "$url" 2>/dev/null || echo "000")

  if [[ "$http_code" == "200" ]]; then
    return 0
  elif [[ "$http_code" == "429" ]]; then
    # Rate limited but key is valid
    return 0
  else
    return 1
  fi
}

# Test Azure DevOps PAT
test_azure_pat() {
  local pat="$1"

  # Just test authentication against the Azure DevOps API
  local http_code
  http_code=$(curl -s -o /dev/null -w "%{http_code}" \
    --max-time 10 \
    -u ":$pat" \
    "https://dev.azure.com/_apis/profile/profiles/me?api-version=7.0" \
    2>/dev/null || echo "000")

  [[ "$http_code" == "200" ]]
}

# ---- Dependency validation ----

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

# ---- Config ----

load_config() {
  # Load .env file if it exists
  # Precedence: env var > .env > default
  # Only set vars that are not already defined in environment
  if [[ -f "$ENV_FILE" ]]; then
    while IFS= read -r line; do
      # Skip comments and empty lines
      [[ "$line" =~ ^[[:space:]]*# ]] && continue
      [[ -z "${line// /}" ]] && continue

      # Extract key and value
      local key="${line%%=*}"
      local value="${line#*=}"

      # Trim whitespace from key
      key=$(echo "$key" | xargs)

      # Skip if key is empty
      [[ -z "$key" ]] && continue

      # Remove surrounding quotes from value
      value="${value#\"}"
      value="${value%\"}"
      value="${value#\'}"
      value="${value%\'}"

      # Only set if not already defined in environment
      if [[ -z "${!key:-}" ]]; then
        export "$key=$value"
      fi
    done < "$ENV_FILE"
  fi

  # Apply defaults for unset vars
  PR_PROVIDERS="${PR_PROVIDERS:-${DEFAULT_PROVIDERS:-}}"
  OPENROUTER_MODEL="${OPENROUTER_MODEL:-${DEFAULT_OPENROUTER_MODEL:-}}"
  GROQ_MODEL="${GROQ_MODEL:-${DEFAULT_GROQ_MODEL:-}}"
  GEMINI_MODEL="${GEMINI_MODEL:-${DEFAULT_GEMINI_MODEL:-}}"

  # Support PR_STREAM env var for streaming mode (default: false)
  if [[ "${PR_STREAM:-}" == "true" ]]; then
    STREAM_MODE=true
  fi
}

# ---- Auto-update ----

# Parameterized update function.
# Usage: do_update <script_name> <current_version> <repo_url>
#   script_name     — basename of the script (e.g. "create-pr-description")
#   current_version — the caller's VERSION string
#   repo_url        — raw GitHub content base URL for the repo
do_update() {
  local script_name="$1"
  local current_version="$2"
  local repo_url="$3"

  local script_path
  # realpath may not exist on older macOS; readlink -f may not work either
  if command -v realpath &>/dev/null; then
    script_path=$(realpath "$0")
  elif readlink -f "$0" &>/dev/null; then
    script_path=$(readlink -f "$0")
  else
    # Fallback: resolve manually
    script_path="$0"
    if [[ "$script_path" != /* ]]; then
      script_path="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"
    fi
  fi

  log_info "Verificando atualizacao..."

  local remote_script
  remote_script=$(curl -fsSL "$repo_url/bin/$script_name" 2>/dev/null || echo "")

  if [[ -z "$remote_script" ]]; then
    log_error "Falha ao baixar atualizacao. Verifique sua conexao."
    exit 1
  fi

  # Extract remote version
  local remote_version
  remote_version=$(echo "$remote_script" | grep '^VERSION=' | head -1 | cut -d'"' -f2 || true)

  if [[ -z "$remote_version" ]]; then
    log_warn "Nao foi possivel determinar a versao remota."
    remote_version="desconhecida"
  fi

  if [[ "$remote_version" == "$current_version" ]]; then
    log_success "Voce ja esta na versao mais recente (v$current_version)."
    return
  fi

  log_info "Versao atual: v$current_version"
  log_info "Versao disponivel: v$remote_version"

  echo "$remote_script" > "$script_path"
  chmod +x "$script_path"
  log_success "Script atualizado para v$remote_version!"

  # Update lib files alongside the script
  local script_dir
  script_dir="$(cd "$(dirname "$script_path")" && pwd)"
  local lib_dir="${script_dir}/../lib/pr-tools"
  if [[ ! -d "$lib_dir" ]]; then
    lib_dir="${script_dir}/../lib"
  fi
  mkdir -p "$lib_dir"

  local lib_file lib_content
  for lib_file in common.sh llm.sh azure.sh test-card-azure.sh test-card-llm.sh; do
    lib_content=$(curl -fsSL "$repo_url/lib/$lib_file" 2>/dev/null || echo "")
    if [[ -n "$lib_content" ]]; then
      echo "$lib_content" > "$lib_dir/$lib_file"
      log_success "Lib atualizada: $lib_file"
    else
      log_warn "Falha ao baixar lib/$lib_file. Tente reinstalar: curl -fsSL $repo_url/install.sh | bash"
    fi
  done
}
