# pr-tools Monorepo — Design Spec

**Data:** 2026-04-03  
**Status:** Aprovado

---

## Visão geral

Evolução do repositório `pr-tools` de um projeto flat para um monorepo gerenciado pelo Bun. Esta implementação cobre apenas a **fundação**: estrutura de diretórios, scaffolding dos apps e configuração de tooling. O conteúdo real de cada app (landing page, docs, newsletter) é implementado em fases posteriores.

---

## O que entra nesta implementação (fundação)

1. **Monorepo root** — Bun workspaces + tooling compartilhado (oxlint, oxformat)
2. **apps/cli** — migração do código existente (`src/`, `tests/`, `install.sh`, `VERSION`) para cá; atualização de paths em `release.sh` e workflows
3. **apps/www** — scaffold Astro 5 SSR com Tailwind CSS 4 + plugin React configurados, pronto para implementação do design
4. **apps/docs** — `mint.json` com navegação definida + arquivos `.mdx` vazios/stub por página

## O que NÃO entra agora (fases posteriores)

- Implementação do design e conteúdo do landing page
- Preenchimento do conteúdo das páginas MDX da documentação
- API route `/api/subscribe` e integração com Resend
- Workflow `newsletter.yml` e geração de conteúdo via LLM
- Template HTML do email

---

## Estrutura do monorepo

```
pr-tools/
├── apps/
│   ├── cli/              ← código do CLI (migrado de src/, tests/, install.sh, VERSION)
│   │   ├── src/
│   │   │   ├── bin/
│   │   │   └── lib/
│   │   ├── tests/
│   │   ├── install.sh
│   │   └── VERSION
│   ├── www/              ← landing page (Astro SSR — scaffold apenas)
│   │   ├── src/
│   │   │   ├── pages/
│   │   │   │   └── index.astro   ← placeholder
│   │   │   └── components/
│   │   ├── astro.config.mjs
│   │   ├── tailwind.config.ts
│   │   └── package.json
│   └── docs/             ← documentação (Mintlify — mint.json + stubs MDX)
│       ├── mint.json
│       ├── getting-started/
│       │   ├── introduction.mdx  ← stub
│       │   ├── installation.mdx  ← stub
│       │   ├── quickstart.mdx    ← stub
│       │   └── configuration.mdx ← stub
│       ├── commands/
│       │   ├── create-pr-description.mdx ← stub
│       │   └── create-test-card.mdx      ← stub
│       ├── guides/
│       │   ├── azure-devops.mdx          ← stub
│       │   ├── ai-providers.mdx          ← stub
│       │   ├── markdown-rendering.mdx    ← stub
│       │   └── advanced-examples.mdx     ← stub
│       └── reference/
│           ├── environment-variables.mdx ← stub
│           ├── troubleshooting.mdx       ← stub
│           └── changelog.mdx             ← stub
├── packages/             ← vazio, workspace válido para libs futuras
├── .github/
│   └── workflows/
│       ├── release.yml   ← caminhos atualizados para apps/cli
│       └── auto-tag.yml  ← caminhos atualizados para apps/cli
├── package.json          ← root Bun workspace
├── bunfig.toml
├── cliff.toml
├── CHANGELOG.md
├── release.sh            ← caminhos atualizados para apps/cli
└── LICENSE
```

**Decisões:**
- `src/`, `tests/`, `install.sh`, `VERSION` movidos para `apps/cli/` — raiz sem código do CLI
- `packages/` existe como workspace válido sem código (YAGNI)
- `release.sh` e workflows têm paths atualizados para `apps/cli/`
- O `install.sh` funciona desde que o raw URL do GitHub seja atualizado na documentação

---

## Bun workspaces (root package.json)

```json
{
  "name": "pr-tools",
  "private": true,
  "workspaces": ["apps/*", "packages/*"],
  "scripts": {
    "lint": "oxlint .",
    "format": "oxformat ."
  }
}
```

`bunfig.toml` na raiz com configurações globais de install.

---

## Tooling compartilhado (monorepo root)

### oxlint
Linter rápido da suite Oxc, configurado na raiz e aplicado a todos os apps TypeScript/JavaScript.
- Arquivo de config: `oxlint.json` na raiz
- Regras: base recomendada da Oxc
- Scripts: `bun lint` na raiz executa para todo o monorepo

### oxformat
Formatter da suite Oxc, configurado na raiz.
- Arquivo de config: `oxformat.json` (ou seção em `oxlint.json` conforme a API da versão em uso)
- Scripts: `bun format` na raiz executa para todo o monorepo

Ambos se aplicam aos arquivos `.ts`, `.tsx`, `.astro` e `.js` dentro de `apps/` e `packages/`.

---

## apps/www — Scaffold (Astro)

### Stack configurada nesta fase
- **Astro 5** com output `server` (SSR)
- **@astrojs/node** como adapter padrão (substituível por Cloudflare no deploy)
- **@astrojs/react** — plugin React habilitado para componentes interativos
- **Tailwind CSS 4** via `@astrojs/tailwind`
- **TypeScript** strict mode (`tsconfig.json`)
- `index.astro` com placeholder mínimo (sem layout implementado)

### O que fica para depois
- Implementação do design dark minimal (paleta, componentes, seções)
- API route `/api/subscribe`
- Template de email

### Referência de design (para fase posterior)
- Estilo: dark minimal, accent violet (`#7c3aed`), inspiração Linear/Vercel
- Seções planejadas: Nav, Hero, Demo terminal, Features, Providers, Instalação, Newsletter, Footer
- Paleta: bg `#0d0d0d`/`#0a0a0a`, texto `#f8f8f8`, muted `#6b7280`, borders `#1f1f1f`

---

## apps/docs — Scaffold (Mintlify)

`apps/docs` contém apenas `mint.json` e arquivos `.mdx` stub. Sem `package.json` — Mintlify é serviço externo com integração GitHub nativa apontando para `apps/docs/` como root.

### mint.json — navegação definida nesta fase

```json
{
  "name": "pr-tools",
  "colors": { "primary": "#7c3aed" },
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
  ]
}
```

### Stubs MDX
Cada página tem apenas o frontmatter com título, descrição e um `# Título` — suficiente para o Mintlify renderizar sem erro.

---

## Newsletter — Referência de design (fases posteriores)

### Inscrição
- `POST /api/subscribe` em `apps/www` → Resend Audiences API
- Resend gerencia unsubscribe (RFC 8058), bounce e suppression list nativamente

### Envio automatizado
- GitHub Actions trigger: `on: release: types: [published]`
- Idempotência: verifica broadcast `release-{tag}` existente antes de criar
- LLM gera conteúdo em **Markdown** (não HTML) — tone técnico, público dev
- Template HTML próprio do projeto converte Markdown e aplica design do email
- Resend Broadcast enviado para a audience de inscritos

### Secrets (a configurar no momento da implementação)
- `RESEND_API_KEY`
- `RESEND_AUDIENCE_ID`
- `LLM_API_KEY` (ex: `OPENROUTER_API_KEY`)

---

## O que NÃO muda
- Scripts bash do CLI — apenas movidos para `apps/cli/`
- Processo de release (`release.sh`, `auto-tag.yml`, `release.yml`) — apenas paths atualizados
- `CHANGELOG.md` e `cliff.toml` na raiz
