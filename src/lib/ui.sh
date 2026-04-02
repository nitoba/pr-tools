#!/usr/bin/env bash
# src/lib/ui.sh — spinner progress UI for pr-tools

[[ -n "${_PR_TOOLS_UI_SH:-}" ]] && return 0
_PR_TOOLS_UI_SH=1

# ---- State ----
_SPINNER_PID=""
_SPINNER_MSG=""
_SPINNER_ACTIVE=false
_SPINNER_INTERACTIVE=true

_TITLE_MSG=""
_TITLE_ACTIVE=false
_TITLE_LINES_BELOW=0

if [[ ! -t 2 || -n "${NO_COLOR:-}" ]]; then
  _SPINNER_INTERACTIVE=false
fi

_UI_GREEN="${GREEN:-\033[0;32m}"
_UI_RED="${RED:-\033[0;31m}"
_UI_YELLOW="${YELLOW:-\033[1;33m}"
_UI_BOLD="${BOLD:-\033[1m}"
_UI_DIM="${DIM:-\033[2m}"
_UI_NC="${NC:-\033[0m}"

# Orange tones based on #c15f3c
_UI_ORANGE='\033[38;2;193;95;60m'
_UI_ORANGE_LIGHT='\033[38;2;224;130;85m'
_UI_ORANGE_DIM='\033[38;2;153;75;48m'
# Gray for hierarchy connector
_UI_GRAY='\033[38;5;242m'

if [[ "$_SPINNER_INTERACTIVE" == "false" ]]; then
  _UI_GREEN=""
  _UI_RED=""
  _UI_YELLOW=""
  _UI_BOLD=""
  _UI_DIM=""
  _UI_NC=""
  _UI_ORANGE=""
  _UI_ORANGE_LIGHT=""
  _UI_ORANGE_DIM=""
  _UI_GRAY=""
fi

# ---- Single background loop: handles both title sparkle + step spinner ----

_spinner_loop() {
  local msg="$1"
  local title_dist="$2"    # 0 = no title, >0 = title is N lines up
  local title_msg="$3"
  local frame=0

  local sparkle_frames=("✦" "✧" "✦" "·")
  local sparkle_colors=(
    "\033[38;2;193;95;60m\033[1m"     # #c15f3c bold
    "\033[38;2;224;130;85m\033[1m"    # lighter orange bold
    "\033[38;2;193;95;60m\033[2m"     # #c15f3c dim
    "\033[38;2;153;75;48m\033[2m"     # darker orange dim
  )

  while true; do
    local i=$(( frame % 4 ))

    # Animate spinner on current line
    if [[ "$title_dist" -gt 0 ]]; then
      # With title: "  │ ● msg..."  (│ at col 2, aligned after title icon)
      if (( frame % 2 == 0 )); then
        printf '\r\033[2K  \033[38;5;242m│\033[0m %b●%b %s...' "\033[1;33m\033[1m" "\033[0m" "$msg" >&2
      else
        printf '\r\033[2K  \033[38;5;242m│\033[0m %b●%b %s...' "\033[1;33m\033[2m" "\033[0m" "$msg" >&2
      fi
    else
      # No title: "    ● msg..."
      if (( frame % 2 == 0 )); then
        printf '\r\033[2K    %b●%b %s...' "\033[1;33m\033[1m" "\033[0m" "$msg" >&2
      else
        printf '\r\033[2K    %b●%b %s...' "\033[1;33m\033[2m" "\033[0m" "$msg" >&2
      fi
    fi

    # Animate title sparkle if active
    if [[ "$title_dist" -gt 0 && -n "$title_msg" ]]; then
      printf '\033[s' >&2
      printf '\033[%dA\r\033[2K' "$title_dist" >&2
      printf ' %b%s%b %b%s%b' "${sparkle_colors[$i]}" "${sparkle_frames[$i]}" "\033[0m" "\033[38;2;153;75;48m" "$title_msg" "\033[0m" >&2
      printf '\033[u' >&2
    fi

    frame=$(( frame + 1 ))
    sleep 0.3
  done
}

_spinner_stop() {
  # Skip if running inside a subshell (e.g. result=$(func))
  # Subshells inherit _SPINNER_PID but must not kill the parent's spinner
  if [[ "${BASHPID:-$$}" != "$$" ]]; then
    return 0
  fi
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

# ---- Title API ----

ui_title_start() {
  local msg="$1"
  _TITLE_MSG="$msg"
  _TITLE_ACTIVE=true
  _TITLE_LINES_BELOW=0

  if [[ "$_SPINNER_INTERACTIVE" == "true" ]]; then
    printf ' %b✦%b %b%s%b\n' "$_UI_ORANGE$_UI_BOLD" "$_UI_NC" "$_UI_ORANGE_DIM" "$msg" "$_UI_NC" >&2
  else
    printf ' ✦ %s\n' "$msg" >&2
  fi
}

ui_title_done() {
  _TITLE_ACTIVE=false
  _TITLE_MSG=""
  _TITLE_LINES_BELOW=0
}

# ---- Step API ----

step_start() {
  local msg="$1"

  if [[ "$_SPINNER_ACTIVE" == "true" ]]; then
    step_done "$_SPINNER_MSG"
  fi

  _SPINNER_MSG="$msg"
  _SPINNER_ACTIVE=true

  if [[ "$_SPINNER_INTERACTIVE" == "true" ]]; then
    local dist=0
    [[ "$_TITLE_ACTIVE" == "true" ]] && dist=$(( _TITLE_LINES_BELOW + 1 ))
    _spinner_loop "$msg" "$dist" "$_TITLE_MSG" &
    _SPINNER_PID=$!
    disown "$_SPINNER_PID" 2>/dev/null
  else
    if [[ "$_TITLE_ACTIVE" == "true" ]]; then
      printf '  │ ● %s...\n' "$msg" >&2
    else
      printf '    ● %s...\n' "$msg" >&2
    fi
  fi
}

step_done() {
  local msg="${1:-$_SPINNER_MSG}"
  _spinner_stop
  _spinner_clear_line
  if [[ "$_TITLE_ACTIVE" == "true" ]]; then
    printf '  %b│%b %b✓%b %s\n' "$_UI_GRAY" "$_UI_NC" "$_UI_GREEN" "$_UI_NC" "$msg" >&2
  else
    printf '    %b✓%b %s\n' "$_UI_GREEN" "$_UI_NC" "$msg" >&2
  fi
  _SPINNER_MSG=""
  if [[ "$_TITLE_ACTIVE" == "true" ]]; then
    _TITLE_LINES_BELOW=$(( _TITLE_LINES_BELOW + 1 ))
  fi
}

step_fail() {
  local msg="${1:-$_SPINNER_MSG}"
  _spinner_stop
  _spinner_clear_line
  if [[ "$_TITLE_ACTIVE" == "true" ]]; then
    printf '  %b│%b %b✗%b %s\n' "$_UI_GRAY" "$_UI_NC" "$_UI_RED" "$_UI_NC" "$msg" >&2
  else
    printf '    %b✗%b %s\n' "$_UI_RED" "$_UI_NC" "$msg" >&2
  fi
  _SPINNER_MSG=""
  if [[ "$_TITLE_ACTIVE" == "true" ]]; then
    _TITLE_LINES_BELOW=$(( _TITLE_LINES_BELOW + 1 ))
  fi
}

# ---- Trap: cleanup on exit ----

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

# ---- Override log functions for consistent UI ----

if declare -f log_info >/dev/null 2>&1; then
  eval "$(echo '_original_log_info()'; declare -f log_info | tail -n +2)"
  log_info() {
    if [[ "$_SPINNER_ACTIVE" == "true" ]]; then
      return 0
    fi
    if [[ "$_TITLE_ACTIVE" == "true" ]]; then
      printf '  %b│%b %b%s%b\n' "$_UI_GRAY" "$_UI_NC" "$_UI_DIM" "$1" "$_UI_NC" >&2
      _TITLE_LINES_BELOW=$(( _TITLE_LINES_BELOW + 1 ))
      return 0
    fi
    _original_log_info "$@"
  }
fi

if declare -f log_error >/dev/null 2>&1; then
  eval "$(echo '_original_log_error()'; declare -f log_error | tail -n +2)"
  log_error() {
    if [[ "$_SPINNER_ACTIVE" == "true" ]]; then
      _spinner_stop
      _spinner_clear_line
    fi
    if [[ "$_TITLE_ACTIVE" == "true" ]]; then
      printf '  %b│%b %b✗ %s%b\n' "$_UI_GRAY" "$_UI_NC" "$_UI_RED" "$1" "$_UI_NC" >&2
      _TITLE_LINES_BELOW=$(( _TITLE_LINES_BELOW + 1 ))
      return 0
    fi
    _original_log_error "$@"
  }
fi

if declare -f log_warn >/dev/null 2>&1; then
  eval "$(echo '_original_log_warn()'; declare -f log_warn | tail -n +2)"
  log_warn() {
    if [[ "$_SPINNER_ACTIVE" == "true" ]]; then
      _spinner_stop
      _spinner_clear_line
    fi
    if [[ "$_TITLE_ACTIVE" == "true" ]]; then
      printf '  %b│%b %b⚠ %s%b\n' "$_UI_GRAY" "$_UI_NC" "$_UI_YELLOW" "$1" "$_UI_NC" >&2
      _TITLE_LINES_BELOW=$(( _TITLE_LINES_BELOW + 1 ))
      return 0
    fi
    _original_log_warn "$@"
  }
fi
