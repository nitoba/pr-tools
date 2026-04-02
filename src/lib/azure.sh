#!/usr/bin/env bash
# lib/azure.sh — Azure DevOps integration (remote parsing, repo ID, PR links, PR creation)

[[ -n "${_PR_TOOLS_AZURE_SH:-}" ]] && return 0
_PR_TOOLS_AZURE_SH=1

# Source common.sh relative to this file
_AZURE_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/common.sh
source "${_AZURE_LIB_DIR}/common.sh"

parse_azure_remote() {
  AZURE_ORG=""
  AZURE_PROJECT=""
  AZURE_REPO=""
  IS_AZURE_DEVOPS=false

  local remote_url
  remote_url=$(git remote get-url origin 2>/dev/null || echo "")

  if [[ -z "$remote_url" ]]; then
    log_warn "Remote origin não configurado. Links de PR não serão gerados."
    return
  fi

  # HTTPS: https://dev.azure.com/{org}/{project}/_git/{repo}
  # HTTPS with user: https://{org}@dev.azure.com/{org}/{project}/_git/{repo}
  if [[ "$remote_url" =~ dev\.azure\.com[/:]([^/]+)/([^/]+)/_git/([^/]+) ]]; then
    local matched_org="${BASH_REMATCH[1]}"
    local matched_project="${BASH_REMATCH[2]}"
    AZURE_REPO="${BASH_REMATCH[3]}"

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
    log_warn "Remote não é Azure DevOps. Links de PR não serão gerados."
  fi
}

get_cached_repo_id() {
  local remote_url="$1"
  if [[ -f "$CACHE_FILE" ]]; then
    grep "^${remote_url}=" "$CACHE_FILE" 2>/dev/null | head -1 | cut -d'=' -f2- || echo ""
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
    log_warn "AZURE_PAT não configurado. Links gerados sem repositoryId."
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

resolve_reviewer_id() {
  local email="$1"
  local cached_id=""

  # Check cache: email=id format
  if [[ -f "$CACHE_FILE" ]]; then
    cached_id=$(grep "^reviewer:${email}=" "$CACHE_FILE" 2>/dev/null | head -1 | cut -d'=' -f2- || true)
  fi

  if [[ -n "$cached_id" ]]; then
    echo "$cached_id"
    return 0
  fi

  if [[ -z "${AZURE_PAT:-}" ]]; then
    log_warn "AZURE_PAT necessario para resolver reviewer '$email'"
    echo ""
    return 1
  fi

  # Search for identity via Azure DevOps Identity Picker API
  local response
  response=$(curl -s --max-time 10 \
    -u ":$AZURE_PAT" \
    -H "Content-Type: application/json" \
    -d "{\"query\":\"$email\",\"identityTypes\":[\"user\"],\"operationScopes\":[\"ims\",\"source\"]}" \
    "https://dev.azure.com/$AZURE_ORG/_apis/IdentityPicker/Identities?api-version=7.0-preview.1" \
    2>/dev/null || echo "")

  if [[ -z "$response" ]]; then
    echo ""
    return 1
  fi

  local user_id
  user_id=$(echo "$response" | jq -r '.results[0].identities[0].localId // empty' 2>/dev/null || true)

  if [[ -n "$user_id" && "$user_id" != "null" ]]; then
    # Cache it
    echo "reviewer:${email}=${user_id}" >> "$CACHE_FILE"
    echo "$user_id"
    return 0
  fi

  echo ""
  return 1
}

create_pr_via_api() {
  local target_ref="$1"
  local title="$2"
  local description="$3"
  local reviewer_email="$4"
  local work_item_id="${5:-}"

  if [[ -z "${AZURE_PAT:-}" ]]; then
    log_error "AZURE_PAT necessário para criar PR via API."
    return 1
  fi

  if [[ -z "$AZURE_REPO_ID" ]]; then
    log_error "repositoryId não disponível. Execute novamente com AZURE_PAT configurado."
    return 1
  fi

  # Resolve reviewer
  local reviewers_json="[]"
  if [[ -n "$reviewer_email" ]]; then
    log_info "Resolvendo reviewer: $reviewer_email..."
    local reviewer_id
    reviewer_id=$(resolve_reviewer_id "$reviewer_email")
    if [[ -n "$reviewer_id" ]]; then
      reviewers_json="[{\"id\":\"$reviewer_id\",\"isRequired\":true}]"
      log_success "Reviewer resolvido: ${reviewer_id:0:8}..."
    else
      log_warn "Não foi possível resolver reviewer '$reviewer_email'. PR será criado sem reviewer."
    fi
  fi

  # Work items
  local work_items_json="[]"
  if [[ -n "$work_item_id" ]]; then
    work_items_json="[{\"id\":\"$work_item_id\",\"url\":\"https://dev.azure.com/$AZURE_ORG/$AZURE_PROJECT/_apis/wit/workItems/$work_item_id\"}]"
  fi

  # Build payload using temp files
  local pr_payload_tmp
  pr_payload_tmp=$(mktemp)

  local desc_tmp title_tmp
  desc_tmp=$(mktemp)
  title_tmp=$(mktemp)
  printf '%s' "$description" > "$desc_tmp"
  printf '%s' "$title" > "$title_tmp"

  jq -n \
    --arg source "refs/heads/$BRANCH_NAME" \
    --arg target "refs/heads/$target_ref" \
    --rawfile title "$title_tmp" \
    --rawfile desc "$desc_tmp" \
    --argjson reviewers "$reviewers_json" \
    --argjson workItems "$work_items_json" \
    '{
      sourceRefName: $source,
      targetRefName: $target,
      title: $title,
      description: $desc,
      reviewers: $reviewers,
      workItemRefs: $workItems
    }' > "$pr_payload_tmp"

  rm -f "$desc_tmp" "$title_tmp"

  # Create PR
  local response
  response=$(curl -s -w "\n%{http_code}" \
    --max-time 30 \
    -u ":$AZURE_PAT" \
    -H "Content-Type: application/json" \
    -d @"$pr_payload_tmp" \
    "https://dev.azure.com/$AZURE_ORG/$AZURE_PROJECT/_apis/git/repositories/$AZURE_REPO_ID/pullrequests?api-version=7.0" \
    2>/dev/null || echo -e "\n000")

  rm -f "$pr_payload_tmp"

  local http_code
  http_code=$(echo "$response" | tail -1)
  local body
  body=$(echo "$response" | sed '$d')

  if [[ "$http_code" == "201" || "$http_code" == "200" ]]; then
    local pr_id pr_url
    pr_id=$(echo "$body" | jq -r '.pullRequestId // empty' 2>/dev/null || true)
    pr_url="https://dev.azure.com/$AZURE_ORG/$AZURE_PROJECT/_git/$AZURE_REPO/pullrequest/$pr_id"
    log_success "PR #$pr_id criado para $target_ref"
    echo "$pr_url"
    return 0
  else
    local error_msg
    error_msg=$(echo "$body" | jq -r '.message // empty' 2>/dev/null || true)
    log_error "Falha ao criar PR para $target_ref (HTTP $http_code)"
    if [[ -n "$error_msg" ]]; then
      log_error "Detalhes: $error_msg"
    fi
    echo ""
    return 1
  fi
}

offer_pr_creation() {
  # Only in interactive mode and with Azure DevOps configured
  if [[ ! -t 0 || "$IS_AZURE_DEVOPS" != "true" || -z "${AZURE_PAT:-}" ]]; then
    return
  fi

  ui_title_start "Publicar no Azure DevOps"

  printf '  %b│%b\n' "$_UI_GRAY" "$_UI_NC" >&2
  if ! prompt_yn "Criar PR(s) no Azure DevOps?" "n"; then
    printf '  %b│%b %b(cancelado)%b\n' "$_UI_GRAY" "$_UI_NC" "$_UI_DIM" "$_UI_NC" >&2
    ui_title_done
    return
  fi

  for target in "${TARGETS[@]}"; do
    local target_ref=""
    local default_reviewer=""

    if [[ "$target" == "dev" ]]; then
      target_ref="dev"
      default_reviewer="${PR_REVIEWER_DEV:-}"
    elif [[ "$target" == "sprint" && -n "$SPRINT_BRANCH" ]]; then
      target_ref="$SPRINT_BRANCH"
      default_reviewer="${PR_REVIEWER_SPRINT:-}"
    else
      continue
    fi

    printf '  %b│%b\n' "$_UI_GRAY" "$_UI_NC" >&2
    printf '  %b│%b %b→ PR para %s%b\n' "$_UI_GRAY" "$_UI_NC" "$_UI_BOLD" "$target_ref" "$_UI_NC" >&2

    # Ask/confirm reviewer
    local reviewer=""
    reviewer=$(prompt_value "Reviewer (email)" "$default_reviewer")

    step_start "Criando PR → $target_ref"
    local pr_url
    if pr_url=$(create_pr_via_api "$target_ref" "$PR_TITLE" "$PR_BODY" "$reviewer" "$WORK_ITEM_ID"); then
      step_done "PR criado → $target_ref"
      if [[ -n "$pr_url" ]]; then
        printf '  %b│%b   %b%s%b\n' "$_UI_GRAY" "$_UI_NC" "$_UI_DIM" "$pr_url" "$_UI_NC" >&2
      fi
    else
      step_fail "Falha ao criar PR → $target_ref"
    fi
  done

  ui_title_done
}
