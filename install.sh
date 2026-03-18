#!/usr/bin/env bash
set -euo pipefail

# ============================================================
# pr-tools installer
# Usage: curl -fsSL https://raw.githubusercontent.com/nitoba/pr-tools/main/install.sh | bash
# ============================================================

REPO="nitoba/pr-tools"
BRANCH="main"
RAW_URL="https://raw.githubusercontent.com/$REPO/$BRANCH"
INSTALL_DIR="$HOME/.local/bin"

RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

log_info()    { echo -e "${CYAN}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[OK]${NC} $1"; }
log_warn()    { echo -e "${YELLOW}[AVISO]${NC} $1"; }
log_error()   { echo -e "${RED}[ERRO]${NC} $1" >&2; }

echo ""
echo -e "${BOLD}pr-tools installer${NC}"
echo -e "${BOLD}==================${NC}"
echo ""

# Check dependencies
for cmd in curl git jq; do
  if ! command -v "$cmd" &>/dev/null; then
    log_error "Dependencia nao encontrada: $cmd"
    log_error "Instale $cmd e tente novamente."
    exit 1
  fi
done
log_success "Dependencias encontradas (curl, git, jq)"

# Create install directory
mkdir -p "$INSTALL_DIR"
log_info "Diretorio de instalacao: $INSTALL_DIR"

# Download script
log_info "Baixando create-pr-description..."
if curl -fsSL "$RAW_URL/bin/create-pr-description" -o "$INSTALL_DIR/create-pr-description"; then
  chmod +x "$INSTALL_DIR/create-pr-description"
  log_success "Script instalado: $INSTALL_DIR/create-pr-description"
else
  log_error "Falha ao baixar o script."
  exit 1
fi

# Check if install dir is in PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
  log_warn "$INSTALL_DIR nao esta no PATH."
  echo ""

  # Detect shell config file
  SHELL_NAME=$(basename "$SHELL")
  case "$SHELL_NAME" in
    zsh)  SHELL_RC="$HOME/.zshrc" ;;
    bash) SHELL_RC="$HOME/.bashrc" ;;
    *)    SHELL_RC="$HOME/.profile" ;;
  esac

  echo -e "  Adicione ao seu ${BOLD}$SHELL_RC${NC}:"
  echo ""
  echo -e "    ${CYAN}export PATH=\"\$HOME/.local/bin:\$PATH\"${NC}"
  echo ""
  echo -e "  Depois execute: ${CYAN}source $SHELL_RC${NC}"
  echo ""
else
  log_success "$INSTALL_DIR ja esta no PATH"
fi

# Run --init if config doesn't exist
CONFIG_DIR="$HOME/.config/pr-tools"
if [[ ! -f "$CONFIG_DIR/.env" ]]; then
  echo ""
  log_info "Iniciando configuracao..."
  log_info "O wizard vai te guiar na configuracao das credenciais."
  echo ""
  "$INSTALL_DIR/create-pr-description" --init
else
  log_info "Configuracao existente encontrada em $CONFIG_DIR"
  echo ""
  echo -e "  Para reconfigurar: ${CYAN}create-pr-description --init${NC}"
fi

echo ""
echo -e "${BOLD}========================================${NC}"
echo -e "${GREEN}Instalacao concluida!${NC}"
echo ""
echo -e "Uso: ${CYAN}create-pr-description${NC}"
echo -e "${BOLD}========================================${NC}"
