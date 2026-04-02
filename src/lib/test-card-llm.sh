#!/usr/bin/env bash
# lib/test-card-llm.sh — LLM functions for create-test-card
# Sourced by bin/create-test-card.

[[ -n "${_PR_TOOLS_TEST_CARD_LLM_SH:-}" ]] && return 0
_PR_TOOLS_TEST_CARD_LLM_SH=1

_tcllm_lib_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$_tcllm_lib_dir/common.sh"

strip_html() {
  local text="$1"
  printf '%s' "$text" | sed 's/<[^>]*>/ /g' | tr '\r' ' ' | tr '\n' ' ' | sed 's/[[:space:]]\+/ /g' | sed 's/^ //; s/ $//'
}

truncate_text() {
  local text="$1"
  local max_chars="$2"
  if (( ${#text} > max_chars )); then
    printf '%s' "${text:0:max_chars}"
    printf ' [texto truncado]'
  else
    printf '%s' "$text"
  fi
}

build_user_prompt() {
  local work_desc pr_desc
  work_desc=$(truncate_text "$(strip_html "$WORK_ITEM_DESCRIPTION")" 1800)
  pr_desc=$(truncate_text "$(strip_html "$PR_DESCRIPTION")" 1200)

  cat <<EOF
## Contexto do Work Item

ID: $WORK_ITEM_ID
Título: $WORK_ITEM_TITLE
Tipo: $WORK_ITEM_TYPE
Área: ${WORK_ITEM_AREA_PATH:-N/A}
Iteração: ${WORK_ITEM_ITERATION_PATH:-N/A}
Prioridade: ${WORK_ITEM_PRIORITY:-N/A}
Descrição:
${work_desc:-"(sem descrição)"}

## Contexto do PR

PR: $PR_ID
Título: $PR_TITLE
Status: ${PR_STATUS:-N/A}
Branch origem: ${PR_SOURCE_REF:-N/A}
Branch destino: ${PR_TARGET_REF:-N/A}
Repositório: ${AZURE_REPO:-N/A}
Descrição:
${pr_desc:-"(sem descrição)"}

## Work Items vinculados ao PR

${LINKED_WORK_ITEMS_SUMMARY:-"(não disponível)"}

## Arquivos alterados e resumo técnico

${DIFF_SUMMARY:-"(não disponível)"}

## Exemplos de Test Case

${EXAMPLES_SUMMARY:-"(não disponível)"}

## Instruções finais

Gere um card de teste em Markdown, seguindo exatamente o formato pedido no system prompt.
Não invente comportamento fora do contexto.
Use o contexto técnico apenas para entender a mudança; não exponha detalhes de código, arquivos ou implementação na resposta final.
EOF
}

build_openai_payload() {
  local model="$1"
  local system_prompt="$2"
  local user_prompt="$3"
  local payload_file="$4"
  local system_tmp user_tmp
  system_tmp=$(mktemp)
  user_tmp=$(mktemp)
  printf '%s' "$system_prompt" > "$system_tmp"
  printf '%s' "$user_prompt" > "$user_tmp"
  jq -n \
    --arg model "$model" \
    --rawfile system "$system_tmp" \
    --rawfile user "$user_tmp" \
    '{model:$model,messages:[{role:"system",content:$system},{role:"user",content:$user}],temperature:0.3}' \
    > "$payload_file"
  rm -f "$system_tmp" "$user_tmp"
}

call_openai_provider() {
  local provider="$1"
  local url="$2"
  local key="$3"
  local model="$4"
  local user_prompt="$5"
  local payload_tmp response code body content

  payload_tmp=$(mktemp)
  build_openai_payload "$model" "$DEFAULT_SYSTEM_PROMPT" "$user_prompt" "$payload_tmp"

  local curl_args=(
    -s -w "\n%{http_code}" --max-time 120
    -H "Content-Type: application/json"
    -H "Authorization: Bearer $key"
  )
  if [[ "$provider" == "openrouter" ]]; then
    curl_args+=(-H "HTTP-Referer: https://github.com/create-test-card")
    curl_args+=(-H "X-Title: create-test-card")
  fi
  curl_args+=(-d @"$payload_tmp" "$url")

  response=$(curl "${curl_args[@]}" 2>/dev/null || printf '\n000')
  rm -f "$payload_tmp"

  code=$(response_code "$response")
  body=$(response_body "$response")
  if [[ "$code" != "200" ]]; then
    debug_log "Provider $provider retornou HTTP $code"
    debug_log "$body"
    return 1
  fi
  content=$(printf '%s' "$body" | jq -r '.choices[0].message.content // empty' | sed 's/\\n/\n/g; s/\\t/\t/g')
  [[ -n "$content" ]] || return 1
  LLM_RESULT="$content"
  USED_PROVIDER="$provider"
  USED_MODEL="$model"
  return 0
}

call_gemini_provider() {
  local key="$1"
  local model="$2"
  local user_prompt="$3"
  local payload_tmp system_tmp user_tmp response code body content

  payload_tmp=$(mktemp)
  system_tmp=$(mktemp)
  user_tmp=$(mktemp)
  printf '%s' "$DEFAULT_SYSTEM_PROMPT" > "$system_tmp"
  printf '%s' "$user_prompt" > "$user_tmp"

  jq -n \
    --rawfile system "$system_tmp" \
    --rawfile user "$user_tmp" \
    '{system_instruction:{parts:[{text:$system}]},contents:[{role:"user",parts:[{text:$user}]}],generationConfig:{temperature:0.3}}' \
    > "$payload_tmp"
  rm -f "$system_tmp" "$user_tmp"

  response=$(curl -s -w "\n%{http_code}" --max-time 120 \
    -H "Content-Type: application/json" \
    -d @"$payload_tmp" "https://generativelanguage.googleapis.com/v1beta/models/${model}:generateContent?key=${key}" 2>/dev/null || printf '\n000')
  rm -f "$payload_tmp"

  code=$(response_code "$response")
  body=$(response_body "$response")
  if [[ "$code" != "200" ]]; then
    debug_log "Provider gemini retornou HTTP $code"
    debug_log "$body"
    return 1
  fi
  content=$(printf '%s' "$body" | jq -r '.candidates[0].content.parts[0].text // empty' | sed 's/\\n/\n/g; s/\\t/\t/g')
  [[ -n "$content" ]] || return 1
  LLM_RESULT="$content"
  USED_PROVIDER="gemini"
  USED_MODEL="$model"
  return 0
}

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
    esac
  done

  log_error "Todos os providers falharam."
  exit 1
}

parse_llm_result() {
  # Search for TITULO: line (case-insensitive) anywhere in the output
  local found_title=false
  local title_line_num=0
  local current_line=0

  while IFS= read -r line; do
    current_line=$((current_line + 1))
    if [[ "$line" =~ ^[[:space:]]*[Tt][Ii][Tt][Uu][Ll][Oo]:[[:space:]]*(.+)$ ]]; then
      GENERATED_TITLE="${BASH_REMATCH[1]}"
      found_title=true
      title_line_num=$current_line
      break
    fi
  done <<< "$LLM_RESULT"

  if [[ "$found_title" == "true" && -n "$GENERATED_TITLE" ]]; then
    # Collect only lines AFTER the TITULO: line as markdown body
    local body_lines=""
    local line_count=0
    while IFS= read -r line; do
      line_count=$((line_count + 1))
      if [[ $line_count -gt $title_line_num ]]; then
        body_lines="${body_lines}${line}"$'\n'
      fi
    done <<< "$LLM_RESULT"
    GENERATED_MARKDOWN="$body_lines"
  else
    # Fallback: use first non-empty line as title, full content as body
    GENERATED_TITLE=$(printf '%s' "$LLM_RESULT" | grep -m1 -v '^[[:space:]]*$' | sed 's/^#* *//')
    GENERATED_MARKDOWN="$LLM_RESULT"
    debug_log "Resposta da LLM não seguiu o formato TITULO:, aplicando recuperação leve."
  fi

  GENERATED_TITLE=$(printf '%s' "$GENERATED_TITLE" | sed 's/^ *//; s/ *$//')
  GENERATED_MARKDOWN=$(printf '%s\n' "$GENERATED_MARKDOWN" | sed '/./,$!d')
  [[ -n "$GENERATED_TITLE" ]] || GENERATED_TITLE="Teste | Validar alteracoes do PR #$PR_ID"
}

strip_think_blocks() {
  printf '%s\n' "$1" | awk '
    /<think>/ { skip=1; next }
    /<\/think>/ { skip=0; next }
    skip != 1 { print }
  '
}

markdown_inline_to_html() {
  local escaped
  escaped=$(printf '%s' "$1" | sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g')
  printf '%s' "$escaped" | sed 's/\*\*\([^*][^*]*\)\*\*/<b>\1<\/b>/g'
}

markdown_to_html() {
  local input="$1"
  local html=""
  local in_ul=false in_ol=false
  while IFS= read -r line || [[ -n "$line" ]]; do
    if [[ "$line" =~ ^[[:space:]]*$ ]]; then
      if [[ "$in_ul" == "true" ]]; then html+="</ul>"$'\n'; in_ul=false; fi
      if [[ "$in_ol" == "true" ]]; then html+="</ol>"$'\n'; in_ol=false; fi
      continue
    fi

    if [[ "$line" =~ ^##[[:space:]]+(.+)$ ]]; then
      if [[ "$in_ul" == "true" ]]; then html+="</ul>"$'\n'; in_ul=false; fi
      if [[ "$in_ol" == "true" ]]; then html+="</ol>"$'\n'; in_ol=false; fi
      html+="<h2>$(markdown_inline_to_html "${BASH_REMATCH[1]}")</h2>"$'\n'
    elif [[ "$line" =~ ^-[[:space:]]+(.+)$ ]]; then
      if [[ "$in_ol" == "true" ]]; then html+="</ol>"$'\n'; in_ol=false; fi
      if [[ "$in_ul" == "false" ]]; then html+="<ul>"$'\n'; in_ul=true; fi
      html+="<li>$(markdown_inline_to_html "${BASH_REMATCH[1]}")</li>"$'\n'
    elif [[ "$line" =~ ^[0-9]+\.[[:space:]]+(.+)$ ]]; then
      if [[ "$in_ul" == "true" ]]; then html+="</ul>"$'\n'; in_ul=false; fi
      if [[ "$in_ol" == "false" ]]; then html+="<ol>"$'\n'; in_ol=true; fi
      html+="<li>$(markdown_inline_to_html "${BASH_REMATCH[1]}")</li>"$'\n'
    else
      if [[ "$in_ul" == "true" ]]; then html+="</ul>"$'\n'; in_ul=false; fi
      if [[ "$in_ol" == "true" ]]; then html+="</ol>"$'\n'; in_ol=false; fi
      html+="<p>$(markdown_inline_to_html "$line")</p>"$'\n'
    fi
  done <<< "$input"

  if [[ "$in_ul" == "true" ]]; then html+="</ul>"$'\n'; fi
  if [[ "$in_ol" == "true" ]]; then html+="</ol>"$'\n'; fi
  printf '%s' "$html"
}

xml_escape() {
  printf '%s' "$1" | sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g; s/"/\&quot;/g'
}

markdown_to_azure_steps() {
  local input="$1"
  local in_checklist=false
  local step_id=2
  local steps_xml=''

  while IFS= read -r line || [[ -n "$line" ]]; do
    if [[ "$line" =~ ^##[[:space:]]+Checklist[[:space:]]+de[[:space:]]+testes ]]; then
      in_checklist=true
      continue
    fi

    if [[ "$in_checklist" == "true" && "$line" =~ ^##[[:space:]]+ ]]; then
      break
    fi

    if [[ "$in_checklist" != "true" ]]; then
      continue
    fi

    if [[ "$line" =~ ^[[:space:]]*-[[:space:]]+\[[^]]*\][[:space:]]+(.+)$ ]]; then
      local action_text escaped_action
      action_text="${BASH_REMATCH[1]}"
      escaped_action=$(xml_escape "$action_text")
      steps_xml+="<step id=\"${step_id}\" type=\"ActionStep\"><parameterizedString isformatted=\"true\">&lt;DIV&gt;&lt;P&gt;${escaped_action}&lt;/P&gt;&lt;/DIV&gt;</parameterizedString><parameterizedString isformatted=\"true\">&lt;DIV&gt;&lt;P&gt;&lt;BR/&gt;&lt;/P&gt;&lt;/DIV&gt;</parameterizedString><description/></step>"
      step_id=$((step_id + 1))
      continue
    fi

    if [[ "$line" =~ ^[[:space:]]*-[[:space:]]+(.+)$ ]]; then
      local action_text escaped_action
      action_text="${BASH_REMATCH[1]}"
      escaped_action=$(xml_escape "$action_text")
      steps_xml+="<step id=\"${step_id}\" type=\"ActionStep\"><parameterizedString isformatted=\"true\">&lt;DIV&gt;&lt;P&gt;${escaped_action}&lt;/P&gt;&lt;/DIV&gt;</parameterizedString><parameterizedString isformatted=\"true\">&lt;DIV&gt;&lt;P&gt;&lt;BR/&gt;&lt;/P&gt;&lt;/DIV&gt;</parameterizedString><description/></step>"
      step_id=$((step_id + 1))
      continue
    fi

    if [[ "$line" =~ ^[[:space:]]*[0-9]+\.[[:space:]]+(.+)$ ]]; then
      local action_text escaped_action
      action_text="${BASH_REMATCH[1]}"
      escaped_action=$(xml_escape "$action_text")
      steps_xml+="<step id=\"${step_id}\" type=\"ActionStep\"><parameterizedString isformatted=\"true\">&lt;DIV&gt;&lt;P&gt;${escaped_action}&lt;/P&gt;&lt;/DIV&gt;</parameterizedString><parameterizedString isformatted=\"true\">&lt;DIV&gt;&lt;P&gt;&lt;BR/&gt;&lt;/P&gt;&lt;/DIV&gt;</parameterizedString><description/></step>"
      step_id=$((step_id + 1))
    fi
  done <<< "$input"

  if [[ -z "$steps_xml" ]]; then
    local fallback escaped_fallback
    fallback=$(xml_escape "$GENERATED_TITLE")
    steps_xml="<step id=\"2\" type=\"ActionStep\"><parameterizedString isformatted=\"true\">&lt;DIV&gt;&lt;P&gt;${fallback}&lt;/P&gt;&lt;/DIV&gt;</parameterizedString><parameterizedString isformatted=\"true\">&lt;DIV&gt;&lt;P&gt;&lt;BR/&gt;&lt;/P&gt;&lt;/DIV&gt;</parameterizedString><description/></step>"
    step_id=3
  fi

  printf '<steps id="0" last="%d">%s</steps>' "$((step_id - 1))" "$steps_xml"
}
