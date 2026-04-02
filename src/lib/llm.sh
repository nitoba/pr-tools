#!/usr/bin/env bash
# lib/llm.sh — LLM provider logic (OpenAI-compatible, Gemini, fallback)
# Sourced by bin/create-pr-description after lib/common.sh.
# Communicates entirely via globals defined in the orchestrator script.

[[ -n "${_PR_TOOLS_LLM_SH:-}" ]] && return 0
_PR_TOOLS_LLM_SH=1

# ---- Source common.sh ----
_LLM_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/common.sh
source "${_LLM_LIB_DIR}/common.sh"

# ---- Provider Configuration ----

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
    *)
      log_warn "Provider desconhecido: $provider"
      PROVIDER_KEY=""
      return
      ;;
  esac
}

# ---- Payload Building ----

build_openai_compatible_payload() {
  local model="$1"
  local system_prompt="$2"
  local user_prompt="$3"
  local include_reasoning_format="$4"

  local system_tmp user_tmp
  system_tmp=$(mktemp)
  user_tmp=$(mktemp)

  printf '%s' "$system_prompt" > "$system_tmp"
  printf '%s' "$user_prompt" > "$user_tmp"

  if [[ "$include_reasoning_format" == "true" ]]; then
    jq -n \
      --arg model "$model" \
      --rawfile system "$system_tmp" \
      --rawfile user "$user_tmp" \
      '{
        model: $model,
        messages: [
          { role: "system", content: $system },
          { role: "user", content: $user }
        ],
        temperature: 0.3,
        reasoning_format: "hidden"
      }'
  else
    jq -n \
      --arg model "$model" \
      --rawfile system "$system_tmp" \
      --rawfile user "$user_tmp" \
      '{
        model: $model,
        messages: [
          { role: "system", content: $system },
          { role: "user", content: $user }
        ],
        temperature: 0.3
      }'
  fi

  rm -f "$system_tmp" "$user_tmp"
}

# ---- Request Execution ----

execute_openai_compatible_request() {
  local url="$1"
  local key="$2"
  local provider_name="$3"
  local payload_file="$4"

  local curl_args=(
    -s
    -w "\n%{http_code}"
    --max-time 120
    -H "Content-Type: application/json"
    -H "Authorization: Bearer $key"
  )

  if [[ "$provider_name" == "openrouter" ]]; then
    curl_args+=(-H "HTTP-Referer: https://github.com/create-pr-description")
    curl_args+=(-H "X-Title: create-pr-description")
  fi

  curl_args+=(-d @"$payload_file" "$url")
  curl "${curl_args[@]}" 2>/dev/null || printf '\n000'
}

# ---- Content Normalization ----

# Normalize LLM content: some models (e.g. qwen) emit literal \n (backslash-n)
# instead of real newlines inside SSE delta tokens. Convert them to real newlines.
normalize_llm_content() {
  sed 's/\\n/\n/g; s/\\t/\t/g'
}

# ---- Streaming Functions ----

parse_openai_sse_stream() {
  local accumulator_file="$1"
  local error_file="$2"

  > "$accumulator_file"
  > "$error_file"

  while IFS= read -r line; do
    # Strip carriage return (SSE uses \r\n)
    line="${line%%$'\r'}"

    # Skip empty lines (SSE delimiter)
    [[ -z "$line" ]] && continue

    # Check for stream end
    [[ "$line" == "data: [DONE]" ]] && break

    # Only process data: lines
    [[ "$line" != data:* ]] && continue

    # Extract JSON after "data: "
    local json="${line#data: }"

    # Check for error object
    local err
    err=$(printf '%s' "$json" | jq -r '.error.message // empty' 2>/dev/null)
    if [[ -n "$err" ]]; then
      printf '%s' "$err" > "$error_file"
      return 1
    fi

    # Extract delta content
    local token
    token=$(printf '%s' "$json" | jq -r '.choices[0].delta.content // empty' 2>/dev/null)

    if [[ -n "$token" ]]; then
      # Normalize literal \n sequences emitted by some models (e.g. qwen)
      local normalized_token
      normalized_token=$(printf '%s' "$token" | sed 's/\\n/\n/g; s/\\t/\t/g')
      printf '%s' "$normalized_token" >&2
      printf '%s' "$normalized_token" >> "$accumulator_file"
    fi
  done

  printf '\n' >&2
  return 0
}

parse_gemini_sse_stream() {
  local accumulator_file="$1"
  local error_file="$2"

  > "$accumulator_file"
  > "$error_file"

  while IFS= read -r line; do
    line="${line%%$'\r'}"
    [[ -z "$line" ]] && continue
    [[ "$line" != data:* ]] && continue

    local json="${line#data: }"

    # Check for error
    local err
    err=$(printf '%s' "$json" | jq -r '.error.message // empty' 2>/dev/null)
    if [[ -n "$err" ]]; then
      printf '%s' "$err" > "$error_file"
      return 1
    fi

    local token
    token=$(printf '%s' "$json" | jq -r '.candidates[0].content.parts[0].text // empty' 2>/dev/null)

    if [[ -n "$token" ]]; then
      local normalized_token
      normalized_token=$(printf '%s' "$token" | sed 's/\\n/\n/g; s/\\t/\t/g')
      printf '%s' "$normalized_token" >&2
      printf '%s' "$normalized_token" >> "$accumulator_file"
    fi
  done

  printf '\n' >&2
  return 0
}

execute_openai_compatible_stream_request() {
  local url="$1"
  local key="$2"
  local provider_name="$3"
  local payload_file="$4"
  local accumulator_file="$5"
  local error_file="$6"

  local header_file
  header_file=$(mktemp)

  local curl_args=(
    -N
    -s
    --max-time 120
    --dump-header "$header_file"
    -H "Content-Type: application/json"
    -H "Authorization: Bearer $key"
  )

  if [[ "$provider_name" == "openrouter" ]]; then
    curl_args+=(-H "HTTP-Referer: https://github.com/create-pr-description")
    curl_args+=(-H "X-Title: create-pr-description")
  fi

  curl_args+=(-d @"$payload_file" "$url")

  local parse_ok=true
  curl "${curl_args[@]}" 2>/dev/null | parse_openai_sse_stream "$accumulator_file" "$error_file" || parse_ok=false

  # Extract HTTP status code from dumped headers
  local http_code
  http_code=$(head -1 "$header_file" 2>/dev/null | grep -o '[0-9][0-9][0-9]' || echo "000")
  rm -f "$header_file"

  if [[ "$parse_ok" == "false" && "$http_code" == "200" ]]; then
    # SSE parser detected an error in the stream data
    http_code="500"
  fi

  echo "$http_code"
}

execute_gemini_stream_request() {
  local api_key="$1"
  local model="$2"
  local payload_file="$3"
  local accumulator_file="$4"
  local error_file="$5"

  local api_url="https://generativelanguage.googleapis.com/v1beta/models/${model}:streamGenerateContent?alt=sse&key=${api_key}"

  local header_file
  header_file=$(mktemp)

  local parse_ok=true
  curl -N -s \
    --max-time 120 \
    --dump-header "$header_file" \
    -H "Content-Type: application/json" \
    -d @"$payload_file" \
    "$api_url" 2>/dev/null | parse_gemini_sse_stream "$accumulator_file" "$error_file" || parse_ok=false

  local http_code
  http_code=$(head -1 "$header_file" 2>/dev/null | grep -o '[0-9][0-9][0-9]' || echo "000")
  rm -f "$header_file"

  if [[ "$parse_ok" == "false" && "$http_code" == "200" ]]; then
    http_code="500"
  fi

  echo "$http_code"
}

# ---- Error Detection ----

is_groq_reasoning_format_retryable_error() {
  local provider_name="$1"
  local http_code="$2"
  local body="$3"

  if [[ "$provider_name" != "groq" || "$http_code" != "400" ]]; then
    return 1
  fi

  local error_param error_message error_message_lc
  error_param=$(printf '%s' "$body" | jq -r '.error.param? // empty' 2>/dev/null || printf '')
  error_message=$(printf '%s' "$body" | jq -r '.error.message? // empty' 2>/dev/null || printf '')

  if [[ "$error_param" != "reasoning_format" || -z "$error_message" ]]; then
    return 1
  fi

  error_message_lc=$(printf '%s' "$error_message" | tr '[:upper:]' '[:lower:]')
  [[ "$error_message_lc" == *"reasoning_format"* && "$error_message_lc" == *"not supported"* ]]
}

# ---- API Call Functions ----

call_llm_api() {
  local url="$1"
  local key="$2"
  local model="$3"
  local system_prompt="$4"
  local user_prompt="$5"
  local provider_name="$6"

  # Build JSON payload using temp files to avoid "Argument list too long"
  # jq --arg passes data via process arguments which have OS size limits
  # Solution: write prompts to files, use jq --rawfile (jq 1.6+) to read them
  local payload_tmp
  payload_tmp=$(mktemp)

  local include_reasoning_format=false
  if [[ "$provider_name" == "groq" ]]; then
    include_reasoning_format=true
  fi

  build_openai_compatible_payload "$model" "$system_prompt" "$user_prompt" "$include_reasoning_format" > "$payload_tmp"

  # Streaming mode
  if [[ "$STREAM_MODE" == "true" ]]; then
    # Inject stream:true into payload
    local stream_payload_tmp
    stream_payload_tmp=$(mktemp)
    jq '. + {stream: true}' "$payload_tmp" > "$stream_payload_tmp"

    local accumulator_file error_file
    accumulator_file=$(mktemp)
    error_file=$(mktemp)

    local http_code
    http_code=$(execute_openai_compatible_stream_request "$url" "$key" "$provider_name" "$stream_payload_tmp" "$accumulator_file" "$error_file")

    # Handle Groq reasoning_format retry in streaming mode
    if [[ "$http_code" != "200" && "$provider_name" == "groq" && "$include_reasoning_format" == "true" ]]; then
      log_warn "Groq rejeitou reasoning_format para este modelo; tentando novamente sem esse parametro"
      build_openai_compatible_payload "$model" "$system_prompt" "$user_prompt" "false" > "$payload_tmp"
      jq '. + {stream: true}' "$payload_tmp" > "$stream_payload_tmp"
      > "$accumulator_file"
      > "$error_file"
      http_code=$(execute_openai_compatible_stream_request "$url" "$key" "$provider_name" "$stream_payload_tmp" "$accumulator_file" "$error_file")
    fi

    rm -f "$payload_tmp" "$stream_payload_tmp"

    if [[ "$http_code" == "000" ]]; then
      rm -f "$accumulator_file" "$error_file"
      log_warn "Timeout no provider $provider_name (sem resposta em 120s)"
      echo ""
      return 1
    elif [[ "$http_code" == "429" ]]; then
      rm -f "$accumulator_file" "$error_file"
      log_warn "Rate limit (HTTP 429) no provider $provider_name"
      echo ""
      return 1
    elif [[ "$http_code" != "200" ]]; then
      local err_msg
      err_msg=$(cat "$error_file" 2>/dev/null)
      rm -f "$accumulator_file" "$error_file"
      log_warn "HTTP $http_code de $provider_name ($url)${err_msg:+ - $err_msg}"
      echo ""
      return 1
    fi

    local content
    content=$(cat "$accumulator_file" | normalize_llm_content)
    rm -f "$accumulator_file" "$error_file"

    if [[ -z "$content" ]]; then
      log_warn "Resposta vazia ou inválida de $provider_name"
      printf ''
      return 1
    fi

    printf '%s' "$content"
    return 0
  fi

  # Non-streaming mode (original)
  local response
  response=$(execute_openai_compatible_request "$url" "$key" "$provider_name" "$payload_tmp")

  local http_code
  http_code="${response##*$'\n'}"
  local body
  body="${response%$'\n'*}"

  if [[ "$http_code" == "000" ]]; then
    rm -f "$payload_tmp"
    log_warn "Timeout no provider $provider_name (sem resposta em 120s)"
    echo ""
    return 1
  elif [[ "$http_code" == "429" ]]; then
    rm -f "$payload_tmp"
    log_warn "Rate limit (HTTP 429) no provider $provider_name"
    echo ""
    return 1
  elif [[ "$http_code" != "200" ]]; then
    if is_groq_reasoning_format_retryable_error "$provider_name" "$http_code" "$body"; then
      log_warn "Groq rejeitou reasoning_format para este modelo; tentando novamente sem esse parametro"
      build_openai_compatible_payload "$model" "$system_prompt" "$user_prompt" "false" > "$payload_tmp"
      response=$(execute_openai_compatible_request "$url" "$key" "$provider_name" "$payload_tmp")
      http_code="${response##*$'\n'}"
      body="${response%$'\n'*}"
    fi

    if [[ "$http_code" == "000" ]]; then
      rm -f "$payload_tmp"
      log_warn "Timeout no provider $provider_name (sem resposta em 120s)"
      echo ""
      return 1
    elif [[ "$http_code" == "429" ]]; then
      rm -f "$payload_tmp"
      log_warn "Rate limit (HTTP 429) no provider $provider_name"
      echo ""
      return 1
    elif [[ "$http_code" != "200" ]]; then
      rm -f "$payload_tmp"
      log_warn "HTTP $http_code de $provider_name ($url)"
      echo ""
      return 1
    fi
  fi

  rm -f "$payload_tmp"

  # Extract content
  local content
  content=$(printf '%s' "$body" | jq -r '.choices[0].message.content // empty' 2>/dev/null | normalize_llm_content || printf '')

  if [[ -z "$content" || "$content" == "null" ]]; then
    log_warn "Resposta vazia ou inválida de $provider_name"
    printf ''
    return 1
  fi

  printf '%s' "$content"
  return 0
}

call_gemini_api() {
  local key="$1"
  local model="$2"
  local system_prompt="$3"
  local user_prompt="$4"

  # Build Gemini-format payload using temp files
  local payload_tmp system_tmp user_tmp
  payload_tmp=$(mktemp)
  system_tmp=$(mktemp)
  user_tmp=$(mktemp)

  printf '%s' "$system_prompt" > "$system_tmp"
  printf '%s' "$user_prompt" > "$user_tmp"

  jq -n \
    --rawfile system "$system_tmp" \
    --rawfile user "$user_tmp" \
    '{
      system_instruction: {
        parts: [{ text: $system }]
      },
      contents: [
        {
          role: "user",
          parts: [{ text: $user }]
        }
      ],
      generationConfig: {
        temperature: 0.3
      }
    }' > "$payload_tmp"

  rm -f "$system_tmp" "$user_tmp"

  # Streaming mode
  if [[ "$STREAM_MODE" == "true" ]]; then
    local accumulator_file error_file
    accumulator_file=$(mktemp)
    error_file=$(mktemp)

    local http_code
    http_code=$(execute_gemini_stream_request "$key" "$model" "$payload_tmp" "$accumulator_file" "$error_file")

    rm -f "$payload_tmp"

    if [[ "$http_code" == "000" ]]; then
      rm -f "$accumulator_file" "$error_file"
      log_warn "Timeout no provider gemini (sem resposta em 120s)"
      echo ""
      return 1
    elif [[ "$http_code" == "429" ]]; then
      rm -f "$accumulator_file" "$error_file"
      log_warn "Rate limit (HTTP 429) no provider gemini"
      echo ""
      return 1
    elif [[ "$http_code" != "200" ]]; then
      local err_msg
      err_msg=$(cat "$error_file" 2>/dev/null)
      rm -f "$accumulator_file" "$error_file"
      log_warn "HTTP $http_code de gemini${err_msg:+ - $err_msg}"
      echo ""
      return 1
    fi

    local content
    content=$(cat "$accumulator_file" | normalize_llm_content)
    rm -f "$accumulator_file" "$error_file"

    if [[ -z "$content" ]]; then
      log_warn "Resposta vazia ou inválida de gemini"
      printf ''
      return 1
    fi

    printf '%s' "$content"
    return 0
  fi

  # Non-streaming mode (original)
  local api_url="https://generativelanguage.googleapis.com/v1beta/models/${model}:generateContent?key=${key}"

  local response
  response=$(curl -s -w "\n%{http_code}" \
    --max-time 120 \
    -H "Content-Type: application/json" \
    -d @"$payload_tmp" \
    "$api_url" 2>/dev/null || echo -e "\n000")

  rm -f "$payload_tmp"

  local http_code
  http_code=$(echo "$response" | tail -1)
  local body
  body=$(echo "$response" | sed '$d')

  if [[ "$http_code" == "000" ]]; then
    log_warn "Timeout no provider gemini (sem resposta em 120s)"
    echo ""
    return 1
  elif [[ "$http_code" == "429" ]]; then
    log_warn "Rate limit (HTTP 429) no provider gemini"
    echo ""
    return 1
  elif [[ "$http_code" != "200" ]]; then
    log_warn "HTTP $http_code de gemini"
    echo ""
    return 1
  fi

  # Gemini response format: candidates[0].content.parts[0].text
  local content
  content=$(printf '%s' "$body" | jq -r '.candidates[0].content.parts[0].text // empty' 2>/dev/null | normalize_llm_content || printf '')

  if [[ -z "$content" || "$content" == "null" ]]; then
    log_warn "Resposta vazia ou inválida de gemini"
    printf ''
    return 1
  fi

  printf '%s' "$content"
  return 0
}

# ---- Fallback Orchestration ----

call_with_fallback() {
  local system_prompt="$1"
  local user_prompt="$2"

  IFS=',' read -ra providers <<< "$PR_PROVIDERS"

  for provider in "${providers[@]}"; do
    provider=$(echo "$provider" | tr -d '[:space:]')
    get_provider_config "$provider"

    if [[ -z "$PROVIDER_KEY" ]]; then
      log_warn "API key não configurada para $provider. Pulando..."
      continue
    fi

    log_info "Tentando provider: $provider ($PROVIDER_MODEL)..."
    local result
    if [[ "$PROVIDER_NAME" == "gemini" ]]; then
      if result=$(call_gemini_api "$PROVIDER_KEY" "$PROVIDER_MODEL" "$system_prompt" "$user_prompt"); then
        if [[ -n "$result" ]]; then
          USED_PROVIDER="$provider"
          USED_MODEL="$PROVIDER_MODEL"
          LLM_RESULT="$result"
          return 0
        fi
      fi
    else
      if result=$(call_llm_api "$PROVIDER_URL" "$PROVIDER_KEY" "$PROVIDER_MODEL" "$system_prompt" "$user_prompt" "$PROVIDER_NAME"); then
        if [[ -n "$result" ]]; then
          USED_PROVIDER="$provider"
          USED_MODEL="$PROVIDER_MODEL"
          LLM_RESULT="$result"
          return 0
        fi
      fi
    fi
    log_warn "Provider $provider falhou. Tentando próximo..."
  done

  log_error "Todos os providers falharam. Verifique suas API keys e conexão."
  exit 1
}
