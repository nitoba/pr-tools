# Monorepo Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Converter o repositório flat em um monorepo Bun com apps/cli, apps/www (Astro scaffold) e apps/docs (Mintlify stubs), com oxlint e oxformat configurados na raiz.

**Architecture:** Bun workspaces na raiz agrupa `apps/*` e `packages/*`. O código bash existente migra para `apps/cli/` sem alterações funcionais. `apps/www` é um projeto Astro 5 SSR com Tailwind CSS 4 e React, sem conteúdo implementado. `apps/docs` é só `mint.json` + arquivos MDX stub — sem package.json, pois o Mintlify é um serviço externo.

**Tech Stack:** Bun (workspaces), Astro 5, Tailwind CSS 4 (`@tailwindcss/vite`), `@astrojs/react`, `@astrojs/node`, TypeScript strict, oxlint, oxformat (Oxc project)

---

## Mapa de arquivos

### Criar
- `package.json` — root workspace Bun
- `bunfig.toml` — config global do Bun
- `oxlint.json` — config do oxlint (raiz)
- `packages/.gitkeep` — mantém o diretório no git
- `apps/cli/package.json` — workspace entry para o CLI
- `apps/www/package.json` — dependências do Astro
- `apps/www/astro.config.mjs` — SSR + Tailwind + React + Node adapter
- `apps/www/src/env.d.ts` — tipos do Astro
- `apps/www/src/pages/index.astro` — placeholder
- `apps/www/src/styles/global.css` — `@import "tailwindcss"`
- `apps/www/tsconfig.json` — TypeScript strict
- `apps/docs/mint.json` — navegação e tema Mintlify
- `apps/docs/getting-started/introduction.mdx`
- `apps/docs/getting-started/installation.mdx`
- `apps/docs/getting-started/quickstart.mdx`
- `apps/docs/getting-started/configuration.mdx`
- `apps/docs/commands/create-pr-description.mdx`
- `apps/docs/commands/create-test-card.mdx`
- `apps/docs/guides/azure-devops.mdx`
- `apps/docs/guides/ai-providers.mdx`
- `apps/docs/guides/markdown-rendering.mdx`
- `apps/docs/guides/advanced-examples.mdx`
- `apps/docs/reference/environment-variables.mdx`
- `apps/docs/reference/troubleshooting.mdx`
- `apps/docs/reference/changelog.mdx`

### Mover
- `src/` → `apps/cli/src/`
- `tests/` → `apps/cli/tests/`
- `install.sh` → `apps/cli/install.sh`
- `VERSION` → `apps/cli/VERSION`

### Modificar
- `release.sh` — atualizar todos os paths para `apps/cli/`
- `.github/workflows/release.yml` — atualizar paths para `apps/cli/`
- `.github/workflows/auto-tag.yml` — atualizar path do `VERSION`
- `.gitignore` — adicionar `.superpowers/`, `node_modules/`, `.astro/`

---

## Task 1: Monorepo root — Bun workspace

**Files:**
- Create: `package.json`
- Create: `bunfig.toml`
- Modify: `.gitignore`

- [ ] **Step 1: Criar package.json raiz**

```json
{
  "name": "pr-tools",
  "private": true,
  "workspaces": ["apps/*", "packages/*"],
  "scripts": {
    "lint": "oxlint .",
    "format": "oxformat .",
    "dev:www": "bun --filter @pr-tools/www dev",
    "build:www": "bun --filter @pr-tools/www build"
  }
}
```

- [ ] **Step 2: Criar bunfig.toml**

```toml
[install]
# Usa node_modules linkado (compatível com Astro e Tailwind)
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

- [ ] **Step 5: Verificar**

```bash
bun install
```

Expected: Bun instala workspaces sem erros. Pode mostrar `workspace:apps/cli`, `workspace:apps/www` pendentes — normal, serão criados nas próximas tasks.

- [ ] **Step 6: Commit**

```bash
git add package.json bunfig.toml packages/.gitkeep .gitignore
git commit -m "chore: initialize bun monorepo workspace"
```

---

## Task 2: Configurar oxlint

**Files:**
- Create: `oxlint.json`

- [ ] **Step 1: Instalar oxlint**

```bash
bun add -D oxlint
```

- [ ] **Step 2: Criar oxlint.json na raiz**

```json
{
  "$schema": "https://raw.githubusercontent.com/oxc-project/oxc/main/npm/oxlint/configuration_schema.json",
  "plugins": [],
  "rules": {},
  "ignorePatterns": [
    "node_modules",
    "dist",
    ".astro",
    "apps/cli"
  ]
}
```

> `apps/cli` é ignorado pois contém apenas bash scripts, não TypeScript/JavaScript.

- [ ] **Step 3: Verificar que oxlint roda**

```bash
bun lint
```

Expected: `oxlint .` executa sem erros de configuração. Pode não encontrar arquivos TS ainda — ok.

- [ ] **Step 4: Commit**

```bash
git add oxlint.json package.json bun.lock
git commit -m "chore: add oxlint to monorepo root"
```

---

## Task 3: Configurar oxformat

**Files:**
- Create: `.oxlintrc.json` ou config dedicada conforme versão disponível

- [ ] **Step 1: Verificar disponibilidade do oxformat**

```bash
npm info oxformat
```

Se disponível: `bun add -D oxformat`  
Se não disponível (pacote ainda em desenvolvimento): usar `@biomejs/biome` apenas para formatação e anotar no README que oxformat será adotado quando estabilizar.

- [ ] **Step 2a: Se oxformat disponível — instalar e configurar**

```bash
bun add -D oxformat
```

Criar `oxformat.json` na raiz:

```json
{
  "ignore": ["node_modules", "dist", ".astro", "apps/cli"]
}
```

- [ ] **Step 2b: Se oxformat NÃO disponível — usar Biome para formatação**

```bash
bun add -D @biomejs/biome
bunx biome init
```

Atualizar `package.json` — script format:

```json
{
  "scripts": {
    "lint": "oxlint .",
    "format": "biome format --write .",
    "dev:www": "bun --filter @pr-tools/www dev",
    "build:www": "bun --filter @pr-tools/www build"
  }
}
```

Criar `biome.json` na raiz:

```json
{
  "$schema": "https://biomejs.dev/schemas/1.9.0/schema.json",
  "formatter": {
    "enabled": true,
    "indentStyle": "space",
    "indentWidth": 2
  },
  "linter": {
    "enabled": false
  },
  "files": {
    "ignore": ["node_modules", "dist", ".astro", "apps/cli"]
  }
}
```

> oxlint cuida do linting; Biome cuida apenas de formatação neste cenário.

- [ ] **Step 3: Verificar que o formatter roda**

```bash
bun format
```

Expected: formata arquivos `.ts`, `.astro`, `.js` sem erros.

- [ ] **Step 4: Commit**

```bash
git add . && git commit -m "chore: add formatter to monorepo root"
```

---

## Task 4: Migrar código do CLI para apps/cli

**Files:**
- Create: `apps/cli/package.json`
- Move: `src/` → `apps/cli/src/`
- Move: `tests/` → `apps/cli/tests/`
- Move: `install.sh` → `apps/cli/install.sh`
- Move: `VERSION` → `apps/cli/VERSION`

- [ ] **Step 1: Criar diretório apps/cli**

```bash
mkdir -p apps/cli
```

- [ ] **Step 2: Criar apps/cli/package.json**

```json
{
  "name": "@pr-tools/cli",
  "private": true,
  "version": "0.0.0",
  "description": "CLI bash para gerar descrições de PR e cards de teste no Azure DevOps"
}
```

> Sem scripts JS — o CLI é bash puro. O package.json existe apenas para o workspace Bun reconhecer o app.

- [ ] **Step 3: Mover src/, tests/, install.sh e VERSION**

```bash
git mv src apps/cli/src
git mv tests apps/cli/tests
git mv install.sh apps/cli/install.sh
git mv VERSION apps/cli/VERSION
```

- [ ] **Step 4: Verificar estrutura**

```bash
ls apps/cli/
```

Expected:
```
install.sh  package.json  src/  tests/  VERSION
```

```bash
ls apps/cli/src/bin/
```

Expected: `create-pr-description  create-test-card  azure.sh  common.sh  llm.sh  ...`

- [ ] **Step 5: Commit**

```bash
git add apps/cli/ package.json
git commit -m "chore: migrate cli code to apps/cli"
```

---

## Task 5: Atualizar release.sh e workflows para novos paths

**Files:**
- Modify: `release.sh`
- Modify: `.github/workflows/release.yml`
- Modify: `.github/workflows/auto-tag.yml`

- [ ] **Step 1: Atualizar release.sh — leitura do VERSION**

Em `release.sh`, localizar:
```bash
CURRENT_VERSION="$(cat "$SCRIPT_DIR/VERSION" 2>/dev/null || echo "unknown")"
```
Substituir por:
```bash
CURRENT_VERSION="$(cat "$SCRIPT_DIR/apps/cli/VERSION" 2>/dev/null || echo "unknown")"
```

Localizar:
```bash
printf '%s\n' "$VERSION" > "$SCRIPT_DIR/VERSION"
```
Substituir por:
```bash
printf '%s\n' "$VERSION" > "$SCRIPT_DIR/apps/cli/VERSION"
```

- [ ] **Step 2: Atualizar release.sh — paths dos scripts CLI**

Localizar:
```bash
for script in "$SCRIPT_DIR/src/bin/create-pr-description" "$SCRIPT_DIR/src/bin/create-test-card"; do
```
Substituir por:
```bash
for script in "$SCRIPT_DIR/apps/cli/src/bin/create-pr-description" "$SCRIPT_DIR/apps/cli/src/bin/create-test-card"; do
```

- [ ] **Step 3: Atualizar release.sh — git add**

Localizar:
```bash
git add VERSION src/bin/create-pr-description src/bin/create-test-card CHANGELOG.md
```
Substituir por:
```bash
git add apps/cli/VERSION apps/cli/src/bin/create-pr-description apps/cli/src/bin/create-test-card CHANGELOG.md
```

- [ ] **Step 4: Atualizar release.yml — Package release assets**

Localizar o step `Package release assets` em `.github/workflows/release.yml`:

```yaml
- name: Package release assets
  run: |
    TAG=${GITHUB_REF#refs/tags/}
    DIST_DIR="pr-tools-$TAG"
    mkdir -p "$DIST_DIR/bin" "$DIST_DIR/lib"

    cp install.sh "$DIST_DIR/"
    cp src/bin/create-pr-description "$DIST_DIR/bin/"
    cp src/bin/create-test-card "$DIST_DIR/bin/"
    cp src/lib/*.sh "$DIST_DIR/lib/"

    zip -r "$DIST_DIR.zip" "$DIST_DIR"
    echo "DIST_FILE=$DIST_DIR.zip" >> "$GITHUB_ENV"
```

Substituir por:

```yaml
- name: Package release assets
  run: |
    TAG=${GITHUB_REF#refs/tags/}
    DIST_DIR="pr-tools-$TAG"
    mkdir -p "$DIST_DIR/bin" "$DIST_DIR/lib"

    cp apps/cli/install.sh "$DIST_DIR/"
    cp apps/cli/src/bin/create-pr-description "$DIST_DIR/bin/"
    cp apps/cli/src/bin/create-test-card "$DIST_DIR/bin/"
    cp apps/cli/src/lib/*.sh "$DIST_DIR/lib/"

    zip -r "$DIST_DIR.zip" "$DIST_DIR"
    echo "DIST_FILE=$DIST_DIR.zip" >> "$GITHUB_ENV"
```

- [ ] **Step 5: Atualizar auto-tag.yml — leitura do VERSION**

Localizar o step `Verify VERSION file matches` em `.github/workflows/auto-tag.yml`:

```yaml
- name: Verify VERSION file matches
  run: |
    FILE_VERSION="$(cat VERSION | tr -d '[:space:]')"
```

Substituir por:

```yaml
- name: Verify VERSION file matches
  run: |
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

## Task 6: Scaffold apps/www (Astro 5 + Tailwind 4 + React)

**Files:**
- Create: `apps/www/package.json`
- Create: `apps/www/astro.config.mjs`
- Create: `apps/www/tsconfig.json`
- Create: `apps/www/src/env.d.ts`
- Create: `apps/www/src/pages/index.astro`
- Create: `apps/www/src/styles/global.css`

- [ ] **Step 1: Criar package.json do apps/www**

```json
{
  "name": "@pr-tools/www",
  "private": true,
  "version": "0.0.0",
  "type": "module",
  "scripts": {
    "dev": "astro dev",
    "build": "astro build",
    "preview": "astro preview"
  }
}
```

- [ ] **Step 2: Instalar dependências do apps/www**

```bash
cd apps/www && bun add astro @astrojs/node @astrojs/react react react-dom @tailwindcss/vite tailwindcss
bun add -D typescript @types/react @types/react-dom
```

- [ ] **Step 3: Criar astro.config.mjs**

```js
// apps/www/astro.config.mjs
import { defineConfig } from 'astro/config';
import node from '@astrojs/node';
import react from '@astrojs/react';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  output: 'server',
  adapter: node({ mode: 'standalone' }),
  integrations: [react()],
  vite: {
    plugins: [tailwindcss()],
  },
});
```

- [ ] **Step 4: Criar tsconfig.json**

```json
{
  "extends": "astro/tsconfigs/strict",
  "compilerOptions": {
    "strictNullChecks": true,
    "baseUrl": "."
  }
}
```

- [ ] **Step 5: Criar src/env.d.ts**

```ts
/// <reference path="../.astro/types.d.ts" />
```

- [ ] **Step 6: Criar src/styles/global.css**

```css
@import "tailwindcss";
```

- [ ] **Step 7: Criar src/pages/index.astro (placeholder)**

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
cd apps/www && bun dev
```

Expected: Astro inicia em `http://localhost:4321` sem erros. Página exibe "pr-tools — em construção" com fundo escuro.

- [ ] **Step 9: Commit**

```bash
cd ../..
git add apps/www/
git commit -m "feat: scaffold apps/www with Astro 5, Tailwind CSS 4 and React"
```

---

## Task 7: Scaffold apps/docs (Mintlify)

**Files:**
- Create: `apps/docs/mint.json`
- Create: 13 arquivos MDX stub (listados abaixo)

> `apps/docs` não tem `package.json` — o Mintlify é serviço externo com integração GitHub.

- [ ] **Step 1: Criar apps/docs/mint.json**

```json
{
  "name": "pr-tools",
  "logo": {
    "dark": "/logo/dark.svg",
    "light": "/logo/light.svg"
  },
  "favicon": "/favicon.svg",
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

- [ ] **Step 2: Criar stubs de Primeiros passos**

`apps/docs/getting-started/introduction.mdx`:
```mdx
---
title: "Introdução"
description: "O que é o pr-tools e para quem é indicado"
---

# Introdução

Conteúdo em breve.
```

`apps/docs/getting-started/installation.mdx`:
```mdx
---
title: "Instalação"
description: "Como instalar o pr-tools em macOS, Linux e Windows WSL"
---

# Instalação

Conteúdo em breve.
```

`apps/docs/getting-started/quickstart.mdx`:
```mdx
---
title: "Quickstart"
description: "Gere seu primeiro PR em menos de 5 minutos"
---

# Quickstart

Conteúdo em breve.
```

`apps/docs/getting-started/configuration.mdx`:
```mdx
---
title: "Configuração"
description: "Como configurar API keys, providers e defaults"
---

# Configuração

Conteúdo em breve.
```

- [ ] **Step 3: Criar stubs de Comandos**

`apps/docs/commands/create-pr-description.mdx`:
```mdx
---
title: "create-pr-description"
description: "Gera descrições de PR via LLM a partir do git diff"
---

# create-pr-description

Conteúdo em breve.
```

`apps/docs/commands/create-test-card.mdx`:
```mdx
---
title: "create-test-card"
description: "Gera cards de teste a partir de PR e Work Item do Azure DevOps"
---

# create-test-card

Conteúdo em breve.
```

- [ ] **Step 4: Criar stubs de Guias**

`apps/docs/guides/azure-devops.mdx`:
```mdx
---
title: "Configurando o Azure DevOps"
description: "Como configurar o PAT e as permissões necessárias"
---

# Configurando o Azure DevOps

Conteúdo em breve.
```

`apps/docs/guides/ai-providers.mdx`:
```mdx
---
title: "Escolhendo providers de IA"
description: "Comparação entre OpenRouter, Groq e Google Gemini"
---

# Escolhendo providers de IA

Conteúdo em breve.
```

`apps/docs/guides/markdown-rendering.mdx`:
```mdx
---
title: "Renderizando Markdown no terminal"
description: "Como usar glow, bat ou texto puro para visualizar a saída"
---

# Renderizando Markdown no terminal

Conteúdo em breve.
```

`apps/docs/guides/advanced-examples.mdx`:
```mdx
---
title: "Exemplos avançados"
description: "Casos de uso avançados com flags e variáveis de ambiente"
---

# Exemplos avançados

Conteúdo em breve.
```

- [ ] **Step 5: Criar stubs de Referência**

`apps/docs/reference/environment-variables.mdx`:
```mdx
---
title: "Variáveis de ambiente"
description: "Referência completa de todas as variáveis configuráveis"
---

# Variáveis de ambiente

Conteúdo em breve.
```

`apps/docs/reference/troubleshooting.mdx`:
```mdx
---
title: "Troubleshooting"
description: "Soluções para problemas comuns"
---

# Troubleshooting

Conteúdo em breve.
```

`apps/docs/reference/changelog.mdx`:
```mdx
---
title: "Changelog"
description: "Histórico de versões do pr-tools"
---

# Changelog

Conteúdo em breve.
```

- [ ] **Step 6: Verificar estrutura**

```bash
find apps/docs -name "*.mdx" | sort
```

Expected: 13 arquivos MDX listados.

```bash
cat apps/docs/mint.json | jq '.navigation | length'
```

Expected: `4`

- [ ] **Step 7: Commit**

```bash
git add apps/docs/
git commit -m "feat: scaffold apps/docs with Mintlify config and MDX stubs"
```

---

## Task 8: Verificação final do monorepo

- [ ] **Step 1: Instalar todas as dependências do workspace**

```bash
bun install
```

Expected: todas as dependências instaladas sem erros.

- [ ] **Step 2: Verificar workspaces reconhecidos**

```bash
bun workspaces list 2>/dev/null || bun pm ls
```

Expected: `@pr-tools/cli` e `@pr-tools/www` listados como workspaces.

- [ ] **Step 3: Rodar lint no monorepo**

```bash
bun lint
```

Expected: oxlint roda sem erros de configuração. Pode não encontrar problemas — ok.

- [ ] **Step 4: Verificar que apps/www builda**

```bash
bun build:www
```

Expected: Astro faz build do site placeholder sem erros em `apps/www/dist/`.

- [ ] **Step 5: Verificar que os scripts bash do CLI não foram corrompidos**

```bash
bash -n apps/cli/src/bin/create-pr-description
bash -n apps/cli/src/bin/create-test-card
```

Expected: sem erros de sintaxe em ambos.

- [ ] **Step 6: Commit final**

```bash
git add .
git commit -m "chore: monorepo foundation complete"
```
