# Groq Reasoning Fallback Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Groq retries automatic when `reasoning_format` is rejected for a model, by retrying the same request once without that field.

**Architecture:** Keep the change inside `bin/create-pr-description` with a minimal refactor: split OpenAI-compatible payload creation from HTTP execution, add a Groq-specific error classifier, and preserve the existing provider fallback flow. Verification stays Bash-first with syntax checks, CLI smoke checks, and local fixture-based validation of the retry predicate.

**Tech Stack:** Bash, curl, jq, git

**Spec:** `docs/superpowers/specs/2026-03-26-groq-reasoning-fallback-design.md`

---

## File Structure

| File                                                                  | Responsibility                                                                                                                     |
| --------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| `bin/create-pr-description`                                           | Build Groq/OpenAI-compatible payloads, execute HTTP calls, classify Groq retryable errors, and preserve provider fallback behavior |
| `docs/superpowers/specs/2026-03-26-groq-reasoning-fallback-design.md` | Approved design reference for retry predicate, logging rules, and verification scope                                               |
| `docs/superpowers/plans/2026-03-26-groq-reasoning-fallback.md`        | This implementation plan                                                                                                           |

This change should stay in one runtime file. Do not add a new framework, test runner, or permanent helper script just for this fallback.

---

## Chunk 1: Refactor request helpers and add Groq retry predicate

### Task 1: Extract payload builder for OpenAI-compatible providers

**Files:**

- Modify: `bin/create-pr-description`

- [ ] **Step 1: Add a payload builder helper near the LLM provider section**

Create a helper dedicated to building the JSON payload for OpenAI-compatible providers. It must accept whether `reasoning_format` should be included.

```bash
build_openai_payload() {
  local model="$1"
  local system_prompt="$2"
  local user_prompt="$3"
  local include_reasoning_format="$4"

  local payload_tmp system_tmp user_tmp
  payload_tmp=$(mktemp)
  system_tmp=$(mktemp)
  user_tmp=$(mktemp)

  printf '%s' "$system_prompt" > "$system_tmp"
  printf '%s' "$user_prompt" > "$user_tmp"

  local jq_filter='{
    model: $model,
    messages: [
      { role: "system", content: $system },
      { role: "user", content: $user }
    ],
    temperature: 0.3
  }'

  if [[ "$include_reasoning_format" == "true" ]]; then
    jq_filter='{
      model: $model,
      messages: [
        { role: "system", content: $system },
        { role: "user", content: $user }
      ],
      temperature: 0.3,
      reasoning_format: "hidden"
    }'
  fi

  jq -n \
    --arg model "$model" \
    --rawfile system "$system_tmp" \
    --rawfile user "$user_tmp" \
    "$jq_filter" > "$payload_tmp"

  rm -f "$system_tmp" "$user_tmp"
  printf '%s\n' "$payload_tmp"
}
```

- [ ] **Step 2: Replace the inline payload construction inside `call_llm_api`**

Update `call_llm_api` to call `build_openai_payload` instead of assembling provider JSON inline. For `groq`, the first call must still include `reasoning_format`; for other OpenAI-compatible providers, keep the current payload behavior.

- [ ] **Step 3: Verify syntax after the refactor**

Run: `bash -n bin/create-pr-description`
Expected: no output.

- [ ] **Step 4: Commit**

```bash
git status --short -- bin/create-pr-description
git diff --staged
git diff -- bin/create-pr-description
git diff --staged -- bin/create-pr-description
git commit -m "refactor: extract openai payload builder for provider calls"
```

Expected:

- if the index already contains unrelated staged changes, do not create this commit yet
- if `bin/create-pr-description` already contains unrelated user edits, skip this per-task commit and continue implementation without committing yet
- only create this commit when the file is isolated enough that the staged diff contains only this task's intended changes

---

### Task 2: Extract HTTP executor that returns status and body

**Files:**

- Modify: `bin/create-pr-description`

- [ ] **Step 1: Add a helper that executes the HTTP call without discarding the body on non-200**

Create a helper that receives the existing OpenAI-compatible request inputs and prints a structured result containing `http_code` and `body`. Keep URL, headers, timeout, method, and all curl arguments identical across attempts.

```bash
execute_openai_http_request() {
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
```

- [ ] **Step 2: Update `call_llm_api` to read `http_code` and `body` from this helper**

Preserve the existing handling for timeout, rate limit, and generic provider failures, but restructure it so the Groq-specific classifier can inspect the original `400` body before the generic warning path runs.

- [ ] **Step 3: Add a safe Bash entrypoint guard if the script does not already have one**

Allow the script to be sourced for helper-level verification without running the full CLI startup.

```bash
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi
```

Keep runtime behavior unchanged when the script is executed normally.

- [ ] **Step 4: Re-run syntax check**

Run: `bash -n bin/create-pr-description`
Expected: no output.

- [ ] **Step 5: Commit**

```bash
git status --short -- bin/create-pr-description
git diff --staged
git diff -- bin/create-pr-description
git diff --staged -- bin/create-pr-description
git commit -m "refactor: separate provider HTTP execution from response handling"
```

Expected:

- if the index already contains unrelated staged changes, do not create this commit yet
- if `bin/create-pr-description` already contains unrelated user edits, skip this per-task commit and continue implementation without committing yet
- only create this commit when the file is isolated enough that the staged diff contains only this task's intended changes

---

### Task 3: Add the conservative Groq retry classifier

**Files:**

- Modify: `bin/create-pr-description`

- [ ] **Step 1: Add a helper that returns success only for the approved Groq retry case**

Implement a helper that receives `provider_name`, `http_code`, and `body`, and returns zero only when all approved conditions are true:

- `provider_name == "groq"`
- `http_code == "400"`
- `error.param == "reasoning_format"`
- lowercased `error.message` contains both `reasoning_format` and `not supported`
- malformed JSON or missing fields return failure

```bash
is_groq_reasoning_format_retryable_error() {
  local provider_name="$1"
  local http_code="$2"
  local body="$3"

  [[ "$provider_name" == "groq" ]] || return 1
  [[ "$http_code" == "400" ]] || return 1

  local error_param error_message error_message_lc
  error_param=$(printf '%s' "$body" | jq -r '.error.param // empty' 2>/dev/null || true)
  error_message=$(printf '%s' "$body" | jq -r '.error.message // empty' 2>/dev/null || true)

  [[ -n "$error_param" && -n "$error_message" ]] || return 1
  [[ "$error_param" == "reasoning_format" ]] || return 1

  error_message_lc=$(printf '%s' "$error_message" | tr '[:upper:]' '[:lower:]')
  [[ "$error_message_lc" == *"reasoning_format"* ]] || return 1
  [[ "$error_message_lc" == *"not supported"* ]] || return 1

  return 0
}
```

- [ ] **Step 2: Add a small extraction helper for successful completion content if it improves clarity**

If needed, add a helper for parsing `.choices[0].message.content // empty` so `call_llm_api` stays readable after the retry path is inserted.

- [ ] **Step 3: Verify syntax**

Run: `bash -n bin/create-pr-description`
Expected: no output.

- [ ] **Step 4: Commit**

```bash
git status --short -- bin/create-pr-description
git diff --staged
git diff -- bin/create-pr-description
git diff --staged -- bin/create-pr-description
git commit -m "feat: add groq reasoning-format retry classifier"
```

Expected:

- if the index already contains unrelated staged changes, do not create this commit yet
- if `bin/create-pr-description` already contains unrelated user edits, skip this per-task commit and continue implementation without committing yet
- only create this commit when the file is isolated enough that the staged diff contains only this task's intended changes

---

## Chunk 2: Implement retry flow, deterministic logs, and local verification

### Task 4: Add the single retry path inside `call_llm_api`

**Files:**

- Modify: `bin/create-pr-description`

- [ ] **Step 1: Update `call_llm_api` to attempt Groq once with `reasoning_format` and retry once without it**

Implement the flow exactly as approved in the spec:

- first Groq attempt includes `reasoning_format: "hidden"`
- if the classifier says the response is retryable, log one specific warning and rebuild the payload without `reasoning_format`
- retry exactly once using the same URL, headers, timeout, method, and other payload fields
- if the retry succeeds, continue normally
- if the retry fails, log only the final provider failure from the second response
- if the first response is not retryable, keep the existing generic provider failure path

Suggested shape:

```bash
local include_reasoning_format=false
if [[ "$provider_name" == "groq" ]]; then
  include_reasoning_format=true
fi

local payload_tmp response http_code body
payload_tmp=$(build_openai_payload "$model" "$system_prompt" "$user_prompt" "$include_reasoning_format")
response=$(execute_openai_http_request "$url" "$key" "$provider_name" "$payload_tmp")
rm -f "$payload_tmp"

http_code=$(printf '%s' "$response" | tail -1)
body=$(printf '%s' "$response" | sed '$d')

if is_groq_reasoning_format_retryable_error "$provider_name" "$http_code" "$body"; then
  log_warn "Groq rejeitou reasoning_format para este modelo; tentando novamente sem esse parametro"

  payload_tmp=$(build_openai_payload "$model" "$system_prompt" "$user_prompt" "false")
  response=$(execute_openai_http_request "$url" "$key" "$provider_name" "$payload_tmp")
  rm -f "$payload_tmp"

  http_code=$(printf '%s' "$response" | tail -1)
  body=$(printf '%s' "$response" | sed '$d')
fi
```

- [ ] **Step 2: Preserve the timeout and rate-limit branches exactly**

Do not let the Groq retry path alter the current behavior for:

- `000` timeout
- `429` rate limit
- other provider errors

The retry path is only for the approved Groq `400` case.

- [ ] **Step 3: Re-run syntax and smoke checks**

Run: `bash -n bin/create-pr-description && bin/create-pr-description --help`
Expected: no syntax output, then help text.

- [ ] **Step 4: Commit**

```bash
git status --short -- bin/create-pr-description
git diff --staged
git diff -- bin/create-pr-description
git diff --staged -- bin/create-pr-description
git commit -m "fix: retry groq requests without reasoning_format when unsupported"
```

Expected:

- if the index already contains unrelated staged changes, do not create this commit yet
- if `bin/create-pr-description` already contains unrelated user edits, skip this per-task commit and continue implementation without committing yet
- only create this commit when the file is isolated enough that the staged diff contains only this task's intended changes

---

### Task 5: Validate the retry predicate locally with inline fixtures

**Files:**

- Modify: `bin/create-pr-description` (only if a tiny helper extraction is still needed)

- [ ] **Step 1: Run a local shell validation for the classifier with three fixtures**

Use a one-off shell command from the repo root that sources the script helpers through the guarded entrypoint and validates:

- target Groq error returns success
- different Groq error returns failure
- invalid JSON returns failure

Example verification command:

```bash
bash -lc '
source ./bin/create-pr-description

target_body=$(cat <<"EOF"
{"error":{"message":"reasoning_format is not supported with this model","type":"invalid_request_error","param":"reasoning_format"}}
EOF
)

other_body=$(cat <<"EOF"
{"error":{"message":"model not found","type":"invalid_request_error","param":"model"}}
EOF
)

invalid_body="not-json"

is_groq_reasoning_format_retryable_error groq 400 "$target_body"
is_groq_reasoning_format_retryable_error groq 400 "$other_body" && exit 1 || true
is_groq_reasoning_format_retryable_error groq 400 "$invalid_body" && exit 1 || true
'
```

Expected: command exits with status `0` and no unexpected output.

- [ ] **Step 2: Run a local shell validation for the retry flow with stubbed request execution**

Use a one-off shell command that sources the guarded script, temporarily overrides `execute_openai_http_request`, and writes observations to temp files so the proof survives command substitution inside `call_llm_api`:

- target Groq error causes exactly one retry
- non-target Groq error does not retry
- invalid body does not retry
- URL, key, and provider identity stay the same across both attempts
- only the payload body changes, removing `reasoning_format` on the second attempt

The helper-level proof for curl flags already lives in Task 2, where `execute_openai_http_request` is extracted and kept identical between attempts. This retry-flow proof focuses on call count and payload invariants at the `call_llm_api` level.

Example verification shape:

```bash
bash -lc '
source ./bin/create-pr-description

state_dir=$(mktemp -d)

execute_openai_http_request() {
  local url="$1"
  local key="$2"
  local provider_name="$3"
  local payload_file="$4"
  local calls_file="$state_dir/calls"
  local call_number=1

  if [[ -f "$calls_file" ]]; then
    call_number=$(( $(cat "$calls_file") + 1 ))
  fi

  printf "%s" "$call_number" > "$calls_file"
  printf "%s" "$url|$key|$provider_name" > "$state_dir/signature_$call_number"
  cat "$payload_file" > "$state_dir/payload_$call_number.json"

  if [[ "$TEST_CASE" == "target" && "$call_number" -eq 1 ]]; then
    printf "%s\n400" "{\"error\":{\"message\":\"reasoning_format is not supported with this model\",\"type\":\"invalid_request_error\",\"param\":\"reasoning_format\"}}"
    return 0
  fi

  if [[ "$TEST_CASE" == "target" && "$call_number" -eq 2 ]]; then
    printf "%s\n200" "{\"choices\":[{\"message\":{\"content\":\"TITULO: teste\n\n## Descrição\n\nok\"}}]}"
    return 0
  fi

  if [[ "$TEST_CASE" == "other" ]]; then
    printf "%s\n400" "{\"error\":{\"message\":\"model not found\",\"type\":\"invalid_request_error\",\"param\":\"model\"}}"
    return 0
  fi

  printf "%s\n400" "not-json"
}

TEST_CASE=target
result_file=$(mktemp)
call_llm_api https://example.invalid key model system user groq > "$result_file"
result=$(cat "$result_file")
rm -f "$result_file"
[[ "$(cat "$state_dir/calls")" -eq 2 ]]
[[ "$result" == *"TITULO: teste"* ]]
[[ "$(cat "$state_dir/signature_1")" == "$(cat "$state_dir/signature_2")" ]]
[[ "$(cat "$state_dir/payload_1.json")" == *'"reasoning_format":"hidden"'* ]]
[[ "$(cat "$state_dir/payload_2.json")" != *'"reasoning_format":"hidden"'* ]]
payload_one_no_reasoning=$(jq -c 'del(.reasoning_format)' "$state_dir/payload_1.json")
payload_two_compact=$(jq -c '.' "$state_dir/payload_2.json")
[[ "$payload_one_no_reasoning" == "$payload_two_compact" ]]

rm -f "$state_dir"/calls "$state_dir"/signature_* "$state_dir"/payload_*.json
TEST_CASE=other
call_llm_api https://example.invalid key model system user groq >/dev/null 2>&1 || true
[[ "$(cat "$state_dir/calls")" -eq 1 ]]

rm -f "$state_dir"/calls "$state_dir"/signature_* "$state_dir"/payload_*.json
TEST_CASE=invalid
call_llm_api https://example.invalid key model system user groq >/dev/null 2>&1 || true
[[ "$(cat "$state_dir/calls")" -eq 1 ]]

rm -rf "$state_dir"
'
```

Expected: command exits with status `0`; target case retries once, preserves request identity inputs, removes only `reasoning_format`, and other cases do not retry.

- [ ] **Step 3: Run the standard repo smoke checks**

Run: `bash -n bin/create-pr-description && bin/create-pr-description --help && bin/create-pr-description --dry-run`
Expected:

- no syntax errors
- help text renders normally
- `--dry-run` still reaches the existing non-network flow until the current repo/config preconditions stop it

- [ ] **Step 4: If Groq credentials are available, do one manual live validation**

Run the smallest realistic path you can safely execute with a Groq model known to reject `reasoning_format`, and confirm:

- one retry warning appears
- the request succeeds on the second attempt
- PR description output is still parsed normally

If credentials or a suitable model are not available, document that verification was limited to local fixture-based validation and smoke checks.

- [ ] **Step 5: Record the verification results in the implementation handoff notes**

Capture which of the following were actually run:

- local fixture-based helper validation
- syntax and CLI smoke checks
- live Groq validation, if available

---

## Chunk 3: Final verification and handoff

### Task 6: Final repo verification

**Files:**

- Modify: `bin/create-pr-description`

- [ ] **Step 1: Run final verification commands from the repo root**

Run: `bash -n bin/create-pr-description && bash -n install.sh && bin/create-pr-description --help && bin/create-pr-description --dry-run`
Expected:

- no syntax errors in either script
- CLI help works
- `--dry-run` still exercises the safe non-network flow

- [ ] **Step 2: Review the diff for unintended behavior changes**

Run: `git status --short && git log --oneline -n 5 && git diff -- bin/create-pr-description`
Expected:

- recent commits show the intended helper extraction and Groq retry work
- working-tree diff only shows the helper extraction, Groq retry classifier, retry flow, entrypoint guard if added, and any tiny readability refactors required to support them
- working-tree diff does not contain unrelated last-minute edits in `bin/create-pr-description`

- [ ] **Step 3: Create the final implementation commit only if there are remaining staged changes**

```bash
git status --short
```

Expected:

- if the index already contains unrelated staged changes, do not create a final commit yet
- if there are remaining implementation changes, stage only the intended file changes for this task, create one final non-empty commit, and do not include unrelated staged user changes
- if the earlier task commits already cover the work, skip this step and avoid an empty commit

- [ ] **Step 4: Prepare implementation summary for handoff**

Include:

- where the retry predicate lives
- how the retry preserves payload/curl invariants
- which verification commands were actually run
- whether live Groq validation was performed or skipped

---

Plan complete and saved to `docs/superpowers/plans/2026-03-26-groq-reasoning-fallback.md`. Ready to execute?
