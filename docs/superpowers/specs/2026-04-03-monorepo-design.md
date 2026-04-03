# pr-tools Monorepo — Design Spec

**Data:** 2026-04-03  
**Status:** Aprovado

---

## Visão geral

Evolução do repositório `pr-tools` de um projeto flat (scripts bash na raiz) para um monorepo gerenciado pelo Bun, adicionando um landing page (Astro), documentação (Mintlify) e uma newsletter automatizada (Resend + GitHub Actions).

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
│   ├── www/              ← landing page (Astro SSR)
│   │   ├── src/
│   │   │   ├── pages/
│   │   │   │   ├── index.astro
│   │   │   │   └── api/
│   │   │   │       └── subscribe.ts
│   │   │   └── components/
│   │   └── astro.config.mjs
│   └── docs/             ← documentação (Mintlify, apenas MDX + mint.json)
│       ├── mint.json
│       ├── getting-started/
│       ├── commands/
│       ├── guides/
│       └── reference/
├── packages/             ← vazio por ora, pronto para libs compartilhadas futuras
├── .github/
│   └── workflows/
│       ├── release.yml   ← existente, caminhos atualizados para apps/cli
│       ├── auto-tag.yml  ← existente
│       └── newsletter.yml ← novo
├── package.json          ← root Bun workspace
├── bunfig.toml
├── cliff.toml
├── CHANGELOG.md
├── release.sh            ← caminhos atualizados para apps/cli
└── LICENSE
```

**Decisões:**
- `src/`, `tests/`, `install.sh`, `VERSION` são movidos para `apps/cli/` — a raiz não contém mais código do CLI
- `packages/` existe como workspace válido, sem código inicial (YAGNI)
- `release.sh` e os workflows existentes têm seus caminhos atualizados para `apps/cli/`
- O `install.sh` público (URL do GitHub raw) continua funcionando desde que o caminho no repo seja atualizado no script curl da documentação

---

## Bun workspaces

`package.json` na raiz:

```json
{
  "name": "pr-tools",
  "private": true,
  "workspaces": ["apps/*", "packages/*"]
}
```

`bunfig.toml` na raiz com configurações globais de install.

---

## apps/www — Landing Page (Astro)

### Stack
- **Astro 5** com output SSR (necessário para API routes)
- **Adapter:** `@astrojs/node` (a confirmar no deploy — Vercel ou Cloudflare também suportados)
- **Estilização:** Tailwind CSS 4 + variáveis CSS custom (sem component library)
- **TypeScript** strict mode

### Seções da página (ordem)
1. **Nav** — logo, links Docs / GitHub / Instalar
2. **Hero** — headline, subheadline, badge de versão, comando curl com botão de copiar, CTAs (Ver docs, GitHub)
3. **Demo** — animação de terminal mostrando fluxo do `create-pr-description`
4. **Features** — grid 3×2 com as principais funcionalidades
5. **Providers** — badges do OpenRouter, Groq e Google Gemini
6. **Instalação rápida** — bloco de código com curl + requisitos mínimos
7. **Newsletter** — input de email + botão de inscrever, tagline "Sem spam. Cancele quando quiser."
8. **Footer** — MIT License, links Docs / GitHub / Changelog

### Estilo visual
Dark minimal com accent roxo/violeta — referências Linear e Vercel. Paleta:
- Background: `#0d0d0d` / `#0a0a0a`
- Accent: `#7c3aed` (violet-700)
- Texto: `#f8f8f8`
- Muted: `#6b7280`
- Borders: `#1f1f1f`

### API Route de inscrição

`src/pages/api/subscribe.ts`:
1. Valida formato do email
2. Chama `resend.contacts.create({ audienceId, email, unsubscribed: false })`
3. Retorna `200 { success: true }` ou erro apropriado
4. O Resend gerencia unsubscribe (RFC 8058), bounce e suppression list nativamente

---

## apps/docs — Documentação (Mintlify)

Mintlify é um serviço externo — `apps/docs` contém apenas arquivos `.mdx` e `mint.json`. Não tem `package.json` nem dependências npm. O build e deploy são feitos pela integração GitHub nativa do Mintlify, apontando para `apps/docs/` como root.

### Tema
Dark com accent violet, alinhado ao landing page (`mint.json`).

### Estrutura de navegação

```
Primeiros passos
  ├── Introdução
  ├── Instalação
  ├── Quickstart
  └── Configuração

Comandos
  ├── create-pr-description
  └── create-test-card

Guias
  ├── Configurando o Azure DevOps
  ├── Escolhendo providers de IA
  ├── Renderizando Markdown no terminal
  └── Exemplos avançados

Referência
  ├── Variáveis de ambiente
  ├── Troubleshooting
  └── Changelog
```

**Conteúdo:** migrado e expandido a partir do `README.md` existente. O Changelog na docs aponta para o `CHANGELOG.md` da raiz ou é uma página MDX que referencia o conteúdo.

---

## Newsletter — Inscrição e Envio

### Fluxo de inscrição
1. Usuário preenche email na seção Newsletter do landing page
2. `POST /api/subscribe` (Astro) valida e chama Resend Audiences API
3. Resend adiciona contato à audience e envia email de boas-vindas
4. Unsubscribe, bounce handling e suppression list gerenciados pelo Resend nativamente

### Fluxo de envio (GitHub Actions)

Trigger: `on: release: types: [published]`

Arquivo: `.github/workflows/newsletter.yml`

**Passos:**
1. Extrair a tag da release e a seção do `CHANGELOG.md` correspondente
2. Verificar idempotência: listar broadcasts no Resend e checar se já existe um com nome `release-{tag}`. Se sim, encerrar com sucesso (skip)
3. Chamar LLM (via API do OpenRouter, Groq ou Gemini) com o changelog da versão para gerar o **conteúdo em Markdown** do email
4. Converter o Markdown para HTML usando o template HTML próprio do projeto (com header, footer e estilos do pr-tools)
5. Criar Resend Broadcast com nome `release-{tag}` e o HTML gerado
6. Resend envia para todos os inscritos na audience

### Prompt LLM (geração do conteúdo)

```
Você é um redator técnico. Gere o conteúdo de uma newsletter
anunciando a versão {tag} do pr-tools (CLI para Azure DevOps + IA).

CHANGELOG desta versão:
{changelog_section}

Retorne apenas Markdown. O conteúdo deve:
- Ter tom técnico e direto (público: desenvolvedores)
- Destacar as mudanças mais relevantes com contexto
- Incluir o comando de atualização: create-pr-description --update
- Ter subject line sugerida na primeira linha como: Subject: ...
```

### Template HTML do email
Arquivo em `apps/www/src/templates/email.html` — contém o design do email (header com logo, cores do pr-tools, footer com unsubscribe link). O conteúdo Markdown gerado pelo LLM é convertido para HTML (via biblioteca como `marked`) e injetado no corpo do template.

### Idempotência
O nome do broadcast (`release-{tag}`) é único por release. Antes de criar, o workflow lista broadcasts existentes via API do Resend e encerra sem erro caso já exista — evita envios duplicados em caso de re-run do workflow.

### Secrets necessários
- `RESEND_API_KEY` — compartilhada entre `apps/www` (subscribe) e o workflow de newsletter
- `RESEND_AUDIENCE_ID` — ID da audience de inscritos
- `LLM_API_KEY` — API key do provider LLM para geração do conteúdo (ex: `OPENROUTER_API_KEY`)

---

## O que NÃO muda
- Os scripts bash do CLI (`create-pr-description`, `create-test-card`) — apenas movidos para `apps/cli/`
- O processo de release (`release.sh`, `auto-tag.yml`, `release.yml`) — apenas caminhos atualizados
- O `CHANGELOG.md` e `cliff.toml` na raiz
- O `install.sh` público (curl) — URL do GitHub raw é atualizada na documentação
