#!/usr/bin/env bash
set -euo pipefail

# ============================================================
# release.sh — Automatiza o processo de release do pr-tools
# Uso: ./release.sh 2.9.1
# ============================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

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

# ---- Validate input ----
if [[ $# -lt 1 ]]; then
  log_error "Uso: $0 <versão>"
  log_error "Exemplo: $0 2.9.1"
  exit 1
fi

VERSION="$1"

# Validate semver format
if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  log_error "Versão inválida: $VERSION"
  log_error "Use o formato SemVer: MAJOR.MINOR.PATCH (ex: 2.9.1)"
  exit 1
fi

TAG="v$VERSION"

# ---- Validate repo state ----
if ! git rev-parse --is-inside-work-tree &>/dev/null; then
  log_error "Este script deve ser executado na raiz do repositorio git."
  exit 1
fi

# Check for uncommitted changes
if [[ -n "$(git status --porcelain)" ]]; then
  log_error "Existem alterações não commitadas. Commit ou stash antes de criar uma release."
  exit 1
fi

# Check if on main branch
CURRENT_BRANCH="$(git branch --show-current)"
if [[ "$CURRENT_BRANCH" != "main" ]]; then
  log_warn "Branch atual: $CURRENT_BRANCH (recomendado: main)"
  read -rp "Continuar mesmo assim? (y/N) " confirm
  if [[ "$confirm" != "y" && "$confirm" != "Y" ]]; then
    log_info "Cancelado."
    exit 0
  fi
fi

# Check if tag already exists
if git rev-parse "$TAG" &>/dev/null; then
  log_error "A tag $TAG ja existe."
  exit 1
fi

# ---- Confirm ----
CURRENT_VERSION="$(cat "$SCRIPT_DIR/VERSION" 2>/dev/null || echo "unknown")"
echo ""
echo -e "${BOLD}Release $TAG${NC}"
echo -e "${BOLD}==================${NC}"
echo ""
echo -e "Versão atual:  $CURRENT_VERSION"
echo -e "Nova versão:   $VERSION"
echo -e "Tag:           $TAG"
echo ""
read -rp "Continuar? (y/N) " confirm
if [[ "$confirm" != "y" && "$confirm" != "Y" ]]; then
  log_info "Cancelado."
  exit 0
fi

echo ""

# ---- Update VERSION file ----
log_info "Atualizando VERSION para $VERSION..."
printf '%s\n' "$VERSION" > "$SCRIPT_DIR/VERSION"
log_success "VERSION atualizado"

# ---- Update hardcoded versions in CLI scripts ----
log_info "Atualizando versão hardcoded nos scripts..."

for script in "$SCRIPT_DIR/src/bin/create-pr-description" "$SCRIPT_DIR/src/bin/create-test-card"; do
  if [[ -f "$script" ]]; then
    # Update the fallback VERSION line (VERSION="X.Y.Z")
    sed -i "s/^VERSION=\"[0-9]\+\.[0-9]\+\.[0-9]\+\"$/VERSION=\"$VERSION\"/" "$script"
    log_success "Atualizado: $script"
  fi
done

# ---- Regenerate CHANGELOG.md ----
if command -v git-cliff &>/dev/null; then
  log_info "Regenerando CHANGELOG.md..."
  git-cliff > "$SCRIPT_DIR/CHANGELOG.md"
  log_success "CHANGELOG.md atualizado"
else
  log_warn "git-cliff não encontrado. CHANGELOG.md será gerado pelo workflow de release."
fi

# ---- Commit ----
log_info "Criando commit..."
git add VERSION src/bin/create-pr-description src/bin/create-test-card CHANGELOG.md
git commit -m "chore: bump version to $TAG"
log_success "Commit criado"

# ---- Create tag ----
log_info "Criando tag $TAG..."
git tag -a "$TAG" -m "Release $TAG"
log_success "Tag $TAG criada"

# ---- Push ----
log_info "Push para origin..."
git push origin main
git push origin "$TAG"
log_success "Push concluido"

echo ""
echo -e "${BOLD}========================================${NC}"
echo -e "${GREEN}Release $TAG publicada com sucesso!${NC}"
echo ""
echo -e "O workflow de release irá:"
echo -e "  1. Gerar o changelog com git-cliff"
echo -e "  2. Criar o GitHub Release"
echo -e "  3. Fazer upload dos scripts como assets"
echo ""
echo -e "Acompanhe em: ${CYAN}https://github.com/nitoba/pr-tools/actions${NC}"
echo -e "${BOLD}========================================${NC}"
