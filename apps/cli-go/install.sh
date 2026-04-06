#!/usr/bin/env bash
set -euo pipefail

# ============================================================
# prt installer — Linux and macOS
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/nitoba/pr-tools/main/apps/cli-go/install.sh | bash
#   curl -fsSL .../install.sh | INSTALL_VERSION=v1.0.0 bash
# ============================================================

REPO="nitoba/pr-tools"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
GITHUB_API="https://api.github.com/repos/$REPO/releases/latest"
RELEASES_URL="https://github.com/$REPO/releases/download"

RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

if [[ ! -t 1 || -n "${NO_COLOR:-}" ]]; then
  RED=''; GREEN=''; CYAN=''; YELLOW=''; BOLD=''; NC=''
fi

log_info()    { echo -e "${CYAN}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[OK]${NC} $1"; }
log_warn()    { echo -e "${YELLOW}[AVISO]${NC} $1"; }
log_error()   { echo -e "${RED}[ERRO]${NC} $1" >&2; }

echo ""
echo -e "${BOLD}prt installer${NC}"
echo -e "${BOLD}=============${NC}"
echo ""

# --- Check dependencies ---
for cmd in curl tar; do
  if ! command -v "$cmd" &>/dev/null; then
    log_error "Dependencia nao encontrada: $cmd"
    exit 1
  fi
done

# --- Detect OS ---
OS="$(uname -s)"
case "$OS" in
  Linux)  OS="linux" ;;
  Darwin) OS="darwin" ;;
  *)
    log_error "Sistema operacional nao suportado: $OS"
    log_error "Use o instalador PowerShell no Windows."
    exit 1
    ;;
esac

# --- Detect arch ---
ARCH="$(uname -m)"
case "$ARCH" in
  x86_64)           ARCH="amd64" ;;
  aarch64 | arm64)  ARCH="arm64" ;;
  *)
    log_error "Arquitetura nao suportada: $ARCH"
    exit 1
    ;;
esac

log_info "Plataforma detectada: $OS/$ARCH"

# --- Resolve version ---
if [[ -n "${INSTALL_VERSION:-}" ]]; then
  VERSION="${INSTALL_VERSION#v}"   # strip leading 'v' if present
  log_info "Versao solicitada: v$VERSION"
else
  log_info "Buscando ultima versao..."
  LATEST_JSON="$(curl -fsSL "$GITHUB_API" 2>/dev/null || true)"
  VERSION="$(echo "$LATEST_JSON" | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"v\{0,1\}\([^"]*\)".*/\1/')"
  if [[ -z "$VERSION" ]]; then
    log_error "Nao foi possivel determinar a ultima versao. Defina INSTALL_VERSION manualmente."
    exit 1
  fi
  log_info "Ultima versao: v$VERSION"
fi

# --- Build download URL ---
ARCHIVE="prt_${VERSION}_${OS}_${ARCH}.tar.gz"
URL="${RELEASES_URL}/v${VERSION}/${ARCHIVE}"

# --- Download and extract ---
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

log_info "Baixando $ARCHIVE..."
HTTP_CODE="$(curl -fsSL -w "%{http_code}" -o "$TMP_DIR/$ARCHIVE" "$URL" 2>/dev/null || echo "000")"

if [[ "$HTTP_CODE" == "404" ]]; then
  log_error "Versao v$VERSION nao encontrada: $URL"
  exit 1
elif [[ "$HTTP_CODE" != "200" ]]; then
  log_error "Erro ao baixar (HTTP $HTTP_CODE): $URL"
  exit 1
fi

log_info "Extraindo..."
tar -xzf "$TMP_DIR/$ARCHIVE" -C "$TMP_DIR"

# --- Install ---
mkdir -p "$INSTALL_DIR"
mv "$TMP_DIR/prt" "$INSTALL_DIR/prt"
chmod +x "$INSTALL_DIR/prt"

log_success "prt instalado em $INSTALL_DIR/prt"

# --- PATH check ---
if ! echo ":${PATH}:" | grep -q ":${INSTALL_DIR}:"; then
  log_warn "$INSTALL_DIR nao esta no seu PATH."
  log_warn "Adicione ao seu shell profile:"
  log_warn '  export PATH="$HOME/.local/bin:$PATH"'
fi

# --- Smoke test ---
if "$INSTALL_DIR/prt" --version &>/dev/null; then
  VERSION_OUT="$("$INSTALL_DIR/prt" --version 2>&1)"
  log_success "Instalacao verificada: $VERSION_OUT"
else
  log_warn "Instalacao concluida, mas 'prt --version' retornou erro."
fi

echo ""
log_success "Instalacao completa! Execute: prt init"
echo ""
