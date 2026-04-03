#!/usr/bin/env bash
# lib/test-card-azure.sh — Azure DevOps functions for create-test-card
# Sourced by bin/create-test-card.

[[ -n "${_PR_TOOLS_TEST_CARD_AZURE_SH:-}" ]] && return 0
_PR_TOOLS_TEST_CARD_AZURE_SH=1

_tcaz_lib_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$_tcaz_lib_dir/common.sh"

parse_azure_remote() {
  AZURE_ORG=""
  AZURE_PROJECT=""
  AZURE_REPO=""
  IS_AZURE_DEVOPS=false

  if [[ "$IN_GIT_REPO" != "true" ]]; then
    return
  fi

  local remote_url
  remote_url=$(git remote get-url origin 2>/dev/null || echo "")
  [[ -n "$remote_url" ]] || return

  if [[ "$remote_url" =~ dev\.azure\.com[/:]([^/]+)/([^/]+)/_git/([^/]+) ]]; then
    AZURE_ORG="${BASH_REMATCH[1]}"
    AZURE_PROJECT="${BASH_REMATCH[2]}"
    AZURE_REPO="${BASH_REMATCH[3]}"
    IS_AZURE_DEVOPS=true
  elif [[ "$remote_url" =~ ssh\.dev\.azure\.com:v3/([^/]+)/([^/]+)/([^/]+) ]]; then
    AZURE_ORG="${BASH_REMATCH[1]}"
    AZURE_PROJECT="${BASH_REMATCH[2]}"
    AZURE_REPO="${BASH_REMATCH[3]}"
    IS_AZURE_DEVOPS=true
  fi

  AZURE_REPO="${AZURE_REPO%.git}"
}

resolve_routing() {
  if [[ -n "$CLI_ORG" ]]; then AZURE_ORG="$CLI_ORG"; fi
  if [[ -n "$CLI_PROJECT" ]]; then AZURE_PROJECT="$CLI_PROJECT"; fi
  if [[ -n "$CLI_REPO" ]]; then AZURE_REPO="$CLI_REPO"; fi

  if [[ -z "$AZURE_ORG" || -z "$AZURE_PROJECT" ]]; then
    log_error "Não foi possível resolver org/project do Azure DevOps. Use --org e --project ou rode dentro de um repo Azure DevOps."
    exit 1
  fi

  AZURE_ORG_ENC=$(uri_encode "$AZURE_ORG")
  AZURE_PROJECT_ENC=$(uri_encode "$AZURE_PROJECT")
  AZURE_REPO_ENC=$(uri_encode "$AZURE_REPO")
}

azure_get() {
  local url="$1"
  curl -s -w "\n%{http_code}" --max-time 60 -u ":$AZURE_PAT" "$url" 2>/dev/null || echo -e "\n000"
}

azure_post_json() {
  local url="$1"
  local payload="$2"
  curl -s -w "\n%{http_code}" --max-time 60 -u ":$AZURE_PAT" \
    -H "Content-Type: application/json" -d "$payload" "$url" 2>/dev/null || echo -e "\n000"
}

azure_patch_json() {
  local url="$1"
  local payload_file="$2"
  curl -s -w "\n%{http_code}" --max-time 60 -u ":$AZURE_PAT" \
    -X PATCH \
    -H "Content-Type: application/json-patch+json" \
    --data @"$payload_file" "$url" 2>/dev/null || echo -e "\n000"
}

extract_azure_error_message() {
  local body="$1"
  printf '%s' "$body" | jq -r '.message // .Message // .value.message // .value.Message // ""' 2>/dev/null || true
}

response_body() {
  printf '%s\n' "$1" | sed '$d'
}

response_code() {
  printf '%s\n' "$1" | tail -1
}

uri_encode() {
  jq -rn --arg v "$1" '$v|@uri'
}

fetch_pr_by_id() {
  local response body code
  response=$(azure_get "https://dev.azure.com/$AZURE_ORG_ENC/$AZURE_PROJECT_ENC/_apis/git/pullrequests/$PR_ID?api-version=7.0")
  code=$(response_code "$response")
  body=$(response_body "$response")

  if [[ "$code" != "200" ]]; then
    log_error "Falha ao buscar PR #$PR_ID (HTTP $code)."
    debug_log "$body"
    exit 1
  fi

  PR_TITLE=$(printf '%s' "$body" | jq -r '.title // empty')
  PR_DESCRIPTION=$(printf '%s' "$body" | jq -r '.description // empty')
  PR_STATUS=$(printf '%s' "$body" | jq -r '.status // empty')
  PR_SOURCE_REF=$(printf '%s' "$body" | jq -r '.sourceRefName // empty')
  PR_TARGET_REF=$(printf '%s' "$body" | jq -r '.targetRefName // empty')
  PR_REPOSITORY_NAME=$(printf '%s' "$body" | jq -r '.repository.name // empty')
  AZURE_REPO_ID=$(printf '%s' "$body" | jq -r '.repository.id // empty')

  if [[ -z "$PR_REPOSITORY_NAME" ]]; then
    log_error "Não foi possível identificar o repositório do PR #$PR_ID."
    exit 1
  fi

  if [[ -n "$CLI_REPO" && "$CLI_REPO" != "$PR_REPOSITORY_NAME" ]]; then
    log_error "O PR #$PR_ID pertence ao repositório '$PR_REPOSITORY_NAME', diferente do --repo informado ('$CLI_REPO')."
    exit 1
  fi

  AZURE_REPO="$PR_REPOSITORY_NAME"
}

search_pr_by_branch() {
  if [[ -z "$AZURE_REPO" ]]; then
    log_error "Não foi possível autodetectar o PR sem repositório. Use --repo ou --pr explicitamente."
    exit 1
  fi
  if [[ -z "$BRANCH_NAME" ]]; then
    log_error "Não foi possível autodetectar o PR sem branch atual. Use --pr explicitamente."
    exit 1
  fi

  local encoded_ref response body code active_count
  encoded_ref=$(uri_encode "$SOURCE_REF_NAME")

  response=$(azure_get "https://dev.azure.com/$AZURE_ORG_ENC/$AZURE_PROJECT_ENC/_apis/git/repositories/$AZURE_REPO_ENC/pullrequests?searchCriteria.status=active&searchCriteria.sourceRefName=$encoded_ref&api-version=7.0")
  code=$(response_code "$response")
  body=$(response_body "$response")
  if [[ "$code" != "200" ]]; then
    log_error "Falha ao buscar PRs ativos da branch '$BRANCH_NAME' (HTTP $code)."
    debug_log "$body"
    exit 1
  fi

  active_count=$(printf '%s' "$body" | jq -r '.count // 0')
  if [[ "$active_count" == "0" ]]; then
    response=$(azure_get "https://dev.azure.com/$AZURE_ORG_ENC/$AZURE_PROJECT_ENC/_apis/git/repositories/$AZURE_REPO_ENC/pullrequests?searchCriteria.status=all&searchCriteria.sourceRefName=$encoded_ref&api-version=7.0")
    code=$(response_code "$response")
    body=$(response_body "$response")
    if [[ "$code" != "200" ]]; then
      log_error "Falha ao buscar PRs da branch '$BRANCH_NAME' (HTTP $code)."
      debug_log "$body"
      exit 1
    fi
  fi

  local pr_json count candidate_ids candidate_id ranking_tmp
  count=$(printf '%s' "$body" | jq -r '.count // 0')
  if [[ "$count" == "0" ]]; then
    log_error "Nenhum PR encontrado para a branch '$BRANCH_NAME'. Use --pr explicitamente."
    exit 1
  fi

  if (( count > 1 )); then
    debug_log "Múltiplos PRs encontrados para a branch '$BRANCH_NAME'; selecionando por prioridade de status/data."
  fi

  ranking_tmp=$(mktemp)
  : > "$ranking_tmp"
  candidate_ids=$(printf '%s' "$body" | jq -r '.value[]?.pullRequestId')
  while IFS= read -r candidate_id; do
    [[ -n "$candidate_id" ]] || continue
    local det_resp det_body det_code det_status det_rank det_date thread_resp thread_body thread_code thread_date
    det_resp=$(azure_get "https://dev.azure.com/$AZURE_ORG_ENC/$AZURE_PROJECT_ENC/_apis/git/pullrequests/$candidate_id?api-version=7.0")
    det_code=$(response_code "$det_resp")
    det_body=$(response_body "$det_resp")
    [[ "$det_code" == "200" ]] || continue
    det_status=$(printf '%s' "$det_body" | jq -r '.status // empty')
    if [[ "$det_status" == "active" ]]; then det_rank=0; else det_rank=1; fi
    thread_resp=$(azure_get "https://dev.azure.com/$AZURE_ORG_ENC/$AZURE_PROJECT_ENC/_apis/git/repositories/$AZURE_REPO_ENC/pullRequests/$candidate_id/threads?api-version=7.0")
    thread_code=$(response_code "$thread_resp")
    thread_body=$(response_body "$thread_resp")
    if [[ "$thread_code" == "200" ]]; then
      thread_date=$(printf '%s' "$thread_body" | jq -r '.value | map(.lastUpdatedDate // .publishedDate // "") | sort | last // empty')
    else
      thread_date=""
    fi
    det_date=$(printf '%s' "$det_body" | jq -r --arg threadDate "$thread_date" '[$threadDate, .completionQueueTime // "", .closedDate // "", .lastMergeSourceCommit.committer.date // "", .creationDate // ""] | sort | last // ""')
    printf '%s' "$det_body" | jq -c --argjson rank "$det_rank" --arg date "$det_date" '{rank:$rank, date:$date, pr:.}' >> "$ranking_tmp"
    printf '\n' >> "$ranking_tmp"
  done <<< "$candidate_ids"

  pr_json=$(jq -cs 'sort_by(.date) | reverse | sort_by(.rank) | .[0].pr' "$ranking_tmp")
  rm -f "$ranking_tmp"
  PR_ID=$(printf '%s' "$pr_json" | jq -r '.pullRequestId // empty')
  PR_TITLE=$(printf '%s' "$pr_json" | jq -r '.title // empty')
  PR_DESCRIPTION=$(printf '%s' "$pr_json" | jq -r '.description // empty')
  PR_STATUS=$(printf '%s' "$pr_json" | jq -r '.status // empty')
  PR_SOURCE_REF=$(printf '%s' "$pr_json" | jq -r '.sourceRefName // empty')
  PR_TARGET_REF=$(printf '%s' "$pr_json" | jq -r '.targetRefName // empty')
  PR_REPOSITORY_NAME=$(printf '%s' "$pr_json" | jq -r '.repository.name // empty')
  AZURE_REPO_ID=$(printf '%s' "$pr_json" | jq -r '.repository.id // empty')

  if [[ -z "$PR_ID" ]]; then
    log_error "Falha ao selecionar um PR válido para a branch '$BRANCH_NAME'."
    exit 1
  fi
}

fetch_item_content_to_file() {
  local branch_name="$1"
  local item_path="$2"
  local output_file="$3"
  local encoded_path encoded_branch http_code
  encoded_path=$(uri_encode "$item_path")
  encoded_branch=$(uri_encode "$branch_name")
  http_code=$(curl -s -o "$output_file" -w "%{http_code}" --max-time 30 -u ":$AZURE_PAT" \
    "https://dev.azure.com/$AZURE_ORG_ENC/$AZURE_PROJECT_ENC/_apis/git/repositories/$AZURE_REPO_ENC/items?path=${encoded_path}&versionDescriptor.version=${encoded_branch}&versionDescriptor.versionType=branch&includeContent=true&api-version=7.0" \
    2>/dev/null || echo "000")
  [[ "$http_code" == "200" ]]
}

file_has_nul_byte() {
  local file_path="$1"
  LC_ALL=C od -An -tx1 -N 4096 "$file_path" 2>/dev/null | grep -q '00'
}

resolve_pr() {
  log_info "Resolvendo PR..."
  if [[ -n "$CLI_PR_ID" ]]; then
    PR_ID="$CLI_PR_ID"
    fetch_pr_by_id
    return
  fi

  if [[ "$IN_GIT_REPO" != "true" ]]; then
    log_error "Fora de um repositório git você precisa informar --pr explicitamente."
    exit 1
  fi
  if [[ -z "$BRANCH_NAME" ]]; then
    log_error "Não foi possível identificar a branch atual (detached HEAD?). Use --pr explicitamente."
    exit 1
  fi

  search_pr_by_branch
}

fetch_pr_linked_workitems() {
  local response body code work_ids ranked_ids selected_id
  response=$(azure_get "https://dev.azure.com/$AZURE_ORG_ENC/$AZURE_PROJECT_ENC/_apis/git/repositories/$AZURE_REPO_ENC/pullRequests/$PR_ID/workitems?api-version=7.0")
  code=$(response_code "$response")
  body=$(response_body "$response")

  if [[ "$code" != "200" ]]; then
    log_error "Falha ao buscar work items vinculados ao PR #$PR_ID (HTTP $code)."
    debug_log "$body"
    exit 1
  fi

  work_ids=$(printf '%s' "$body" | jq -r '.value[]?.id')
  if [[ -z "$work_ids" ]]; then
    log_error "O PR #$PR_ID não possui work items vinculados. Use --work-item explicitamente."
    exit 1
  fi

  LINKED_WORK_ITEMS_SUMMARY=""
  ranked_ids=""
  while IFS= read -r id; do
    [[ -n "$id" ]] || continue
    local wi_resp wi_body wi_code wi_title wi_type wi_state
    wi_resp=$(azure_get "https://dev.azure.com/$AZURE_ORG_ENC/$AZURE_PROJECT_ENC/_apis/wit/workitems/$id?api-version=7.0")
    wi_code=$(response_code "$wi_resp")
    wi_body=$(response_body "$wi_resp")
    if [[ "$wi_code" != "200" ]]; then
      continue
    fi
    wi_title=$(printf '%s' "$wi_body" | jq -r '.fields["System.Title"] // empty')
    wi_type=$(printf '%s' "$wi_body" | jq -r '.fields["System.WorkItemType"] // empty')
    wi_state=$(printf '%s' "$wi_body" | jq -r '.fields["System.State"] // empty')
    LINKED_WORK_ITEMS_SUMMARY+="- #$id [$wi_type] $wi_title ($wi_state)"$'\n'
    ranked_ids+="$id|$wi_type"$'\n'
  done <<< "$work_ids"

  if [[ -z "$ranked_ids" ]]; then
    log_error "Não foi possível hidratar os work items vinculados ao PR #$PR_ID."
    exit 1
  fi

  selected_id=$(printf '%s' "$ranked_ids" | awk -F'|' '$2 != "Test Case" {print $1}' | sort -n | head -1)
  if [[ -z "$selected_id" ]]; then
    debug_log "Todos os work items vinculados ao PR #$PR_ID são do tipo Test Case; usando o menor ID como fallback."
    selected_id=$(printf '%s' "$ranked_ids" | awk -F'|' '{print $1}' | sort -n | head -1)
  fi

  WORK_ITEM_ID="$selected_id"
}

fetch_work_item_details() {
  local response body code
  response=$(azure_get "https://dev.azure.com/$AZURE_ORG_ENC/$AZURE_PROJECT_ENC/_apis/wit/workitems/$WORK_ITEM_ID?api-version=7.0")
  code=$(response_code "$response")
  body=$(response_body "$response")
  if [[ "$code" != "200" ]]; then
    log_error "Falha ao buscar detalhes do work item #$WORK_ITEM_ID (HTTP $code)."
    debug_log "$body"
    exit 1
  fi

  WORK_ITEM_TITLE=$(printf '%s' "$body" | jq -r '.fields["System.Title"] // empty')
  WORK_ITEM_TYPE=$(printf '%s' "$body" | jq -r '.fields["System.WorkItemType"] // empty')
  WORK_ITEM_DESCRIPTION=$(printf '%s' "$body" | jq -r '.fields["System.Description"] // empty')
  WORK_ITEM_AREA_PATH=$(printf '%s' "$body" | jq -r '.fields["System.AreaPath"] // empty')
  WORK_ITEM_ITERATION_PATH=$(printf '%s' "$body" | jq -r '.fields["System.IterationPath"] // empty')
  WORK_ITEM_PRIORITY=$(printf '%s' "$body" | jq -r '.fields["Microsoft.VSTS.Common.Priority"] // empty')
}

resolve_work_item() {
  log_info "Resolvendo work item pai..."
  if [[ -n "$CLI_WORK_ITEM_ID" ]]; then
    WORK_ITEM_ID="$CLI_WORK_ITEM_ID"
  else
    fetch_pr_linked_workitems
  fi

  if [[ -z "$WORK_ITEM_ID" ]]; then
    log_error "Não foi possível resolver o work item pai. Use --work-item explicitamente."
    exit 1
  fi

  fetch_work_item_details
}

fetch_pr_changes() {
  log_info "Buscando alterações do PR..."
  local response code body iter_id changes_response changes_body changes_code
  response=$(azure_get "https://dev.azure.com/$AZURE_ORG_ENC/$AZURE_PROJECT_ENC/_apis/git/repositories/$AZURE_REPO_ENC/pullRequests/$PR_ID/iterations?api-version=7.0")
  code=$(response_code "$response")
  body=$(response_body "$response")
  if [[ "$code" != "200" ]]; then
    log_warn "Não foi possível buscar iterações do PR #$PR_ID; o prompt será gerado com contexto reduzido."
    return
  fi

  iter_id=$(printf '%s' "$body" | jq -r '.value | map(.id) | max // empty')
  if [[ -z "$iter_id" ]]; then
    log_warn "PR #$PR_ID sem iteração identificável; seguindo sem resumo de changes."
    return
  fi

  changes_response=$(azure_get "https://dev.azure.com/$AZURE_ORG_ENC/$AZURE_PROJECT_ENC/_apis/git/repositories/$AZURE_REPO_ENC/pullRequests/$PR_ID/iterations/$iter_id/changes?api-version=7.0&\$top=2000")
  changes_code=$(response_code "$changes_response")
  changes_body=$(response_body "$changes_response")
  if [[ "$changes_code" != "200" ]]; then
    log_warn "Não foi possível buscar changes do PR #$PR_ID; seguindo com contexto reduzido."
    return
  fi

  CHANGED_FILES_SUMMARY=$(printf '%s' "$changes_body" | jq -r --argjson max "$MAX_CHANGED_FILES" '
    .changeEntries
    | (if length > $max then .[:$max] else . end)
    | map("- [" + (.changeType // "change") + "] " + (.item.path // "(sem-path)"))
    | join("\n")')

  local total_count shown_count
  total_count=$(printf '%s' "$changes_body" | jq -r '.changeEntries | length')
  shown_count=$(printf '%s' "$changes_body" | jq -r --argjson max "$MAX_CHANGED_FILES" '(.changeEntries | if length > $max then $max else length end)')

  DIFF_SUMMARY="$CHANGED_FILES_SUMMARY"
  if (( total_count > shown_count )); then
    DIFF_SUMMARY+=$'\n\n'
    DIFF_SUMMARY+="[changes truncados, mostrando ${shown_count} de ${total_count} arquivos]"
  fi
  if (( ${#DIFF_SUMMARY} > MAX_DIFF_CHARS )); then
    DIFF_SUMMARY="${DIFF_SUMMARY:0:MAX_DIFF_CHARS}"$'\n\n[resumo truncado por limite de caracteres]'
  fi

  if [[ -n "$PR_SOURCE_REF" && -n "$PR_TARGET_REF" ]]; then
    local source_short target_short encoded_source encoded_target diff_response diff_body diff_code azure_diff_lines
    source_short="${PR_SOURCE_REF#refs/heads/}"
    target_short="${PR_TARGET_REF#refs/heads/}"
    encoded_source=$(uri_encode "$source_short")
    encoded_target=$(uri_encode "$target_short")
    diff_response=$(azure_get "https://dev.azure.com/$AZURE_ORG_ENC/$AZURE_PROJECT_ENC/_apis/git/repositories/$AZURE_REPO_ENC/diffs/commits?baseVersion=$encoded_target&baseVersionType=branch&targetVersion=$encoded_source&targetVersionType=branch&api-version=7.0")
    diff_code=$(response_code "$diff_response")
    diff_body=$(response_body "$diff_response")
    if [[ "$diff_code" == "200" ]]; then
      azure_diff_lines=$(printf '%s' "$diff_body" | jq -r --argjson max "$MAX_CHANGED_FILES" '
        .changes
        | (if length > $max then .[:$max] else . end)
        | map("- [" + (.changeType // "change") + "] " + (.item.path // "(sem-path)"))
        | join("\n")')
      if [[ -n "$azure_diff_lines" ]]; then
        DIFF_SUMMARY+=$'\n\n## Diff summary via Azure DevOps API\n'
        DIFF_SUMMARY+="$azure_diff_lines"
      fi
    fi

    local patch_lines patch_chars current_lines tmp_old tmp_new file_path change_type diff_chunk old_ok new_ok
    patch_lines=0
    patch_chars=0
    DIFF_SUMMARY+=$'\n\n## Patch content via Azure DevOps API\n'
    while IFS=$'\t' read -r file_path change_type; do
      [[ -n "$file_path" ]] || continue
      (( patch_lines >= MAX_DIFF_LINES || patch_chars >= MAX_DIFF_CHARS )) && break

      tmp_old=$(mktemp)
      tmp_new=$(mktemp)
      : > "$tmp_old"
      : > "$tmp_new"
      old_ok=true
      new_ok=true

      if [[ "$change_type" != "add" ]]; then
        if ! fetch_item_content_to_file "$target_short" "$file_path" "$tmp_old"; then
          old_ok=false
        fi
      fi
      if [[ "$change_type" != "delete" ]]; then
        if ! fetch_item_content_to_file "$source_short" "$file_path" "$tmp_new"; then
          new_ok=false
        fi
      fi

      if [[ "$change_type" == "edit" || "$change_type" == "rename" ]]; then
        if [[ "$old_ok" != "true" || "$new_ok" != "true" ]]; then
          rm -f "$tmp_old" "$tmp_new"
          debug_log "Pulando patch de $file_path por falha ao buscar uma das versões no Azure DevOps."
          continue
        fi
      elif [[ "$change_type" == "add" && "$new_ok" != "true" ]]; then
        rm -f "$tmp_old" "$tmp_new"
        debug_log "Pulando patch de $file_path por falha ao buscar a versão nova no Azure DevOps."
        continue
      elif [[ "$change_type" == "delete" && "$old_ok" != "true" ]]; then
        rm -f "$tmp_old" "$tmp_new"
        debug_log "Pulando patch de $file_path por falha ao buscar a versão removida no Azure DevOps."
        continue
      fi

      if file_has_nul_byte "$tmp_old" || file_has_nul_byte "$tmp_new"; then
        rm -f "$tmp_old" "$tmp_new"
        continue
      fi

      diff_chunk=$(diff -u --label "a${file_path}" --label "b${file_path}" "$tmp_old" "$tmp_new" 2>/dev/null || true)
      rm -f "$tmp_old" "$tmp_new"

      [[ -n "$diff_chunk" ]] || continue

      current_lines=$(printf '%s\n' "$diff_chunk" | wc -l | tr -d '[:space:]')
      patch_lines=$((patch_lines + current_lines))
      patch_chars=$((patch_chars + ${#diff_chunk}))
      DIFF_SUMMARY+="$diff_chunk"$'\n'
    done < <(printf '%s' "$changes_body" | jq -r --argjson max "$MAX_CHANGED_FILES" '.changeEntries | (if length > $max then .[:$max] else . end) | .[] | [(.item.path // ""), (.changeType // "change")] | @tsv')

    if (( patch_lines >= MAX_DIFF_LINES || patch_chars >= MAX_DIFF_CHARS )); then
      DIFF_SUMMARY+=$'\n[patch truncado pelos limites configurados]'
    fi
  fi

  if [[ "$IN_GIT_REPO" == "true" && -n "$BRANCH_NAME" && "$PR_SOURCE_REF" == "refs/heads/$BRANCH_NAME" ]]; then
    local target_short diff_text target_ref
    target_short="${PR_TARGET_REF#refs/heads/}"
    target_ref="$target_short"
    if git rev-parse --verify "origin/$target_short" &>/dev/null; then
      target_ref="origin/$target_short"
    fi

    if diff_text=$(git diff "$target_ref...HEAD" 2>/dev/null); then
      if [[ -n "$diff_text" ]]; then
        local diff_lines
        diff_lines=$(printf '%s\n' "$diff_text" | wc -l | tr -d '[:space:]')
        DIFF_SUMMARY+=$'\n\n## Patch local (truncado se necessário)\n'
        if (( diff_lines > MAX_DIFF_LINES )); then
          diff_text=$(printf '%s\n' "$diff_text" | sed -n "1,${MAX_DIFF_LINES}p")
          diff_text+=$'\n\n[patch truncado por limite de linhas]'
        fi
        if (( ${#diff_text} > MAX_DIFF_CHARS )); then
          diff_text="${diff_text:0:MAX_DIFF_CHARS}"$'\n\n[patch truncado por limite de caracteres]'
        fi
        DIFF_SUMMARY+="$diff_text"
      fi
    fi
  fi

  if (( ${#DIFF_SUMMARY} > MAX_DIFF_CHARS )); then
    DIFF_SUMMARY="${DIFF_SUMMARY:0:MAX_DIFF_CHARS}"$'\n\n[diff final truncado por limite de caracteres]'
  fi
}

fetch_example_test_cases() {
  log_info "Buscando exemplos de Test Case..."
  if (( EXAMPLE_COUNT == 0 )); then
    EXAMPLES_SUMMARY="(exemplos desabilitados pelo usuário)"
    return
  fi

  local wiql payload response code body ids lines ranked
  wiql=$(cat <<EOF
Select [System.Id]
From WorkItems
Where [System.TeamProject] = '$AZURE_PROJECT'
  And [System.WorkItemType] = 'Test Case'
Order By [System.ChangedDate] Desc
EOF
)
  payload=$(jq -n --arg wiql "$wiql" '{query:$wiql}')
  response=$(azure_post_json "https://dev.azure.com/$AZURE_ORG_ENC/$AZURE_PROJECT_ENC/_apis/wit/wiql?api-version=7.0" "$payload")
  code=$(response_code "$response")
  body=$(response_body "$response")
  if [[ "$code" != "200" ]]; then
    log_warn "Não foi possível buscar exemplos de Test Case; seguindo sem exemplos."
    EXAMPLES_SUMMARY="(não foi possível buscar exemplos)"
    return
  fi

  ids=$(printf '%s' "$body" | jq -r '.workItems[:15][]?.id')
  if [[ -z "$ids" ]]; then
    EXAMPLES_SUMMARY="(nenhum exemplo de Test Case encontrado)"
    return
  fi

  lines=""
  while IFS= read -r id; do
    [[ -n "$id" ]] || continue
    local wi_resp wi_body wi_code title state area changed_date has_desc has_steps score area_score title_score
    wi_resp=$(azure_get "https://dev.azure.com/$AZURE_ORG_ENC/$AZURE_PROJECT_ENC/_apis/wit/workitems/$id?api-version=7.0")
    wi_code=$(response_code "$wi_resp")
    wi_body=$(response_body "$wi_resp")
    [[ "$wi_code" == "200" ]] || continue
    title=$(printf '%s' "$wi_body" | jq -r '.fields["System.Title"] // empty')
    state=$(printf '%s' "$wi_body" | jq -r '.fields["System.State"] // empty')
    area=$(printf '%s' "$wi_body" | jq -r '.fields["System.AreaPath"] // empty')
    changed_date=$(printf '%s' "$wi_body" | jq -r '.fields["System.ChangedDate"] // empty')
    has_desc=$(printf '%s' "$wi_body" | jq -r 'if .fields["System.Description"] then "sim" else "não" end')
    has_steps=$(printf '%s' "$wi_body" | jq -r 'if .fields["Microsoft.VSTS.TCM.Steps"] then "sim" else "não" end')
    area_score=1
    title_score=0
    [[ "$area" == "$WORK_ITEM_AREA_PATH" ]] && area_score=0
    if [[ "$title" == *"Teste |"* || "$title" == *"Teste"* || "$title" == *"Validar"* || "$title" == *"Verificar"* ]]; then
      title_score=0
    else
      title_score=1
    fi
    score="${area_score}${title_score}"
    lines+="$score"$'\t'"$changed_date"$'\t'"$id"$'\t'"$title"$'\t'"$state"$'\t'"$area"$'\t'"$has_desc"$'\t'"$has_steps"$'\n'
  done <<< "$ids"

  ranked=$(printf '%s' "$lines" | sort -t$'\t' -k1,1 -k2,2r -k3,3n | head -n "$EXAMPLE_COUNT")
  EXAMPLES_SUMMARY=$(printf '%s' "$ranked" | awk -F'\t' '{printf "- #%s %s (estado: %s, area: %s, descrição: %s, passos: %s)\n", $3, $4, $5, $6, $7, $8}')
  [[ -n "$EXAMPLES_SUMMARY" ]] || EXAMPLES_SUMMARY="(nenhum exemplo resumido)"
}

resolve_creation_defaults() {
  log_info "Resolvendo defaults de criação do Test Case..."
  SELECTED_ASSIGNED_TO="$CLI_ASSIGNED_TO"
  if [[ -z "$SELECTED_ASSIGNED_TO" ]]; then
    SELECTED_ASSIGNED_TO="${TEST_CARD_ASSIGNED_TO:-}"
  fi
  if [[ -z "$SELECTED_ASSIGNED_TO" && "$NO_CREATE" == "false" ]]; then
    log_warn "Nenhum responsável padrão configurado para o Test Case. A criação será tentada sem atribuição."
  fi

  SELECTED_AREA_PATH="$CLI_AREA_PATH"
  if [[ -z "$SELECTED_AREA_PATH" ]]; then
    SELECTED_AREA_PATH="${TEST_CARD_AREA_PATH:-}"
  fi
  if [[ -z "$SELECTED_AREA_PATH" && "$AZURE_PROJECT" == "AGROTRACE" ]]; then
    SELECTED_AREA_PATH="$DEFAULT_AGROTRACE_AREA_PATH"
  fi

  if [[ -z "$SELECTED_AREA_PATH" ]]; then
    log_error "Não foi possível resolver o AreaPath do Test Case. Use --area-path ou configure TEST_CARD_AREA_PATH."
    exit 1
  fi

  ATTEMPTED_PRIORITY=""
  ATTEMPTED_TEAM=""
  ATTEMPTED_PROGRAMA=""
  if [[ "$AZURE_PROJECT" == "AGROTRACE" ]]; then
    ATTEMPTED_PRIORITY="$DEFAULT_AGROTRACE_PRIORITY"
    ATTEMPTED_TEAM="$DEFAULT_AGROTRACE_TEAM"
    ATTEMPTED_PROGRAMA="$DEFAULT_AGROTRACE_PROGRAMA"
  fi
}

collect_test_qa_required_fields() {
  local wi_response wi_body wi_code current_effort current_real_effort
  wi_response=$(azure_get "https://dev.azure.com/$AZURE_ORG_ENC/$AZURE_PROJECT_ENC/_apis/wit/workitems/$WORK_ITEM_ID?api-version=7.0")
  wi_code=$(response_code "$wi_response")
  wi_body=$(response_body "$wi_response")

  current_effort=""
  current_real_effort=""
  if [[ "$wi_code" == "200" ]]; then
    current_effort=$(printf '%s' "$wi_body" | jq -r '.fields["Microsoft.VSTS.Scheduling.Effort"] // empty')
    current_real_effort=$(printf '%s' "$wi_body" | jq -r '.fields["Custom.RealEffort"] // empty')
  fi

  TEST_QA_EFFORT=""
  if [[ -z "$current_effort" ]]; then
    echo -en "  Effort (horas decimais, ex: 0.5) [${CYAN}0.5${NC}]: " >&2
    local input_effort
    read -r input_effort
    TEST_QA_EFFORT="${input_effort:-0.5}"
  else
    TEST_QA_EFFORT="$current_effort"
  fi

  TEST_QA_REAL_EFFORT=""
  if [[ -z "$current_real_effort" ]]; then
    local default_real="${TEST_QA_EFFORT:-0.5}"
    echo -en "  Real Effort (horas decimais) [${CYAN}${default_real}${NC}]: " >&2
    local input_real_effort
    read -r input_real_effort
    TEST_QA_REAL_EFFORT="${input_real_effort:-$default_real}"
  else
    TEST_QA_REAL_EFFORT="$current_real_effort"
  fi
}

update_parent_work_item_to_test_qa() {
  local payload_tmp response code body
  log_info "Atualizando work item #$WORK_ITEM_ID para Test QA..."

  collect_test_qa_required_fields

  payload_tmp=$(mktemp)

  jq -n --arg effort "$TEST_QA_EFFORT" --arg real_effort "$TEST_QA_REAL_EFFORT" '
    [{op:"add", path:"/fields/System.State", value:"Test QA"}]
    + (if $effort != "" then [{op:"add", path:"/fields/Microsoft.VSTS.Scheduling.Effort", value:($effort|tonumber)}] else [] end)
    + (if $real_effort != "" then [{op:"add", path:"/fields/Custom.RealEffort", value:($real_effort|tonumber)}] else [] end)
  ' > "$payload_tmp"

  response=$(azure_patch_json "https://dev.azure.com/$AZURE_ORG_ENC/$AZURE_PROJECT_ENC/_apis/wit/workitems/$WORK_ITEM_ID?api-version=7.0" "$payload_tmp")
  rm -f "$payload_tmp"

  code=$(response_code "$response")
  body=$(response_body "$response")
  if [[ "$code" != "200" && "$code" != "201" ]]; then
    local error_msg
    error_msg=$(extract_azure_error_message "$body")
    [[ -n "$error_msg" ]] || error_msg="Erro desconhecido ao atualizar work item."
    log_warn "Não foi possível atualizar o work item #$WORK_ITEM_ID para Test QA."
    if [[ -n "$error_msg" ]]; then
      if [[ "$RAW_OUTPUT" == "true" ]]; then
        echo "Falha ao atualizar work item para Test QA: $error_msg" >&2
      else
        echo "$error_msg"
      fi
    fi
    return 1
  fi

  UPDATED_PARENT_TO_TEST_QA=true
  log_success "Work item #$WORK_ITEM_ID atualizado para Test QA."
  return 0
}

create_test_case() {
  local payload_tmp response code body parent_url
  log_info "Criando Test Case no Azure DevOps..."
  payload_tmp=$(mktemp)
  parent_url="https://dev.azure.com/$AZURE_ORG_ENC/$AZURE_PROJECT_ENC/_apis/wit/workItems/$WORK_ITEM_ID"

  jq -n \
    --arg title "$GENERATED_TITLE" \
    --arg desc "$GENERATED_HTML" \
    --arg steps "$GENERATED_STEPS" \
    --arg area "$SELECTED_AREA_PATH" \
    --arg iteration "$WORK_ITEM_ITERATION_PATH" \
    --arg priority "$ATTEMPTED_PRIORITY" \
    --arg team "$ATTEMPTED_TEAM" \
    --arg programa "$ATTEMPTED_PROGRAMA" \
    --arg assigned "$SELECTED_ASSIGNED_TO" \
    --arg parent "$parent_url" '
      [
        {op:"add", path:"/fields/System.Title", value:$title},
        {op:"add", path:"/fields/System.Description", value:$desc},
        {op:"add", path:"/fields/Microsoft.VSTS.TCM.Steps", value:$steps},
        {op:"add", path:"/fields/System.AreaPath", value:$area},
        {op:"add", path:"/relations/-", value:{rel:"System.LinkTypes.Hierarchy-Reverse", url:$parent}}
      ]
      + (if $iteration != "" then [{op:"add", path:"/fields/System.IterationPath", value:$iteration}] else [] end)
      + (if $priority != "" then [{op:"add", path:"/fields/Microsoft.VSTS.Common.Priority", value:($priority|tonumber)}] else [] end)
      + (if $team != "" then [{op:"add", path:"/fields/Custom.Team", value:$team}] else [] end)
      + (if $programa != "" then [{op:"add", path:"/fields/Custom.ProgramasAgrotrace", value:$programa}] else [] end)
      + (if $assigned != "" then [{op:"add", path:"/fields/System.AssignedTo", value:$assigned}] else [] end)
    ' > "$payload_tmp"

  if [[ "$DEBUG_MODE" == "true" ]]; then
    debug_log "Payload de criação do Test Case:"
    cat "$payload_tmp" >&2
  fi

  response=$(azure_patch_json "https://dev.azure.com/$AZURE_ORG_ENC/$AZURE_PROJECT_ENC/_apis/wit/workitems/\$Test%20Case?api-version=7.0" "$payload_tmp")
  rm -f "$payload_tmp"

  code=$(response_code "$response")
  body=$(response_body "$response")
  if [[ "$code" != "200" && "$code" != "201" ]]; then
    CREATE_ERROR=$(extract_azure_error_message "$body")
    [[ -n "$CREATE_ERROR" ]] || CREATE_ERROR="Erro desconhecido na criação do Test Case."
    return 1
  fi

  CREATED_TEST_CASE_ID=$(printf '%s' "$body" | jq -r '.id // empty')
  CREATED_TEST_CASE_URL="https://dev.azure.com/$AZURE_ORG_ENC/$AZURE_PROJECT_ENC/_workitems/edit/$CREATED_TEST_CASE_ID"
  return 0
}
