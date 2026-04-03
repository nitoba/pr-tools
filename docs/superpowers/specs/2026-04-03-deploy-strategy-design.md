# Deploy Strategy Design — pr-tools

**Date:** 2026-04-03
**Author:** Bruno Alves (via AI)
**Status:** Draft

## Problem

O projeto `pr-tools` não possui CI/CD, changelogs automatizados nem estratégia de releases. O `install.sh` baixa sempre do `main`, sem validação de qualidade nem versionamento semântico estruturado.

## Goals

1. Validar qualidade do código em cada PR via CI
2. Gerar changelogs automáticos a partir de commits Conventional Commits
3. Criar GitHub Releases ao fazer push de tags versionadas
4. Permitir instalação de versões específicas via `install.sh`

## Architecture

### 1. CI Workflow — `.github/workflows/ci.yml`

**Trigger:** PRs para `main`, push para `main`

**Jobs:**

- `shellcheck` — Executa `shellcheck` em todos os scripts Bash do projeto
- `syntax-check` — Executa `bash -n` em `src/bin/*`, `src/lib/*`, `install.sh`
- `smoke-test` — Executa `--help` e `--version` nos dois CLIs

Todos os jobs rodam em `ubuntu-latest`. Não é necessário matrix de OS porque o Bash é portável e o shellcheck cobre incompatibilidades.

### 2. Release Workflow — `.github/workflows/release.yml`

**Trigger:** Push de tag com pattern `v*`

**Jobs:**

- `release` — Gera changelog com `git-cliff` e cria GitHub Release

**Steps:**

1. Checkout do repo com `fetch-depth: 0` (histórico completo)
2. Instalar `git-cliff` via apt
3. Gerar changelog para a tag: `git-cliff --tag $TAG` (commits entre a tag anterior e esta)
4. Criar GitHub Release usando `softprops/action-gh-release`
   - Se tag contiver `-` (ex: `v1.0.0-beta.1`), marcar como **pre-release**
   - Corpo do release = changelog gerado
5. Fazer upload dos assets (cada arquivo individualmente via loop no workflow):
   - `install.sh` (raiz)
   - `src/bin/create-pr-description`
   - `src/bin/create-test-card`
   - `src/lib/common.sh`, `src/lib/llm.sh`, `src/lib/azure.sh`, `src/lib/test-card-azure.sh`, `src/lib/test-card-llm.sh`, `src/lib/ui.sh`

### 3. `cliff.toml`

**Configuração:**

```toml
[changelog]
header = "# Changelog\n"
body = """
## {{ version | trim_start_matches(pat="v") }} — {{ timestamp | date(format="%Y-%m-%d") }}
{% for group, commits in commits | group_by(attribute="group") %}
### {{ group | upper_first }}
{% for commit in commits %}
- {{ commit.message | upper_first }} (`{{ commit.id | truncate(length=7, end="") }}`)\
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

### 4. `CHANGELOG.md`

- Arquivo na raiz do repo
- O workflow de release **regenera o arquivo completo** a cada release (`git-cliff > CHANGELOG.md`)
- Localmente, para appendar apenas a nova versão: `git-cliff --tag v2.9.0 --unreleased >> CHANGELOG.md`
- Histórico inicial será gerado uma vez com `git-cliff > CHANGELOG.md` (todas as tags até HEAD)

### 5. `install.sh` — Suporte a versão

**Mudanças:**

- Adicionar variável `INSTALL_VERSION="${INSTALL_VERSION:-main}"` no topo (nome distinto do `VERSION` interno dos CLIs)
- Determinar a ref de download: se `INSTALL_VERSION` começar com `v`, usar `refs/tags/$INSTALL_VERSION`; senão usar `$INSTALL_VERSION` como branch name
- Reatribuir `RAW_URL` com a ref resolvida: `RAW_URL="https://raw.githubusercontent.com/$REPO/$REF"` (substitui o hardcoded da linha 11)
- Todos os `curl` existentes usam `$RAW_URL`, nenhuma outra mudança necessária
- Documentar no README como instalar versão específica

**Comportamento:**

```bash
# Instalar versão específica (note: INSTALL_VERSION vai antes de bash, não antes de curl)
curl -fsSL https://raw.githubusercontent.com/nitoba/pr-tools/main/install.sh | INSTALL_VERSION=v2.9.0 bash

# Instalar do main (bleeding edge, comportamento atual)
curl -fsSL https://raw.githubusercontent.com/nitoba/pr-tools/main/install.sh | bash
```

**Nota técnica:** Variáveis de ambiente antes do `curl` não são passadas pelo pipe. O correto é `curl ... | INSTALL_VERSION=v2.9.0 bash`. Se a versão não existir, `curl` retorna 404 e o script aborta com `[ERRO] Versão v2.9.0 nao encontrada.`

## Data Flow

### CI
```
PR/Push → GitHub Actions → shellcheck + bash -n + --help/--version → Pass/Fail
```

### Release
```
git tag vX.Y.Z → git push --tags → GitHub Actions → git-cliff → GitHub Release + assets
```

### Install
```
curl install.sh | bash → download scripts da tag ou main → instalar em ~/.local/bin
```

## Error Handling

### CI
- Se shellcheck falhar, PR é bloqueado
- Se syntax check falhar, PR é bloqueado
- Se smoke test falhar, PR é bloqueado

### Release
- Se tag não seguir pattern `v*`, workflow não roda
- Se `git-cliff` falhar (ex: tag não existe), workflow falha com erro claro
- Se upload de assets falhar, release é criada mas sem assets (partial success)

### Install
- Se versão não existe, `curl` retorna 404 e script aborta com mensagem clara
- Fallback para `main` se `VERSION` não for definido

## Version Bumping

**Processo manual:**

1. Criar/atualizar arquivo `VERSION` na raiz do repo com o valor `X.Y.Z` (single source of truth)
2. Atualizar `VERSION` hardcoded em `src/bin/create-pr-description` e `src/bin/create-test-card` para bater com o arquivo
3. Commit: `chore: bump version to vX.Y.Z`
4. `git tag vX.Y.Z`
5. `git push origin main --tags`

**SemVer:**

- `MAJOR` — Breaking changes (ex: mudança de provider, remoção de flag)
- `MINOR` — Novas features (ex: novo provider, nova flag)
- `PATCH` — Bug fixes (ex: correção de português, fix de spinner)

**Nota:** O `VERSION` nos scripts CLI (`src/bin/*`) continua existindo como fallback standalone. Em runtime, o CLI tenta ler o arquivo `VERSION` na raiz do repo (se disponível) e usa o hardcoded como fallback.

## Testing Strategy

- `bash -n` — syntax validation (roda em CI)
- `shellcheck` — lint (roda em CI)
- `--help` / `--version` — smoke tests (roda em CI)
- `--dry-run` — teste manual sem chamar LLM
- Testes manuais em macOS/Linux para releases maiores

## Files Created/Modified

| Arquivo | Ação | Propósito |
|---------|------|-----------|
| `.github/workflows/ci.yml` | Create | CI workflow |
| `.github/workflows/release.yml` | Create | Release workflow |
| `cliff.toml` | Create | git-cliff config |
| `CHANGELOG.md` | Create | Generated changelog |
| `VERSION` | Create | Single source of truth for version |
| `install.sh` | Modify | Support INSTALL_VERSION env var |
| `src/bin/create-pr-description` | Modify | Read version from VERSION file |
| `src/bin/create-test-card` | Modify | Read version from VERSION file |
| `README.md` | Modify | Document release process |
