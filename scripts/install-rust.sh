#!/usr/bin/env bash
# Instalador interativo do `prt` Rust — Linux e macOS.
#
#   curl -fsSL https://raw.githubusercontent.com/nitoba/pr-tools/main/scripts/install-rust.sh | bash
#   PR_TOOLS_VERSION=v4.0.10 bash scripts/install-rust.sh
#
# Env (todos opcionais, têm precedência sobre as perguntas):
#   PR_TOOLS_VERSION      tag (v4.0.10) ou 'latest' (padrão)
#   PR_TOOLS_REPOSITORY   owner/repo (padrão: nitoba/pr-tools)
#   PR_TOOLS_INSTALL_DIR  diretório de instalação (padrão: ~/.local/bin)
#   PR_TOOLS_BINARY       usa um binário local em vez de baixar do GitHub
#   PR_TOOLS_GITHUB_TOKEN token para API/downloads privados ou rate-limit maior
#
# Flags: --yes/-y (não pergunta nada), --version X, --dir PATH, --help.
set -euo pipefail

# ---------- estilo ----------
if [[ -t 1 && -z "${NO_COLOR:-}" ]]; then
  C_BOLD='\033[1m'; C_DIM='\033[2m'; C_CYAN='\033[36m'; C_GREEN='\033[32m'
  C_YELLOW='\033[33m'; C_RED='\033[31m'; C_RESET='\033[0m'
else
  C_BOLD=''; C_DIM=''; C_CYAN=''; C_GREEN=''; C_YELLOW=''; C_RED=''; C_RESET=''
fi

say()  { printf '%b\n' "$*"; }
ok()   { say "${C_GREEN}✔${C_RESET} $*"; }
warn() { say "${C_YELLOW}!${C_RESET} $*"; }
fail() { say "${C_RED}✘ $*" >&2; exit 1; }
step() { say "${C_CYAN}→${C_RESET} ${C_BOLD}$*${C_RESET}"; }
dim()  { say "${C_DIM}$*${C_RESET}"; }

banner() {
  say "${C_CYAN}${C_BOLD}"
  say '  ◆ prt — instalador'
  say "${C_RESET}${C_DIM}  descrições de PR e Test Cases a partir do Git${C_RESET}"
  say ''
}

# Lê do /dev/tty quando o stdin é um pipe (curl | bash), senão assume default.
ask() { # ask <var> <pergunta> <default>
  local var="$1" question="$2" default="$3" answer="" input="/dev/stdin"
  if [[ ! -t 0 && -r /dev/tty ]]; then input="/dev/tty"; fi
  if [[ "$ASSUME_YES" = "1" || ! -r "$input" ]]; then
    printf -v "$var" '%s' "$default"
    return 0
  fi
  printf '%b' "${C_BOLD}${question}${C_RESET} ${C_DIM}[${default}]${C_RESET} " >"$input"
  IFS= read -r answer <"$input" || true
  if [[ -z "$answer" ]]; then answer="$default"; fi
  printf -v "$var" '%s' "$answer"
}

confirm() { # confirm <pergunta> <default: Y|n> → 0 = sim
  local ans=""
  ask ans "$1" "$2"
  [[ "$ans" =~ ^[SsYy]?$ ]]
}

# ---------- args ----------
ASSUME_YES=0
VERSION="${PR_TOOLS_VERSION:-latest}"
REPOSITORY="${PR_TOOLS_REPOSITORY:-nitoba/pr-tools}"
INSTALL_DIR="${PR_TOOLS_INSTALL_DIR:-}"
LOCAL_BINARY="${PR_TOOLS_BINARY:-}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    -y|--yes) ASSUME_YES=1; shift ;;
    --version) VERSION="${2:?--version requer um valor}"; shift 2 ;;
    --version=*) VERSION="${1#--version=}"; shift ;;
    --dir) INSTALL_DIR="${2:?--dir requer um valor}"; shift 2 ;;
    --dir=*) INSTALL_DIR="${1#--dir=}"; shift ;;
    --repo) REPOSITORY="${2:?--repo requer um valor}"; shift 2 ;;
    --repo=*) REPOSITORY="${1#--repo=}"; shift ;;
    -h|--help)
      say "Uso: install.sh [--yes] [--version vX.Y.Z] [--dir PATH] [--repo owner/repo]"
      exit 0 ;;
    *) fail "Opção desconhecida: $1 (use --help)." ;;
  esac
done

banner

# ---------- plataforma ----------
PLATFORM="$(uname -s)"
ARCH="$(uname -m)"
case "$PLATFORM:$ARCH" in
  Linux:x86_64|Linux:amd64)    ASSET='prt-rust-linux-x64';   PRETTY='Linux x64' ;;
  Linux:aarch64|Linux:arm64)   ASSET='prt-rust-linux-arm64';  PRETTY='Linux arm64' ;;
  Darwin:arm64)                ASSET='prt-rust-macos-arm64';  PRETTY='macOS arm64' ;;
  MINGW*|MSYS*|CYGWIN*|:*|Windows_NT:*)
    fail "Windows detectado: use o instalador PowerShell (scripts/install-rust.ps1)." ;;
  *)
    fail "Plataforma não suportada: $PLATFORM/$ARCH." ;;
esac
step "Sistema detectado: $PRETTY ${C_DIM}($PLATFORM/$ARCH → $ASSET)${C_RESET}"

if [[ "$REPOSITORY" =~ ^https?://github\.com/ ]]; then
  REPOSITORY="${REPOSITORY#https://github.com/}"
  REPOSITORY="${REPOSITORY#http://github.com/}"
fi
REPOSITORY="${REPOSITORY%.git}"
if [[ ! "$REPOSITORY" =~ ^[^/]+/[^/]+$ ]]; then
  fail "Repositório inválido: $REPOSITORY (use owner/repo ou PR_TOOLS_REPOSITORY)."
fi

# ---------- perguntas ----------
ask VERSION "Versão a instalar ('latest' ou vX.Y.Z)" "$VERSION"
if [[ -z "$INSTALL_DIR" ]]; then
  ask INSTALL_DIR "Diretório de instalação" "${XDG_BIN_HOME:-$HOME/.local/bin}"
fi
TARGET_PATH="$INSTALL_DIR/prt"

say ''
say "${C_BOLD}Resumo:${C_RESET}"
say "  release   ${C_CYAN}$REPOSITORY @ $VERSION${C_RESET}"
say "  asset     ${C_CYAN}$ASSET${C_RESET}"
say "  destino   ${C_CYAN}$TARGET_PATH${C_RESET}"
say ''
if ! confirm "Prosseguir com a instalação?" "Y"; then
  say 'Instalação cancelada.'
  exit 0
fi
say ''

# ---------- download ----------
TMP_DIR=""
BINARY_PATH="$LOCAL_BINARY"
if [[ -z "$BINARY_PATH" ]]; then
  command -v curl >/dev/null 2>&1 || fail "Comando necessário não encontrado: curl."
  TMP_DIR="$(mktemp -d)"
  trap 'rm -rf "$TMP_DIR"' EXIT
  if [[ "$VERSION" == 'latest' ]]; then
    DOWNLOAD_URL="https://github.com/$REPOSITORY/releases/latest/download/$ASSET"
    # Descobre a tag real só para exibir (best-effort).
    TAG="$(curl --fail --silent --show-error --location --output /dev/null --write-out '%{url_effective}' \
      "$DOWNLOAD_URL" 2>/dev/null | grep -o '[^/]*$' || true)"
    [[ -n "$TAG" ]] && dim "  última release: $TAG"
  else
    TAG="v${VERSION#v}"
    DOWNLOAD_URL="https://github.com/$REPOSITORY/releases/download/$TAG/$ASSET"
  fi
  BINARY_PATH="$TMP_DIR/$ASSET"
  step "Baixando ${C_DIM}$DOWNLOAD_URL${C_RESET}"
  CURL_ARGS=(--fail --show-error --location --progress-bar)
  if [[ -n "${PR_TOOLS_GITHUB_TOKEN:-}" ]]; then
    CURL_ARGS+=(--header "Authorization: Bearer $PR_TOOLS_GITHUB_TOKEN")
  fi
  curl "${CURL_ARGS[@]}" "$DOWNLOAD_URL" --output "$BINARY_PATH" \
    || fail "Falha no download (verifique versão/rede/permissão do repo)."
  chmod 0755 "$BINARY_PATH"
else
  step "Usando binário local: $BINARY_PATH"
fi

[[ -f "$BINARY_PATH" ]] || fail "Binário não encontrado em $BINARY_PATH."
[[ -x "$BINARY_PATH" ]] || fail "O arquivo $BINARY_PATH não é executável."
[[ -s "$BINARY_PATH" ]] || fail "O arquivo $BINARY_PATH está vazio."

# Verificação: o próprio binário responde --version?
if INSTALLED_VERSION="$("$BINARY_PATH" --version 2>/dev/null)"; then
  ok "Binário verificado: $INSTALLED_VERSION"
else
  warn "Não foi possível executar '$BINARY_PATH --version'; seguindo assim mesmo."
fi

# ---------- instalação ----------
step "Instalando em $TARGET_PATH"
mkdir -p "$INSTALL_DIR"
install -m 0755 "$BINARY_PATH" "$TARGET_PATH"
ok "prt instalado em $TARGET_PATH"

# ---------- PATH ----------
append_path_entry() { # append_path_entry <arquivo>
  local file="$1" escaped line
  escaped="$(printf '%q' "$INSTALL_DIR")"
  line="export PATH=$escaped:\$PATH"
  mkdir -p "$(dirname "$file")"
  if [[ ! -f "$file" ]] || ! grep -Fqx "$line" "$file"; then
    { printf '\n# Added by prt installer\n%s\n' "$line"; } >>"$file"
    ok "PATH atualizado em $file"
  else
    dim "  PATH já configurado em $file"
  fi
}

append_fish_path_entry() { # append_fish_path_entry <arquivo>
  local file="$1" escaped line
  if command -v fish >/dev/null 2>&1; then
    escaped="$(fish -c 'string escape -- "$argv[1]"' -- "$INSTALL_DIR")"
  else
    escaped="$(printf '%q' "$INSTALL_DIR")"
  fi
  line="fish_add_path --prepend $escaped"
  mkdir -p "$(dirname "$file")"
  if [[ ! -f "$file" ]] || ! grep -Fqx "$line" "$file"; then
    { printf '\n# Added by prt installer\n%s\n' "$line"; } >>"$file"
    ok "PATH atualizado em $file"
  else
    dim "  PATH já configurado em $file"
  fi
}

if [[ ":${PATH:-}:" != *:"$INSTALL_DIR":* ]]; then
  if confirm "Adicionar $INSTALL_DIR ao PATH do seu shell?" "Y"; then
    SHELL_NAME="${SHELL##*/}"
    case "$SHELL_NAME" in
      fish) append_fish_path_entry "${XDG_CONFIG_HOME:-$HOME/.config}/fish/config.fish" ;;
      bash) append_path_entry "$HOME/.profile"; append_path_entry "$HOME/.bashrc" ;;
      zsh)  append_path_entry "$HOME/.profile"; append_path_entry "$HOME/.zprofile" ;;
      *)    append_path_entry "$HOME/.profile" ;;
    esac
  else
    warn "Adicione manualmente: export PATH=\"$INSTALL_DIR:\$PATH\""
  fi
else
  ok "$INSTALL_DIR já está no PATH"
fi

# ---------- fim ----------
say ''
say "${C_GREEN}${C_BOLD}  ✔ Pronto! Execute:${C_RESET}"
say "      prt init     ${C_DIM}# primeira configuração${C_RESET}"
say "      prt doctor   ${C_DIM}# diagnóstico do ambiente${C_RESET}"
case ":${PATH:-}:" in
  *:"$INSTALL_DIR":*) ;;
  *) say ''; warn "Abra um novo terminal para usar \`prt\` diretamente." ;;
esac
