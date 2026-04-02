#!/usr/bin/env bash
# src/lib/ui.sh — spinner progress UI for pr-tools

[[ -n "${_PR_TOOLS_UI_SH:-}" ]] && return 0
_PR_TOOLS_UI_SH=1

# ---- Spinner state ----
_SPINNER_PID=""
_SPINNER_MSG=""
_SPINNER_ACTIVE=false
_SPINNER_INTERACTIVE=true

if [[ ! -t 2 || -n "${NO_COLOR:-}" ]]; then
  _SPINNER_INTERACTIVE=false
fi

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

_spinner_stop() {
  if [[ -n "$_SPINNER_PID" ]]; then
    kill "$_SPINNER_PID" 2>/dev/null
    wait "$_SPINNER_PID" 2>/dev/null
    _SPINNER_PID=""
  fi
  _SPINNER_ACTIVE=false
}

_spinner_clear_line() {
  if [[ "$_SPINNER_INTERACTIVE" == "true" ]]; then
    printf '\r\033[2K' >&2
  fi
}

step_start() {
  local msg="$1"

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

# Override log_info to suppress during spinner
if declare -f log_info >/dev/null 2>&1; then
  eval "$(echo '_original_log_info()'; declare -f log_info | tail -n +2)"
  log_info() {
    if [[ "$_SPINNER_ACTIVE" == "true" ]]; then
      return 0
    fi
    _original_log_info "$@"
  }
fi

# Override log_error to clear spinner before printing
if declare -f log_error >/dev/null 2>&1; then
  eval "$(echo '_original_log_error()'; declare -f log_error | tail -n +2)"
  log_error() {
    if [[ "$_SPINNER_ACTIVE" == "true" ]]; then
      _spinner_stop
      _spinner_clear_line
    fi
    _original_log_error "$@"
  }
fi

# Override log_warn to clear spinner before printing
if declare -f log_warn >/dev/null 2>&1; then
  eval "$(echo '_original_log_warn()'; declare -f log_warn | tail -n +2)"
  log_warn() {
    if [[ "$_SPINNER_ACTIVE" == "true" ]]; then
      _spinner_stop
      _spinner_clear_line
    fi
    _original_log_warn "$@"
  }
fi
