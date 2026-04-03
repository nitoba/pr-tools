# Monorepo Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Converter o repositório flat em um monorepo Bun com apps/cli, apps/www (Astro scaffold) e apps/docs (Mintlify scaffold), com oxlint e oxfmt configurados na raiz.

**Architecture:** Bun workspaces na raiz agrupa `apps/*` e `packages/*`. O código bash migra para `apps/cli/`. `apps/www` é scaffoldado via `bun create astro` e configurado com os integrações Astro (Tailwind CSS 4, React, Node adapter). `apps/docs` é scaffoldado via `mint new` e tem seu `docs.json` substituído pela navegação definitiva + stubs MDX criados.

**Tech Stack:** Bun (workspaces), Astro 5 (`bun create astro` + `astro add`), Tailwind CSS 4 (via `astro add tailwind`), `@astrojs/react` (via `astro add react`), `@astrojs/node` (via `astro add node`), TypeScript strict, oxlint (via `oxlint --init`), oxfmt (formatter do Oxc — pacote `oxfmt`)

---

## Mapa de arquivos

### Criar
- `package.json` — root workspace Bun com scripts de lint/format
- `bunfig.toml` — config global do Bun
- `packages/.gitkeep` — mantém diretório no git
- `apps/cli/package.json` — workspace entry do CLI (sem deps JS)
- Gerados por `bun create astro apps/www` → modificar apenas:
  - `apps/www/astro.config.mjs` — adicionar `output: 'server'`, confirmar adapter/React/Tailwind
  - `apps/www/tsconfig.json` — garantir strict mode
  - `apps/www/src/styles/global.css` — `@import "tailwindcss"`
  - `apps/www/src/pages/index.astro` — substituir por placeholder mínimo
- Gerados por `mint new apps/docs` → substituir/criar:
  - `apps/docs/docs.json` — navegação e tema definitivos
  - 13 arquivos `.mdx` stub (deletar exemplos do starter, criar os nossos)

### Mover
- `src/` → `apps/cli/src/`
- `tests/` → `apps/cli/tests/`
- `install.sh` → `apps/cli/install.sh`
- `VERSION` → `apps/cli/VERSION`

### Modificar
- `release.sh` — atualizar todos os paths para `apps/cli/`
- `.github/workflows/release.yml` — atualizar paths para `apps/cli/`
- `.github/workflows/auto-tag.yml` — atualizar path do `VERSION`
- `.gitignore` — adicionar `.superpowers/`, `node_modules/`, `.astro/`, `dist/`

---

## Task 1: Monorepo root — Bun workspace

**Files:**
- Create: `package.json`
- Create: `bunfig.toml`
- Create: `packages/.gitkeep`
- Modify: `.gitignore`

- [ ] **Step 1: Criar package.json raiz**

```json
{
  "name": "pr-tools",
  "private": true,
  "workspaces": ["apps/*", "packages/*"],
  "scripts": {
    "dev:www": "bun --filter @pr-tools/www dev",
    "build:www": "bun --filter @pr-tools/www build"
  }
}
```

> Scripts de lint/format serão adicionados na Task 2 após instalar as ferramentas.

- [ ] **Step 2: Criar bunfig.toml**

```toml
[install]
link-native-bins = true
```

- [ ] **Step 3: Criar packages/.gitkeep**

```bash
mkdir -p packages && touch packages/.gitkeep
```

- [ ] **Step 4: Atualizar .gitignore**

Adicionar ao final do `.gitignore` existente:

```
node_modules/
.astro/
dist/
.superpowers/
```

- [ ] **Step 5: Verificar e commitar**

```bash
bun install
git add package.json bunfig.toml packages/.gitkeep .gitignore
git commit -m "chore: initialize bun monorepo workspace"
```

Expected: `bun install` completa sem erros (workspaces ainda vazios, normal).

---

## Task 2: Configurar oxlint + oxfmt na raiz

**Files:**
- Create: `.oxlintrc.json` (gerado por `oxlint --init`)
- Modify: `package.json` (adicionar scripts lint/format)

- [ ] **Step 1: Instalar oxlint e oxfmt**

```bash
bun add -D oxlint oxfmt
```

- [ ] **Step 2: Inicializar config do oxlint**

```bash
bunx oxlint --init
```

Expected: cria `.oxlintrc.json` na raiz com config base.

- [ ] **Step 3: Editar .oxlintrc.json para ignorar apps/cli**

Abrir `.oxlintrc.json` gerado e atualizar:

```json
{
  "$schema": "https://raw.githubusercontent.com/oxc-project/oxc/main/npm/oxlint/configuration_schema.json",
  "ignorePatterns": [
    "node_modules",
    "dist",
    ".astro",
    "apps/cli"
  ],
  "rules": {}
}
```

> `apps/cli` é ignorado pois contém apenas bash scripts.

- [ ] **Step 4: Adicionar scripts de lint e format ao package.json**

```json
{
  "name": "pr-tools",
  "private": true,
  "workspaces": ["apps/*", "packages/*"],
  "scripts": {
    "lint": "oxlint .",
    "lint:check": "oxlint . --deny-warnings",
    "format": "oxfmt .",
    "format:check": "oxfmt --check .",
    "dev:www": "bun --filter @pr-tools/www dev",
    "build:www": "bun --filter @pr-tools/www build"
  }
}
```

- [ ] **Step 5: Verificar que lint e format rodam**

```bash
bun lint
bun format
```

Expected: ambos executam sem erros de configuração.

- [ ] **Step 6: Commit**

```bash
git add .oxlintrc.json package.json bun.lock
git commit -m "chore: add oxlint and oxfmt to monorepo root"
```

---

## Task 3: Migrar CLI para apps/cli

**Files:**
- Create: `apps/cli/package.json`
- Move: `src/` → `apps/cli/src/`
- Move: `tests/` → `apps/cli/tests/`
- Move: `install.sh` → `apps/cli/install.sh`
- Move: `VERSION` → `apps/cli/VERSION`

- [ ] **Step 1: Criar apps/cli com package.json**

```bash
mkdir -p apps/cli
```

```json
{
  "name": "@pr-tools/cli",
  "private": true,
  "version": "0.0.0",
  "description": "CLI bash para gerar descrições de PR e cards de teste no Azure DevOps"
}
```

> Sem scripts JS — o CLI é bash puro. O package.json existe para o workspace Bun reconhecer o app.

- [ ] **Step 2: Mover arquivos com git mv**

```bash
git mv src apps/cli/src
git mv tests apps/cli/tests
git mv install.sh apps/cli/install.sh
git mv VERSION apps/cli/VERSION
```

- [ ] **Step 3: Verificar estrutura**

```bash
ls apps/cli/
```

Expected: `install.sh  package.json  src/  tests/  VERSION`

```bash
ls apps/cli/src/bin/
```

Expected: `create-pr-description  create-test-card  azure.sh  common.sh  llm.sh  test-card-azure.sh  test-card-llm.sh  ui.sh  create-test-card-azure-patch.sh  create-test-card-steps.sh`

- [ ] **Step 4: Verificar sintaxe dos scripts bash**

```bash
bash -n apps/cli/src/bin/create-pr-description
bash -n apps/cli/src/bin/create-test-card
```

Expected: sem erros de sintaxe em ambos.

- [ ] **Step 5: Commit**

```bash
git add apps/cli/
git commit -m "chore: migrate cli code to apps/cli"
```

---

## Task 4: Atualizar release.sh e workflows para novos paths

**Files:**
- Modify: `release.sh`
- Modify: `.github/workflows/release.yml`
- Modify: `.github/workflows/auto-tag.yml`

- [ ] **Step 1: Atualizar release.sh — VERSION**

Localizar e substituir:

```bash
# ANTES
CURRENT_VERSION="$(cat "$SCRIPT_DIR/VERSION" 2>/dev/null || echo "unknown")"
```
```bash
# DEPOIS
CURRENT_VERSION="$(cat "$SCRIPT_DIR/apps/cli/VERSION" 2>/dev/null || echo "unknown")"
```

```bash
# ANTES
printf '%s\n' "$VERSION" > "$SCRIPT_DIR/VERSION"
```
```bash
# DEPOIS
printf '%s\n' "$VERSION" > "$SCRIPT_DIR/apps/cli/VERSION"
```

- [ ] **Step 2: Atualizar release.sh — loop dos scripts CLI**

Localizar e substituir:

```bash
# ANTES
for script in "$SCRIPT_DIR/src/bin/create-pr-description" "$SCRIPT_DIR/src/bin/create-test-card"; do
```
```bash
# DEPOIS
for script in "$SCRIPT_DIR/apps/cli/src/bin/create-pr-description" "$SCRIPT_DIR/apps/cli/src/bin/create-test-card"; do
```

- [ ] **Step 3: Atualizar release.sh — git add**

Localizar e substituir:

```bash
# ANTES
git add VERSION src/bin/create-pr-description src/bin/create-test-card CHANGELOG.md
```
```bash
# DEPOIS
git add apps/cli/VERSION apps/cli/src/bin/create-pr-description apps/cli/src/bin/create-test-card CHANGELOG.md
```

- [ ] **Step 4: Atualizar release.yml — Package release assets**

Localizar o bloco `run` do step `Package release assets` e substituir:

```yaml
# ANTES
cp install.sh "$DIST_DIR/"
cp src/bin/create-pr-description "$DIST_DIR/bin/"
cp src/bin/create-test-card "$DIST_DIR/bin/"
cp src/lib/*.sh "$DIST_DIR/lib/"
```
```yaml
# DEPOIS
cp apps/cli/install.sh "$DIST_DIR/"
cp apps/cli/src/bin/create-pr-description "$DIST_DIR/bin/"
cp apps/cli/src/bin/create-test-card "$DIST_DIR/bin/"
cp apps/cli/src/lib/*.sh "$DIST_DIR/lib/"
```

- [ ] **Step 5: Atualizar auto-tag.yml — leitura do VERSION**

Localizar e substituir:

```yaml
# ANTES
FILE_VERSION="$(cat VERSION | tr -d '[:space:]')"
```
```yaml
# DEPOIS
FILE_VERSION="$(cat apps/cli/VERSION | tr -d '[:space:]')"
```

- [ ] **Step 6: Verificar sintaxe do release.sh**

```bash
bash -n release.sh
```

Expected: sem erros de sintaxe.

- [ ] **Step 7: Commit**

```bash
git add release.sh .github/workflows/release.yml .github/workflows/auto-tag.yml
git commit -m "chore: update release paths to apps/cli"
```

---

## Task 5: Scaffold apps/www com Astro CLI

**Files (todos gerados pela CLI — apenas modificar conforme indicado):**
- Modify: `apps/www/astro.config.mjs`
- Modify: `apps/www/tsconfig.json`
- Modify: `apps/www/src/pages/index.astro`
- Create: `apps/www/src/styles/global.css`

- [ ] **Step 1: Criar projeto Astro com Bun**

```bash
bun create astro@latest apps/www
```

Durante o wizard interativo, selecionar:
- Template: **Empty** (minimal starter)
- TypeScript: **Strict**
- Install dependencies: **Yes**

- [ ] **Step 2: Adicionar Tailwind CSS 4**

```bash
cd apps/www && bunx astro add tailwind
```

Expected: instala `@tailwindcss/vite` e `tailwindcss`, atualiza `astro.config.mjs` automaticamente.

- [ ] **Step 3: Adicionar integração React**

```bash
bunx astro add react
```

Expected: instala `@astrojs/react`, `react`, `react-dom` e atualiza `astro.config.mjs`.

- [ ] **Step 4: Adicionar adapter Node.js para SSR**

```bash
bunx astro add node
```

Expected: instala `@astrojs/node` e atualiza `astro.config.mjs`.

- [ ] **Step 5: Garantir output SSR no astro.config.mjs**

Abrir `apps/www/astro.config.mjs` gerado. Verificar se `output: 'server'` está presente e o adapter node está configurado com `mode: 'standalone'`. Se não estiver, ajustar para:

```js
// apps/www/astro.config.mjs
import { defineConfig } from 'astro/config';
import tailwindcss from '@tailwindcss/vite';
import react from '@astrojs/react';
import node from '@astrojs/node';

export default defineConfig({
  output: 'server',
  adapter: node({ mode: 'standalone' }),
  integrations: [react()],
  vite: {
    plugins: [tailwindcss()],
  },
});
```

> A estrutura exata pode variar conforme o que `astro add` gerou. O objetivo é garantir que `output: 'server'`, o adapter node com `mode: 'standalone'`, o React e o Tailwind estejam todos configurados.

- [ ] **Step 6: Criar src/styles/global.css**

```bash
mkdir -p src/styles
```

```css
/* apps/www/src/styles/global.css */
@import "tailwindcss";
```

- [ ] **Step 7: Substituir index.astro por placeholder mínimo**

```astro
---
// apps/www/src/pages/index.astro
// Landing page — design a implementar em fase posterior.
// Seções planejadas: Nav, Hero, Demo, Features, Providers, Install, Newsletter, Footer.
// Estilo: dark minimal, accent #7c3aed, inspiração Linear/Vercel.
import '../styles/global.css';
---
<html lang="pt-BR">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>pr-tools</title>
  </head>
  <body class="bg-[#0d0d0d] text-[#f8f8f8] font-sans">
    <main class="flex min-h-screen items-center justify-center">
      <p class="text-[#7c3aed] font-mono text-lg">pr-tools — em construção</p>
    </main>
  </body>
</html>
```

- [ ] **Step 8: Verificar que o dev server sobe**

```bash
bun dev
```

Expected: Astro inicia em `http://localhost:4321`. Página exibe "pr-tools — em construção" com fundo escuro.

- [ ] **Step 9: Commit**

```bash
cd ../..
git add apps/www/
git commit -m "feat: scaffold apps/www with Astro 5, Tailwind CSS 4 and React"
```

---

## Task 6: Scaffold apps/docs com Mintlify CLI

**Files:**
- Modify: `apps/docs/docs.json` (substituir conteúdo do starter pelo definitivo)
- Delete: arquivos de exemplo do starter kit
- Create: 13 arquivos `.mdx` stub

> O config do Mintlify agora usa `docs.json` (não `mint.json`).
> `apps/docs` não tem `package.json` — Mintlify é serviço externo com integração GitHub.

- [ ] **Step 1: Instalar Mintlify CLI globalmente**

```bash
bun add -g mint
```

- [ ] **Step 2: Criar projeto Mintlify**

Da raiz do repositório:

```bash
mint new apps/docs
```

Expected: clona o starter kit do Mintlify em `apps/docs/` com `docs.json` e páginas de exemplo.

- [ ] **Step 3: Limpar arquivos de exemplo do starter**

```bash
cd apps/docs
# Remover todo conteúdo MDX de exemplo (manter apenas docs.json)
find . -name "*.mdx" -not -path "./.git/*" -delete
find . -name "*.png" -not -path "./.git/*" -delete 2>/dev/null || true
find . -name "*.svg" -not -path "./.git/*" -delete 2>/dev/null || true
```

- [ ] **Step 4: Substituir docs.json pela configuração definitiva**

```json
{
  "$schema": "https://mintlify.com/docs.json",
  "theme": "mint",
  "name": "pr-tools",
  "colors": {
    "primary": "#7c3aed",
    "light": "#a78bfa",
    "dark": "#5b21b6"
  },
  "topbarLinks": [
    {
      "name": "GitHub",
      "url": "https://github.com/nitoba/pr-tools"
    }
  ],
  "topbarCtaButton": {
    "name": "Instalar",
    "url": "https://pr-tools.dev"
  },
  "navigation": [
    {
      "group": "Primeiros passos",
      "pages": [
        "getting-started/introduction",
        "getting-started/installation",
        "getting-started/quickstart",
        "getting-started/configuration"
      ]
    },
    {
      "group": "Comandos",
      "pages": [
        "commands/create-pr-description",
        "commands/create-test-card"
      ]
    },
    {
      "group": "Guias",
      "pages": [
        "guides/azure-devops",
        "guides/ai-providers",
        "guides/markdown-rendering",
        "guides/advanced-examples"
      ]
    },
    {
      "group": "Referência",
      "pages": [
        "reference/environment-variables",
        "reference/troubleshooting",
        "reference/changelog"
      ]
    }
  ],
  "footerSocials": {
    "github": "https://github.com/nitoba/pr-tools"
  }
}
```

- [ ] **Step 5: Criar stubs de Primeiros passos**

```bash
mkdir -p getting-started
```

`getting-started/introduction.mdx`:
```mdx
---
title: "Introdução"
description: "O que é o pr-tools e para quem é indicado"
---

# Introdução

Conteúdo em breve.
```

`getting-started/installation.mdx`:
```mdx
---
title: "Instalação"
description: "Como instalar o pr-tools em macOS, Linux e Windows WSL"
---

# Instalação

Conteúdo em breve.
```

`getting-started/quickstart.mdx`:
```mdx
---
title: "Quickstart"
description: "Gere seu primeiro PR em menos de 5 minutos"
---

# Quickstart

Conteúdo em breve.
```

`getting-started/configuration.mdx`:
```mdx
---
title: "Configuração"
description: "Como configurar API keys, providers e defaults"
---

# Configuração

Conteúdo em breve.
```

- [ ] **Step 6: Criar stubs de Comandos**

```bash
mkdir -p commands
```

`commands/create-pr-description.mdx`:
```mdx
---
title: "create-pr-description"
description: "Gera descrições de PR via LLM a partir do git diff"
---

# create-pr-description

Conteúdo em breve.
```

`commands/create-test-card.mdx`:
```mdx
---
title: "create-test-card"
description: "Gera cards de teste a partir de PR e Work Item do Azure DevOps"
---

# create-test-card

Conteúdo em breve.
```

- [ ] **Step 7: Criar stubs de Guias**

```bash
mkdir -p guides
```

`guides/azure-devops.mdx`:
```mdx
---
title: "Configurando o Azure DevOps"
description: "Como configurar o PAT e as permissões necessárias"
---

# Configurando o Azure DevOps

Conteúdo em breve.
```

`guides/ai-providers.mdx`:
```mdx
---
title: "Escolhendo providers de IA"
description: "Comparação entre OpenRouter, Groq e Google Gemini"
---

# Escolhendo providers de IA

Conteúdo em breve.
```

`guides/markdown-rendering.mdx`:
```mdx
---
title: "Renderizando Markdown no terminal"
description: "Como usar glow, bat ou texto puro para visualizar a saída"
---

# Renderizando Markdown no terminal

Conteúdo em breve.
```

`guides/advanced-examples.mdx`:
```mdx
---
title: "Exemplos avançados"
description: "Casos de uso avançados com flags e variáveis de ambiente"
---

# Exemplos avançados

Conteúdo em breve.
```

- [ ] **Step 8: Criar stubs de Referência**

```bash
mkdir -p reference
```

`reference/environment-variables.mdx`:
```mdx
---
title: "Variáveis de ambiente"
description: "Referência completa de todas as variáveis configuráveis"
---

# Variáveis de ambiente

Conteúdo em breve.
```

`reference/troubleshooting.mdx`:
```mdx
---
title: "Troubleshooting"
description: "Soluções para problemas comuns"
---

# Troubleshooting

Conteúdo em breve.
```

`reference/changelog.mdx`:
```mdx
---
title: "Changelog"
description: "Histórico de versões do pr-tools"
---

# Changelog

Conteúdo em breve.
```

- [ ] **Step 9: Verificar preview local**

```bash
mint dev
```

Expected: servidor Mintlify sobe em `http://localhost:3000` com a navegação definida e 13 páginas stub acessíveis.

- [ ] **Step 10: Commit**

```bash
cd ../..
git add apps/docs/
git commit -m "feat: scaffold apps/docs with Mintlify and MDX stubs"
```

---

## Task 7: Verificação final do monorepo

- [ ] **Step 1: Instalar todas as dependências do workspace**

```bash
bun install
```

Expected: completa sem erros.

- [ ] **Step 2: Rodar lint**

```bash
bun lint
```

Expected: oxlint roda sem erros de configuração.

- [ ] **Step 3: Verificar build do www**

```bash
bun build:www
```

Expected: Astro faz build do placeholder sem erros.

- [ ] **Step 4: Verificar scripts bash não corrompidos**

```bash
bash -n apps/cli/src/bin/create-pr-description
bash -n apps/cli/src/bin/create-test-card
```

Expected: sem erros de sintaxe.

- [ ] **Step 5: Verificar estrutura final**

```bash
find apps -maxdepth 2 -name "package.json" -o -name "docs.json" | sort
```

Expected:
```
apps/cli/package.json
apps/www/package.json
apps/docs/docs.json
```

- [ ] **Step 6: Commit de encerramento**

```bash
git add .
git status  # confirmar que não há arquivos inesperados
git commit -m "chore: monorepo foundation complete" --allow-empty
```
