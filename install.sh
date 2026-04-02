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

if [[ ! -t 1 || -n "${NO_COLOR:-}" ]]; then
  RED=''
  GREEN=''
  CYAN=''
  YELLOW=''
  BOLD=''
  NC=''
fi

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

# Download scripts
log_info "Baixando create-pr-description..."
tmp_pr=$(mktemp)
if curl -fsSL "$RAW_URL/src/bin/create-pr-description" -o "$tmp_pr"; then
  chmod +x "$tmp_pr"
  mv "$tmp_pr" "$INSTALL_DIR/create-pr-description"
  log_success "Script instalado: $INSTALL_DIR/create-pr-description"
else
  rm -f "$tmp_pr"
  log_error "Falha ao baixar create-pr-description."
  exit 1
fi

log_info "Baixando create-test-card..."
tmp_test=$(mktemp)
if curl -fsSL "$RAW_URL/src/bin/create-test-card" -o "$tmp_test"; then
  chmod +x "$tmp_test"
  mv "$tmp_test" "$INSTALL_DIR/create-test-card"
  log_success "Script instalado: $INSTALL_DIR/create-test-card"
else
  rm -f "$tmp_test"
  log_error "Falha ao baixar create-test-card."
  exit 1
fi

# Download libs
LIB_INSTALL_DIR="$HOME/.local/lib/pr-tools"
mkdir -p "$LIB_INSTALL_DIR"
log_info "Diretorio de libs: $LIB_INSTALL_DIR"

for lib_file in common.sh llm.sh azure.sh test-card-azure.sh test-card-llm.sh; do
  log_info "Baixando lib/$lib_file..."
  tmp_lib=$(mktemp)
  if curl -fsSL "$RAW_URL/src/lib/$lib_file" -o "$tmp_lib"; then
    mv "$tmp_lib" "$LIB_INSTALL_DIR/$lib_file"
    log_success "Lib instalada: $LIB_INSTALL_DIR/$lib_file"
  else
    rm -f "$tmp_lib"
    log_error "Falha ao baixar lib/$lib_file."
    exit 1
  fi
done

# Check if install dir is in PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
  log_warn "$INSTALL_DIR nao esta no PATH."
  echo ""

  # Detect shell config file
  SHELL_NAME=$(basename "${SHELL:-sh}")
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
ENV_FILE="$CONFIG_DIR/.env"
if [[ ! -f "$ENV_FILE" ]]; then
  echo ""
  log_info "Iniciando configuracao..."
  if [[ -t 0 && -t 1 ]]; then
    log_info "O wizard vai te guiar na configuracao das credenciais."
    echo ""
    "$INSTALL_DIR/create-pr-description" --init
    "$INSTALL_DIR/create-test-card" --init
  else
    # Non-interactive: create default .env so the scripts are usable
    mkdir -p "$CONFIG_DIR"
    cat > "$ENV_FILE" <<'ENVEOF'
# Providers em ordem de prioridade (tenta o primeiro, se falhar vai pro proximo)
PR_PROVIDERS="openrouter,groq,gemini"

# API Keys (descomente e preencha)
# OPENROUTER_API_KEY="sk-or-..."
# GROQ_API_KEY="gsk_..."
# GEMINI_API_KEY="..."

# Modelos (opcional)
# OPENROUTER_MODEL="meta-llama/llama-3.3-70b-instruct:free"
# GROQ_MODEL="llama-3.3-70b-versatile"
# GEMINI_MODEL="gemini-3.1-flash-lite-preview"

# Streaming (exibe tokens em tempo real; padrao: false)
PR_STREAM="false"

# Azure DevOps
# AZURE_PAT="your-pat-token"

# Test cards
# TEST_CARD_AREA_PATH="AGROTRACE\\Devops"
# TEST_CARD_ASSIGNED_TO="nome@empresa.com"
ENVEOF
    chmod 600 "$ENV_FILE"
    log_success "Arquivo .env criado: $ENV_FILE"
    log_warn "Edite $ENV_FILE e preencha suas API keys."
    echo -e "  Para configurar interativamente: ${CYAN}create-pr-description --init${NC}"
    echo -e "  Para configurar test cards:      ${CYAN}create-test-card --init${NC}"
  fi
else
  log_info "Configuracao existente encontrada em $CONFIG_DIR"
  echo ""
  echo -e "  Para reconfigurar PRs: ${CYAN}create-pr-description --init${NC}"
  echo -e "  Para reconfigurar Test Cases: ${CYAN}create-test-card --init${NC}"
fi

echo ""
echo -e "${BOLD}========================================${NC}"
echo -e "${GREEN}Instalacao concluida!${NC}"
echo ""
echo -e "Uso: ${CYAN}create-pr-description${NC}"
echo -e "Uso: ${CYAN}create-test-card${NC}"
echo -e "${BOLD}========================================${NC}"
