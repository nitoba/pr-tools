# PRT Phase 3 — Migration and Deprecation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the Bash CLI entirely, ship cross-platform installers for the `prt` binary, simplify the release pipeline, and rewrite the README to document `prt` as the only supported CLI.

**Architecture:** Five independent chunks executed in order. Chunks 1–3 are purely additive/destructive and have no dependencies on each other. Chunk 4 (release pipeline) depends on Chunk 1 (Bash removed). Chunk 5 (README) is always last.

**Tech Stack:** Bash, PowerShell, YAML (GitHub Actions), goreleaser v2, git-cliff

---

## File Structure

### Files to delete
```
apps/cli/                          ← entire directory (Bash scripts, libs, installer)
.github/workflows/ci.yml           ← only validated Bash scripts, no longer needed
```

### Files to create
```
apps/cli-go/install.sh             ← Linux/macOS binary installer
apps/cli-go/install.ps1            ← Windows PowerShell binary installer
```

### Files to modify
```
apps/cli-go/.goreleaser.prt.yml    ← fix before.hooks paths, remove --skip=publish
.github/workflows/release.yml      ← remove Bash packaging, let goreleaser own the release
README.md                          ← complete rewrite for prt as primary CLI
```

---

## Chunk 1: Remove Bash CLI

### Task 1.1: Delete Bash CLI and its CI workflow

**Files:**
- Delete: `apps/cli/` (entire directory)
- Delete: `.github/workflows/ci.yml`

- [ ] **Step 1: Delete the Bash CLI directory**

```bash
cd /path/to/pr-tools
git rm -r apps/cli/
```

- [ ] **Step 2: Delete the Bash CI workflow**

```bash
git rm .github/workflows/ci.yml
```

- [ ] **Step 3: Verify no remaining references to apps/cli in active workflows**

```bash
grep -r "apps/cli" .github/workflows/ --include="*.yml"
```

Expected: only `cli-go-ci.yml` shows `apps/cli-go` references — no `apps/cli/` hits.

- [ ] **Step 4: Commit**

```bash
git commit -m "chore: remove Bash CLI and its CI workflow"
```

---

## Chunk 2: `apps/cli-go/install.sh`

### Task 2.1: Create Linux/macOS installer

**Files:**
- Create: `apps/cli-go/install.sh`

- [ ] **Step 1: Create `apps/cli-go/install.sh`**

```bash
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
```

- [ ] **Step 2: Make it executable**

```bash
chmod +x apps/cli-go/install.sh
```

- [ ] **Step 3: Smoke test the script syntax**

```bash
bash -n apps/cli-go/install.sh
```

Expected: no output (syntax OK)

- [ ] **Step 4: Commit**

```bash
git add apps/cli-go/install.sh
git commit -m "feat(installer): add Linux/macOS install script"
```

---

## Chunk 3: `apps/cli-go/install.ps1`

### Task 3.1: Create Windows PowerShell installer

**Files:**
- Create: `apps/cli-go/install.ps1`

- [ ] **Step 1: Create `apps/cli-go/install.ps1`**

```powershell
# prt installer — Windows (PowerShell)
# Usage:
#   irm https://raw.githubusercontent.com/nitoba/pr-tools/main/apps/cli-go/install.ps1 | iex
#   $env:INSTALL_VERSION="v1.0.0"; irm .../install.ps1 | iex

$ErrorActionPreference = "Stop"

$Repo = "nitoba/pr-tools"
$InstallDir = Join-Path $env:LOCALAPPDATA "prt\bin"
$GithubApi = "https://api.github.com/repos/$Repo/releases/latest"
$ReleasesUrl = "https://github.com/$Repo/releases/download"

function Write-Info    { param($msg) Write-Host "[INFO] $msg" -ForegroundColor Cyan }
function Write-Ok      { param($msg) Write-Host "[OK] $msg" -ForegroundColor Green }
function Write-Warn    { param($msg) Write-Host "[AVISO] $msg" -ForegroundColor Yellow }
function Write-Err     { param($msg) Write-Host "[ERRO] $msg" -ForegroundColor Red; exit 1 }

Write-Host ""
Write-Host "prt installer" -ForegroundColor Bold
Write-Host "============="
Write-Host ""

# --- Detect arch ---
$CpuArch = $env:PROCESSOR_ARCHITECTURE
switch ($CpuArch) {
    "AMD64" { $Arch = "amd64" }
    "ARM64" { $Arch = "arm64" }
    default  { Write-Err "Arquitetura nao suportada: $CpuArch" }
}

Write-Info "Plataforma detectada: windows/$Arch"

# --- Resolve version ---
if ($env:INSTALL_VERSION) {
    $Version = $env:INSTALL_VERSION.TrimStart("v")
    Write-Info "Versao solicitada: v$Version"
} else {
    Write-Info "Buscando ultima versao..."
    try {
        $LatestJson = Invoke-RestMethod -Uri $GithubApi -Headers @{ "User-Agent" = "prt-installer" }
        $Version = $LatestJson.tag_name.TrimStart("v")
    } catch {
        Write-Err "Nao foi possivel determinar a ultima versao. Defina INSTALL_VERSION manualmente."
    }
    Write-Info "Ultima versao: v$Version"
}

# --- Build download URL ---
$Archive = "prt_${Version}_windows_${Arch}.zip"
$Url = "$ReleasesUrl/v$Version/$Archive"

# --- Download ---
$TmpDir = Join-Path $env:TEMP "prt-install-$(New-Guid)"
New-Item -ItemType Directory -Path $TmpDir | Out-Null
$ArchivePath = Join-Path $TmpDir $Archive

Write-Info "Baixando $Archive..."
try {
    Invoke-WebRequest -Uri $Url -OutFile $ArchivePath -UseBasicParsing
} catch {
    if ($_.Exception.Response.StatusCode -eq 404) {
        Write-Err "Versao v$Version nao encontrada: $Url"
    }
    Write-Err "Erro ao baixar: $($_.Exception.Message)"
}

# --- Extract ---
Write-Info "Extraindo..."
Expand-Archive -Path $ArchivePath -DestinationPath $TmpDir -Force

# --- Install ---
if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir | Out-Null
}

$ExeSrc = Join-Path $TmpDir "prt.exe"
$ExeDst = Join-Path $InstallDir "prt.exe"
Move-Item -Path $ExeSrc -Destination $ExeDst -Force

Write-Ok "prt instalado em $ExeDst"

# --- Cleanup ---
Remove-Item -Recurse -Force $TmpDir

# --- PATH update ---
$UserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($UserPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("PATH", "$UserPath;$InstallDir", "User")
    Write-Ok "Adicionado ao PATH do usuario: $InstallDir"
    Write-Warn "Reinicie o terminal para que o PATH seja atualizado."
} else {
    Write-Info "$InstallDir ja esta no PATH."
}

# --- Smoke test ---
try {
    $VersionOut = & $ExeDst --version 2>&1
    Write-Ok "Instalacao verificada: $VersionOut"
} catch {
    Write-Warn "Instalacao concluida, mas 'prt --version' retornou erro."
}

Write-Host ""
Write-Ok "Instalacao completa! Execute: prt init"
Write-Host ""
```

- [ ] **Step 2: Commit**

```bash
git add apps/cli-go/install.ps1
git commit -m "feat(installer): add Windows PowerShell install script"
```

---

## Chunk 4: Simplify Release Pipeline

### Task 4.1: Update `.goreleaser.prt.yml`

**Files:**
- Modify: `apps/cli-go/.goreleaser.prt.yml`

The current `before.hooks` run `go vet` and `go test` without specifying the working directory. Since goreleaser is invoked from the repo root, these commands need to target `apps/cli-go`. Also, CI already runs tests — remove the duplicate test run from goreleaser hooks to keep release fast.

- [ ] **Step 1: Update `.goreleaser.prt.yml`**

Replace the file with:

```yaml
version: 2

project_name: prt

before:
  hooks:
    - sh -c "cd apps/cli-go && go vet ./..."

builds:
  - id: prt
    main: ./cmd/prt
    dir: apps/cli-go
    binary: prt
    ldflags:
      - -s -w
      - -X github.com/nitoba/pr-tools/apps/cli-go/internal/version.Version={{ .Version }}
      - -X github.com/nitoba/pr-tools/apps/cli-go/internal/version.Commit={{ .Commit }}
      - -X github.com/nitoba/pr-tools/apps/cli-go/internal/version.Date={{ .Date }}
    goos: [linux, darwin, windows]
    goarch: [amd64, arm64]

archives:
  - id: prt-archives
    builds: [prt]
    name_template: "prt_{{ .Version }}_{{ .Os }}_{{ .Arch }}"
    formats:
      - tar.gz
    format_overrides:
      - goos: windows
        format: zip

checksum:
  name_template: prt_{{ .Version }}_checksums.txt

release:
  disable: false
```

- [ ] **Step 2: Verify goreleaser config is valid (requires goreleaser installed)**

```bash
cd /path/to/pr-tools
goreleaser check --config apps/cli-go/.goreleaser.prt.yml
```

Expected: `• config is valid`

If goreleaser is not installed locally, skip this step — CI will validate it.

- [ ] **Step 3: Commit**

```bash
git add apps/cli-go/.goreleaser.prt.yml
git commit -m "chore(release): fix goreleaser hooks and build dir for monorepo layout"
```

---

### Task 4.2: Simplify `release.yml`

**Files:**
- Modify: `.github/workflows/release.yml`

Remove the Bash packaging step and the separate `softprops/action-gh-release` step. Goreleaser creates the GitHub Release directly, using the changelog generated by git-cliff via `--release-notes`.

- [ ] **Step 1: Replace `.github/workflows/release.yml`**

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'
  workflow_dispatch:

permissions:
  contents: write

jobs:
  release:
    name: Create Release
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Install git-cliff
        run: |
          curl -fsSL https://github.com/orhun/git-cliff/releases/download/v2.12.0/git-cliff-2.12.0-x86_64-unknown-linux-gnu.tar.gz | tar xz
          sudo mv git-cliff-2.12.0/git-cliff /usr/local/bin/git-cliff
          chmod +x /usr/local/bin/git-cliff

      - name: Generate changelog
        run: |
          TAG=${GITHUB_REF#refs/tags/}
          git-cliff --tag "$TAG" > /tmp/release-notes.md

      - name: Setup Go
        uses: actions/setup-go@v5
        with:
          go-version-file: apps/cli-go/go.mod

      - name: Run GoReleaser
        uses: goreleaser/goreleaser-action@v6
        with:
          distribution: goreleaser
          version: latest
          args: release --clean --config apps/cli-go/.goreleaser.prt.yml --release-notes /tmp/release-notes.md
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "chore(ci): simplify release workflow — goreleaser only, no Bash packaging"
```

---

## Chunk 5: Rewrite README

### Task 5.1: Rewrite `README.md`

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace `README.md` with the new content**

```markdown
# pr-tools

Ferramentas de produtividade para Pull Requests e Test Cases no Azure DevOps.
Gera descrições de PR e cards de teste automaticamente usando IA.

## Instalação

**Linux / macOS**

```bash
curl -fsSL https://raw.githubusercontent.com/nitoba/pr-tools/main/apps/cli-go/install.sh | bash
```

**Windows (PowerShell)**

```powershell
irm https://raw.githubusercontent.com/nitoba/pr-tools/main/apps/cli-go/install.ps1 | iex
```

**Versão específica**

```bash
# Linux/macOS
curl -fsSL .../install.sh | INSTALL_VERSION=v1.0.0 bash

# Windows
$env:INSTALL_VERSION="v1.0.0"; irm .../install.ps1 | iex
```

### Requisitos

- `curl` e `tar` (Linux/macOS) — sem dependências adicionais
- PowerShell 5+ (Windows)
- API key de pelo menos um provider de LLM

## Quick Start

```bash
prt init      # cria ~/.config/pr-tools/.env
# edite o arquivo com suas API keys
prt doctor    # verifica configuração
prt desc      # gera descrição de PR
prt test      # gera card de teste no Azure DevOps
```

## Configuração

Edite `~/.config/pr-tools/.env`:

```bash
# Providers (ordem de fallback)
PR_PROVIDERS="openrouter,groq,gemini,ollama"

# API Keys
OPENROUTER_API_KEY="sk-or-..."
GROQ_API_KEY="gsk_..."
GEMINI_API_KEY="..."
OLLAMA_API_KEY="..."

# Modelos (opcional — usa padrão se não definir)
# OPENROUTER_MODEL="meta-llama/llama-3.3-70b-instruct:free"
# GROQ_MODEL="llama-3.3-70b-versatile"
# GEMINI_MODEL="gemini-2.0-flash"
# OLLAMA_MODEL="llama3.2"

# Azure DevOps
AZURE_PAT="seu-pat-token"

# Reviewers padrão para PRs (opcional)
# PR_REVIEWER_DEV="email@exemplo.com"
# PR_REVIEWER_SPRINT="email@exemplo.com"

# Defaults para Test Cases (opcional)
# TEST_CARD_AREA_PATH="PROJETO\Devops"
# TEST_CARD_ASSIGNED_TO="nome@exemplo.com"

# Debug
# PRT_DEBUG=true
# PRT_NO_COLOR=true
```

Variáveis de ambiente do shell sobrescrevem o `.env`.

Precedência: flags CLI > variáveis de ambiente > `.env` > defaults internos.

## Comandos

### `prt desc` — Gera descrição de PR

```bash
# Gera descrição para a branch atual
prt desc

# Apenas mostra o prompt, sem chamar a LLM
prt desc --dry-run

# Define a branch de origem manualmente
prt desc --source feature/1234-login

# Vincula um work item ao PR
prt desc --work-item 11763

# Saída sem renderização Markdown
prt desc --raw

# Cria o PR no Azure DevOps automaticamente
prt desc --create
```

### `prt test` — Gera card de teste no Azure DevOps

```bash
# Gera card de teste para um work item
prt test --work-item 11763

# Especifica org/project/repo do Azure DevOps
prt test --work-item 11763 --org myorg --project myproject

# Apenas gera o markdown, não cria no Azure DevOps
prt test --work-item 11763 --no-create

# Apenas mostra o prompt, sem chamar a LLM
prt test --work-item 11763 --dry-run
```

### `prt init` — Inicializa configuração

```bash
prt init
```

Cria ou atualiza `~/.config/pr-tools/.env` com os valores padrão.

### `prt doctor` — Verifica configuração

```bash
prt doctor
```

Reporta o estado da configuração, versão e ambiente.

## Providers suportados

| Provider | Modelo padrão |
|----------|---------------|
| [OpenRouter](https://openrouter.ai) | `meta-llama/llama-3.3-70b-instruct:free` |
| [Groq](https://console.groq.com) | `llama-3.3-70b-versatile` |
| [Google Gemini](https://aistudio.google.com) | `gemini-2.0-flash` |
| [Ollama](https://ollama.com) | `llama3.2` |

## Processo de Release

```bash
./release.sh 1.0.1
```

O script atualiza a versão, gera o CHANGELOG e abre um PR.
Após o merge, o workflow `auto-tag.yml` cria a tag e o `release.yml` publica os binários via goreleaser.

## Licença

MIT
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: rewrite README for prt Go CLI as primary"
```

---

## Final verification

- [ ] **Run all Go tests to make sure nothing broke**

```bash
cd apps/cli-go && go test ./... -v
```

Expected: all packages pass.

- [ ] **Verify no references to old Bash CLI remain in active files**

```bash
grep -r "create-pr-description\|create-test-card\|apps/cli/" .github/ README.md --include="*.yml" --include="*.md" 2>/dev/null | grep -v "^Binary"
```

Expected: no matches (or only matches inside `docs/` historical specs).
