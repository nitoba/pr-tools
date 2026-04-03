# Deploy Strategy Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement CI/CD with GitHub Actions, automated changelogs via git-cliff, tag-based releases, and version-aware install.sh.

**Architecture:** Two GitHub Actions workflows (CI + Release), a `cliff.toml` config for changelog generation, a `VERSION` file as single source of truth, and modifications to `install.sh` to support installing from specific tags.

**Tech Stack:** Bash, GitHub Actions, git-cliff, GitHub Releases API

---

## File Structure

| File                            | Action | Responsibility                                       |
| ------------------------------- | ------ | ---------------------------------------------------- |
| `.github/workflows/ci.yml`      | Create | CI: shellcheck + syntax + smoke tests                |
| `.github/workflows/release.yml` | Create | CD: tag push → git-cliff → GitHub Release + assets   |
| `cliff.toml`                    | Create | git-cliff configuration for Conventional Commits     |
| `CHANGELOG.md`                  | Create | Generated changelog (initial run covers all history) |
| `VERSION`                       | Create | Single source of truth for version number            |
| `install.sh`                    | Modify | Add `INSTALL_VERSION` env var support                |
| `src/bin/create-pr-description` | Modify | Read version from `VERSION` file with fallback       |
| `src/bin/create-test-card`      | Modify | Read version from `VERSION` file with fallback       |
| `README.md`                     | Modify | Document installation from specific versions         |

---

## Chunk 1: CI Workflow

### Task 1: Create CI Workflow

**Files:**

- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Create `.github/workflows/ci.yml`**

```yaml
name: CI

on:
  pull_request:
    branches: [main]
  push:
    branches: [main]

jobs:
  shellcheck:
    name: ShellCheck
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install shellcheck
        run: sudo apt-get update && sudo apt-get install -y shellcheck

      - name: Run shellcheck
        run: |
          shellcheck src/bin/create-pr-description
          shellcheck src/bin/create-test-card
          shellcheck install.sh
          for f in src/lib/*.sh; do
            shellcheck "$f"
          done

  syntax-check:
    name: Syntax Check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Check Bash syntax
        run: |
          bash -n src/bin/create-pr-description
          bash -n src/bin/create-test-card
          bash -n install.sh
          for f in src/lib/*.sh; do
            bash -n "$f"
          done

  smoke-test:
    name: Smoke Tests
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install dependencies
        run: sudo apt-get update && sudo apt-get install -y jq

      - name: create-pr-description --help
        run: bash src/bin/create-pr-description --help

      - name: create-pr-description --version
        run: bash src/bin/create-pr-description --version

      - name: create-test-card --help
        run: bash src/bin/create-test-card --help

      - name: create-test-card --version
        run: bash src/bin/create-test-card --version
```

- [ ] **Step 2: Validate workflow syntax**

Run: `cat .github/workflows/ci.yml` — verify YAML is valid (proper indentation, no tabs).

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add CI workflow with shellcheck, syntax check, and smoke tests"
```

---

## Chunk 2: Release Workflow + cliff.toml

### Task 2: Create cliff.toml

**Files:**

- Create: `cliff.toml`

- [ ] **Step 1: Create `cliff.toml`**

```toml
[changelog]
header = "# Changelog\n"
body = """
## {{ version | trim_start_matches(pat="v") }} — {{ timestamp | date(format="%Y-%m-%d") }}
{% for group, commits in commits | group_by(attribute="group") %}
### {{ group | upper_first }}
{% for commit in commits %}
- {{ commit.message | upper_first }} (`{{ commit.id | truncate(length=7, end="") }}`)
{% endfor %}
{% endfor %}
"""
footer = ""
trim = true

[git]
conventional_commits = true
commit_parsers = [
  { message = "^feat", group = "Features" },
  { message = "^fix", group = "Bug Fixes" },
  { message = "^docs", group = "Documentation" },
  { message = "^chore", group = "Chores" },
  { message = "^ci", group = "CI/CD" },
  { message = "^refactor", group = "Refactoring" },
  { message = "^perf", group = "Performance" },
  { message = "^test", group = "Tests" },
]
sort_commits = "newest"
```

- [ ] **Step 2: Commit**

```bash
git add cliff.toml
git commit -m "chore: add git-cliff configuration for conventional commits"
```

### Task 3: Create Release Workflow

**Files:**

- Create: `.github/workflows/release.yml`

- [ ] **Step 1: Create `.github/workflows/release.yml`**

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

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
          curl -L https://github.com/orhun/git-cliff/releases/latest/download/git-cliff-x86_64-unknown-linux-gnu.tar.gz | tar xz
          sudo mv git-cliff-*/git-cliff /usr/local/bin/git-cliff
          chmod +x /usr/local/bin/git-cliff

      - name: Generate changelog
        id: changelog
        run: |
          TAG=${GITHUB_REF#refs/tags/}
          CHANGELOG=$(git-cliff --tag "$TAG" --unreleased)
          echo "changelog<<CHANGELOG_EOF" >> "$GITHUB_OUTPUT"
          echo "$CHANGELOG" >> "$GITHUB_OUTPUT"
          echo "CHANGELOG_EOF" >> "$GITHUB_OUTPUT"

      - name: Create GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          body: ${{ steps.changelog.outputs.changelog }}
          prerelease: ${{ contains(github.ref_name, '-') }}
          files: |
            install.sh
            src/bin/create-pr-description
            src/bin/create-test-card
            src/lib/common.sh
            src/lib/llm.sh
            src/lib/azure.sh
            src/lib/test-card-azure.sh
            src/lib/test-card-llm.sh
            src/lib/ui.sh
```

- [ ] **Step 2: Validate workflow syntax**

Run: `cat .github/workflows/release.yml` — verify YAML is valid.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: add release workflow with git-cliff changelog generation"
```

---

## Chunk 3: VERSION file + CHANGELOG.md

### Task 4: Create VERSION file

**Files:**

- Create: `VERSION`

- [ ] **Step 1: Create `VERSION` file**

```
2.9.0
```

- [ ] **Step 2: Commit**

```bash
git add VERSION
git commit -m "chore: add VERSION file as single source of truth"
```

### Task 5: Generate initial CHANGELOG.md

**Files:**

- Create: `CHANGELOG.md`

- [ ] **Step 1: Install git-cliff locally (if not already installed)**

```bash
# Check if installed
command -v git-cliff || echo "git-cliff not installed"
# If not installed, download:
curl -L https://github.com/orhun/git-cliff/releases/latest/download/git-cliff-x86_64-unknown-linux-gnu.tar.gz | tar xz
sudo mv git-cliff-*/git-cliff /usr/local/bin/git-cliff
chmod +x /usr/local/bin/git-cliff
```

- [ ] **Step 2: Generate initial changelog**

```bash
git-cliff > CHANGELOG.md
```

- [ ] **Step 3: Verify changelog content**

Run: `head -50 CHANGELOG.md` — verify it contains grouped commits from the project history.

- [ ] **Step 4: Commit**

```bash
git add CHANGELOG.md
git commit -m "docs: generate initial changelog from git history"
```

---

## Chunk 4: install.sh — Version Support

### Task 6: Modify install.sh

**Files:**

- Modify: `install.sh`

- [ ] **Step 1: Read current install.sh and add INSTALL_VERSION support**

After line 10 (`BRANCH="main"`), add:

```bash
# Version support — install from a specific tag or branch
INSTALL_VERSION="${INSTALL_VERSION:-main}"
if [[ "$INSTALL_VERSION" == v* ]]; then
  REF="refs/tags/$INSTALL_VERSION"
else
  REF="$INSTALL_VERSION"
fi
RAW_URL="https://raw.githubusercontent.com/$REPO/$REF"
```

Replace the existing line 11 (`RAW_URL="https://raw.githubusercontent.com/$REPO/$BRANCH"`) with the version-aware logic above. The `BRANCH` variable is no longer needed since `REF` replaces it.

- [ ] **Step 2: Add 404 error handling for version downloads**

After each `curl` download block (lines 57-65 for create-pr-description, lines 69-77 for create-test-card, and lines 87-95 for libs), the existing `curl -fsSL` already handles 404 via `-f` flag. Add a clearer error message by wrapping the version check at the top:

```bash
# Validate version exists
if [[ "$INSTALL_VERSION" != "main" ]]; then
  log_info "Verificando versao $INSTALL_VERSION..."
  if ! curl -fsSL -o /dev/null "https://raw.githubusercontent.com/$REPO/refs/tags/$INSTALL_VERSION/install.sh"; then
    log_error "Versao $INSTALL_VERSION nao encontrada."
    log_error "Versoes disponiveis: https://github.com/$REPO/tags"
    exit 1
  fi
  log_success "Versao $INSTALL_VERSION encontrada"
fi
```

Place this block after the dependency check (after line 48) and before the install directory creation.

- [ ] **Step 3: Validate syntax**

```bash
bash -n install.sh
```

- [ ] **Step 4: Smoke test**

```bash
bash install.sh 2>&1 | head -5
# Should show: "[INFO] Verificando versao main..." is NOT shown for main
# Should proceed normally
```

- [ ] **Step 5: Commit**

```bash
git add install.sh
git commit -m "feat: support installing from specific versions via INSTALL_VERSION env var"
```

---

## Chunk 5: CLI Version Reading + README

### Task 7: Update CLI scripts to read VERSION file

**Files:**

- Modify: `src/bin/create-pr-description`
- Modify: `src/bin/create-test-card`

- [ ] **Step 1: Update create-pr-description version logic**

Replace line 10 (`VERSION="2.8.10"`) with:

```bash
# Version — read from VERSION file if available, fallback to hardcoded
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." 2>/dev/null && pwd)"
VERSION="2.9.0"
if [[ -f "$REPO_ROOT/VERSION" ]]; then
  VERSION="$(cat "$REPO_ROOT/VERSION" | tr -d '[:space:]')"
fi
```

- [ ] **Step 2: Update create-test-card version logic**

Replace line 10 (`VERSION="0.3.11"`) with:

```bash
# Version — read from VERSION file if available, fallback to hardcoded
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." 2>/dev/null && pwd)"
VERSION="2.9.0"
if [[ -f "$REPO_ROOT/VERSION" ]]; then
  VERSION="$(cat "$REPO_ROOT/VERSION" | tr -d '[:space:]')"
fi
```

- [ ] **Step 3: Validate syntax**

```bash
bash -n src/bin/create-pr-description
bash -n src/bin/create-test-card
```

- [ ] **Step 4: Smoke test**

```bash
bash src/bin/create-pr-description --version
bash src/bin/create-test-card --version
# Both should show version from VERSION file (2.9.0)
```

- [ ] **Step 5: Commit**

```bash
git add src/bin/create-pr-description src/bin/create-test-card
git commit -m "feat: read version from VERSION file with hardcoded fallback"
```

### Task 8: Update README.md

**Files:**

- Modify: `README.md`

- [ ] **Step 1: Add version installation section to README**

After the "Instalação" section (after line 9), add:

````markdown
### Instalar uma versão específica

```bash
# Instalar a versão estável mais recente
curl -fsSL https://raw.githubusercontent.com/nitoba/pr-tools/main/install.sh | INSTALL_VERSION=v2.9.0 bash

# Instalar do branch main (bleeding edge)
curl -fsSL https://raw.githubusercontent.com/nitoba/pr-tools/main/install.sh | bash
```
````

Veja as versões disponíveis em [Releases](https://github.com/nitoba/pr-tools/releases).

````

- [ ] **Step 2: Add release process section to README**

At the end of README.md (before `## Licença`), add:

```markdown
## Processo de Release

### Criar uma nova versão

1. Atualize o arquivo `VERSION` na raiz do projeto
2. Atualize o `VERSION` hardcoded nos scripts `src/bin/*`
3. Commit: `chore: bump version to vX.Y.Z`
4. Crie a tag: `git tag vX.Y.Z`
5. Push: `git push origin main --tags`

O workflow de release irá automaticamente:
- Gerar o changelog com git-cliff
- Criar um GitHub Release com o changelog
- Fazer upload dos scripts como assets

### Versionamento Semântico

- **MAJOR** — Breaking changes
- **MINOR** — Novas features
- **PATCH** — Bug fixes
````

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: document versioned installation and release process"
```

---

## Chunk 6: Verification

### Task 9: Final verification

- [ ] **Step 1: Run all syntax checks**

```bash
bash -n install.sh
bash -n src/bin/create-pr-description
bash -n src/bin/create-test-card
for f in src/lib/*.sh; do bash -n "$f"; done
```

- [ ] **Step 2: Run smoke tests**

```bash
bash src/bin/create-pr-description --help
bash src/bin/create-pr-description --version
bash src/bin/create-test-card --help
bash src/bin/create-test-card --version
```

- [ ] **Step 3: Verify all files exist**

```bash
ls -la .github/workflows/ci.yml
ls -la .github/workflows/release.yml
ls -la cliff.toml
ls -la CHANGELOG.md
ls -la VERSION
```

- [ ] **Step 4: Verify git log**

```bash
git log --oneline -10
```

Should show commits for: CI workflow, release workflow, cliff.toml, VERSION, CHANGELOG, install.sh changes, CLI version changes, README updates.
