#!/usr/bin/env bash
set -euo pipefail

# ============================================================
# release.sh — Prepara um PR de release para o pr-tools
# Uso: ./release.sh 2.9.1
#
# Fluxo:
#   1. Atualiza VERSION, versoes hardcoded nos scripts e CHANGELOG.md
#   2. Cria branch release/vX.Y.Z
#   3. Commit + push
#   4. Abre PR para main
#
# Apos merge do PR:
#   - O workflow auto-tag.yml cria a tag automaticamente
#   - O workflow release.yml cria o GitHub Release com changelog e assets
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
  log_error "Uso: $0 <versao>"
  log_error "Exemplo: $0 2.9.1"
  exit 1
fi

VERSION="$1"

# Validate semver format
if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  log_error "Versao invalida: $VERSION"
  log_error "Use o formato SemVer: MAJOR.MINOR.PATCH (ex: 2.9.1)"
  exit 1
fi

BRANCH="release/$VERSION"
TAG="v$VERSION"

# ---- Validate repo state ----
if ! git rev-parse --is-inside-work-tree &>/dev/null; then
  log_error "Este script deve ser executado na raiz do repositorio git."
  exit 1
fi

# Check for uncommitted changes
if [[ -n "$(git status --porcelain)" ]]; then
  log_error "Existem alteracoes nao commitadas. Commit ou stash antes de criar uma release."
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

# Check if branch already exists
if git rev-parse "$BRANCH" &>/dev/null; then
  log_error "A branch $BRANCH ja existe."
  exit 1
fi

# Check if tag already exists
if git rev-parse "$TAG" &>/dev/null; then
  log_error "A tag $TAG ja existe."
  exit 1
fi

# ---- Confirm ----
CURRENT_VERSION="$(git tag --sort=-version:refname | head -1 | sed 's/^v//' || echo "unknown")"

echo ""
echo -e "${BOLD}Release $TAG${NC}"
echo -e "${BOLD}==================${NC}"
echo ""
echo -e "Ultima tag:    ${CURRENT_VERSION:-none}"
echo -e "Nova versao:   $VERSION"
echo -e "Branch:        $BRANCH"
echo ""
echo -e "O que sera feito:"
echo -e "  1. Atualizar VERSION, scripts e CHANGELOG.md"
echo -e "  2. Criar branch $BRANCH com commit"
echo -e "  3. Push e abrir PR para main"
echo -e "  4. Apos merge: tag e GitHub Release automaticos"
echo ""
read -rp "Continuar? (y/N) " confirm
if [[ "$confirm" != "y" && "$confirm" != "Y" ]]; then
  log_info "Cancelado."
  exit 0
fi

echo ""

# ---- Create branch ----
log_info "Criando branch $BRANCH..."
git checkout -b "$BRANCH"
log_success "Branch criada"

# ---- Regenerate CHANGELOG.md ----
if command -v git-cliff &>/dev/null; then
  log_info "Regenerando CHANGELOG.md..."
  git-cliff --tag "$TAG" > "$SCRIPT_DIR/CHANGELOG.md"
  log_success "CHANGELOG.md atualizado"
else
  log_warn "git-cliff nao encontrado. CHANGELOG.md sera gerado pelo workflow de release."
fi

# ---- Commit ----
log_info "Criando commit..."
git add CHANGELOG.md
git commit -m "chore: bump version to $TAG"
log_success "Commit criado"

# ---- Push ----
log_info "Push para origin..."
git push -u origin "$BRANCH"
log_success "Push concluido"

# ---- Open PR ----
if command -v gh &>/dev/null; then
  log_info "Abrindo PR..."
  PR_URL=$(gh pr create \
    --title "Release $TAG" \
    --body "$(cat <<EOF
## Release $TAG

Prepara release da versao **$VERSION** do \`prt\`.

### O que muda
- \`CHANGELOG.md\` atualizado para $VERSION

### Apos o merge
- O workflow \`auto-tag.yml\` cria a tag \`$TAG\` automaticamente
- O workflow \`release.yml\` publica os binarios \`prt\` via goreleaser
EOF
)"
  )
  log_success "PR criado: $PR_URL"
else
  log_warn "gh CLI nao encontrado."
  echo ""
  echo -e "Abra o PR manualmente:"
  echo -e "  ${CYAN}https://github.com/nitoba/pr-tools/compare/main...$BRANCH?expand=1${NC}"
fi

echo ""
echo -e "${BOLD}========================================${NC}"
echo -e "${GREEN}PR de release $TAG preparado!${NC}"
echo ""
echo -e "Proximos passos:"
echo -e "  1. Aguarde o CI (shellcheck + syntax + smoke)"
echo -e "  2. Faca merge do PR"
echo -e "  3. A tag \`$TAG\` e o GitHub Release sao criados automaticamente"
echo ""
echo -e "Acompanhe em: ${CYAN}https://github.com/nitoba/pr-tools/actions${NC}"
echo -e "${BOLD}========================================${NC}"
