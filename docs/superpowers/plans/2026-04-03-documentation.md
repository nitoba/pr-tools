# Documentation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preencher os 13 arquivos `.mdx` placeholder em `apps/docs/content/docs/` com documentação completa em português brasileiro.

**Architecture:** Cada arquivo é autocontido e cobre exatamente o que seu título promete. Sem duplicação — seções cruzam referência via links. Conteúdo extraído do README e dos scripts CLI. Componentes Fumadocs (`Callout`, `Steps`, `Step`, `Tabs`, `Tab`) disponíveis sem import via provider global.

**Tech Stack:** Fumadocs UI + fumadocs-mdx, MDX, TanStack Start

---

## Mapa de arquivos

Todos os arquivos são modificações de existentes — nenhum arquivo novo:

- Modify: `apps/docs/content/docs/getting-started/introduction.mdx`
- Modify: `apps/docs/content/docs/getting-started/installation.mdx`
- Modify: `apps/docs/content/docs/getting-started/quickstart.mdx`
- Modify: `apps/docs/content/docs/getting-started/configuration.mdx`
- Modify: `apps/docs/content/docs/commands/create-pr-description.mdx`
- Modify: `apps/docs/content/docs/commands/create-test-card.mdx`
- Modify: `apps/docs/content/docs/guides/azure-devops.mdx`
- Modify: `apps/docs/content/docs/guides/ai-providers.mdx`
- Modify: `apps/docs/content/docs/guides/markdown-rendering.mdx`
- Modify: `apps/docs/content/docs/guides/advanced-examples.mdx`
- Modify: `apps/docs/content/docs/reference/environment-variables.mdx`
- Modify: `apps/docs/content/docs/reference/troubleshooting.mdx`
- Modify: `apps/docs/content/docs/reference/changelog.mdx`

---

### Task 1: introduction.mdx

**Files:**
- Modify: `apps/docs/content/docs/getting-started/introduction.mdx`

- [ ] **Step 1: Substituir conteúdo do arquivo**

```mdx
---
title: Introdução
description: O que é o pr-tools e para quem é indicado
---

# Introdução

**pr-tools** é um conjunto de ferramentas de linha de comando que automatiza tarefas repetitivas no fluxo de Pull Requests e Test Cases do Azure DevOps, gerando conteúdo em português brasileiro via LLM.

## O que resolve

Escrever uma boa descrição de PR ou um card de teste detalhado consome tempo e requer contexto que já existe no `git diff` e no histórico de commits. O pr-tools coleta esse contexto automaticamente e envia para um modelo de linguagem, gerando o conteúdo pronto para revisão.

## Ferramentas

### `create-pr-description`

Gera a descrição de um Pull Request a partir do `git diff` e `git log` da branch atual. Detecta automaticamente a sprint vigente, extrai o work item do nome da branch e pode criar o PR diretamente no Azure DevOps com reviewers e work items vinculados.

Use quando estiver prestes a abrir um PR e quiser uma descrição bem estruturada sem escrever do zero.

### `create-test-card`

Gera um card de teste (Test Case) a partir de um PR e seu work item no Azure DevOps. Detecta automaticamente o PR e o work item da branch atual e pode criar o Test Case filho diretamente no Azure DevOps.

Use após abrir um PR, quando precisar documentar os cenários de teste para QA.

## Providers de IA suportados

| Provider | Plano gratuito disponível |
|----------|--------------------------|
| [OpenRouter](https://openrouter.ai) | Sim |
| [Groq](https://console.groq.com) | Sim |
| [Google Gemini](https://aistudio.google.com) | Sim |
| Ollama | Sim (local/cloud) |

O fallback automático tenta o próximo provider configurado se o atual falhar ou estiver sem cota.

## Requisitos

- macOS, Linux ou Windows (WSL / Git Bash)
- Bash 4+
- `git`, `curl`, `jq`
- API key de pelo menos um dos providers acima
- (Opcional) Azure DevOps PAT para criação automática de PR e Test Case

<Callout type="info">
  Não é necessário ter o Azure DevOps configurado para usar o `create-pr-description` em modo somente geração. O PAT é necessário apenas para criar PR/Test Case automaticamente ou para leitura de contexto do work item.
</Callout>

## Próximos passos

- [Instalar o pr-tools](/docs/getting-started/installation)
- [Quickstart — gere seu primeiro PR](/docs/getting-started/quickstart)
```

- [ ] **Step 2: Commit**

```bash
git add apps/docs/content/docs/getting-started/introduction.mdx
git commit -m "docs: write introduction page"
```

---

### Task 2: installation.mdx

**Files:**
- Modify: `apps/docs/content/docs/getting-started/installation.mdx`

- [ ] **Step 1: Substituir conteúdo do arquivo**

```mdx
---
title: Instalação
description: Como instalar o pr-tools em macOS, Linux e Windows WSL
---

# Instalação

## Instalação rápida

Execute o script de instalação com `curl`:

```bash
curl -fsSL https://raw.githubusercontent.com/nitoba/pr-tools/main/install.sh | bash
```

O script instala `create-pr-description` e `create-test-card` em `~/.local/bin/` e as bibliotecas compartilhadas em `~/.local/lib/pr-tools/`.

## Instalar uma versão específica

Veja as versões disponíveis em [Releases](https://github.com/nitoba/pr-tools/releases):

```bash
curl -fsSL https://raw.githubusercontent.com/nitoba/pr-tools/main/install.sh | INSTALL_VERSION=v2.9.0 bash
```

## Instalar do branch main (bleeding edge)

```bash
curl -fsSL https://raw.githubusercontent.com/nitoba/pr-tools/main/install.sh | bash
```

## Pré-requisitos

Certifique-se de ter instalado:

| Dependência | Instalação |
|-------------|-----------|
| `git` | Incluso na maioria dos sistemas |
| `curl` | `apt install curl` / `brew install curl` |
| `jq` | `apt install jq` / `brew install jq` |
| Bash 4+ | macOS: `brew install bash` (o Bash padrão do macOS é 3.x) |

Além disso, você precisa de API key de pelo menos um provider:

- [OpenRouter](https://openrouter.ai) — plano gratuito disponível
- [Groq](https://console.groq.com) — plano gratuito disponível
- [Google Gemini](https://aistudio.google.com) — plano gratuito disponível
- Ollama — local ou via Ollama Cloud

<Callout type="warn">
  **macOS:** O Bash padrão (`/bin/bash`) é a versão 3.x, que não é compatível. Instale o Bash 4+ via Homebrew: `brew install bash`. O script de instalação detecta isso automaticamente.
</Callout>

## Verificar instalação

Após a instalação, verifique se os comandos estão disponíveis:

```bash
create-pr-description --version
create-test-card --version
```

Se o comando não for encontrado, adicione `~/.local/bin` ao seu PATH:

```bash
# Adicione ao ~/.bashrc ou ~/.zshrc
export PATH="$HOME/.local/bin:$PATH"
```

## Atualizar

Para atualizar para a versão mais recente:

```bash
create-pr-description --update
create-test-card --update
```

## Próximos passos

- [Quickstart — configure e gere seu primeiro PR](/docs/getting-started/quickstart)
```

- [ ] **Step 2: Commit**

```bash
git add apps/docs/content/docs/getting-started/installation.mdx
git commit -m "docs: write installation page"
```

---

### Task 3: quickstart.mdx

**Files:**
- Modify: `apps/docs/content/docs/getting-started/quickstart.mdx`

- [ ] **Step 1: Substituir conteúdo do arquivo**

```mdx
---
title: Quickstart
description: Gere seu primeiro PR em menos de 5 minutos
---

# Quickstart

Este guia mostra o caminho mínimo: instalar, configurar e gerar sua primeira descrição de PR.

<Steps>
  <Step>
    ### Instalar

    ```bash
    curl -fsSL https://raw.githubusercontent.com/nitoba/pr-tools/main/install.sh | bash
    ```

    Após a instalação, verifique:

    ```bash
    create-pr-description --version
    ```
  </Step>

  <Step>
    ### Configurar

    Rode o wizard de configuração interativo:

    ```bash
    create-pr-description --init
    ```

    O wizard vai pedir:

    1. Quais providers usar (OpenRouter, Groq, Gemini, Ollama)
    2. A API key de cada provider escolhido
    3. Seu Azure DevOps PAT (opcional — necessário para criar PRs automaticamente)
    4. Emails dos reviewers padrão (opcional)

    As configurações são salvas em `~/.config/pr-tools/.env`.
  </Step>

  <Step>
    ### Gerar a descrição

    Em um repositório git, na branch da sua feature:

    ```bash
    create-pr-description
    ```

    O comando vai:

    1. Coletar o `git diff` e `git log` da branch atual vs base
    2. Detectar a sprint vigente automaticamente
    3. Extrair o work item do nome da branch (ex: `feat/1234-login` → work item `1234`)
    4. Enviar o contexto para o LLM
    5. Imprimir a descrição formatada no terminal
    6. Copiar a descrição para o clipboard
    7. Perguntar se deseja criar o PR automaticamente no Azure DevOps
  </Step>
</Steps>

## Output esperado

```
==========================================
PR Description
==========================================
Provider: groq (llama-3.3-70b-versatile)

# Implementar autenticação JWT

## Contexto
Esta PR implementa o sistema de autenticação JWT para...

## Mudanças
- Adicionado middleware de autenticação
- Criado endpoint de login/refresh token
- ...

## Como testar
1. Execute `npm test`
2. Faça requisição para `/api/auth/login`
...

[OK] Descrição copiada para o clipboard
==========================================
```

<Callout type="info">
  Se a branch não tiver work item no nome (ex: `feat/1234-...`), a descrição é gerada sem vinculação de work item. Use `--work-item <id>` para vincular manualmente.
</Callout>

## Próximos passos

- [Configuração detalhada](/docs/getting-started/configuration) — todas as variáveis e opções
- [Referência do create-pr-description](/docs/commands/create-pr-description) — todas as flags
- [Configurando o Azure DevOps](/docs/guides/azure-devops) — como criar o PAT
```

- [ ] **Step 2: Commit**

```bash
git add apps/docs/content/docs/getting-started/quickstart.mdx
git commit -m "docs: write quickstart page"
```

---

### Task 4: configuration.mdx

**Files:**
- Modify: `apps/docs/content/docs/getting-started/configuration.mdx`

- [ ] **Step 1: Substituir conteúdo do arquivo**

```mdx
---
title: Configuração
description: Como configurar API keys, providers e defaults
---

# Configuração

## Wizard interativo

Na primeira execução com um terminal disponível, o wizard é iniciado automaticamente. Para rodá-lo manualmente a qualquer momento:

```bash
create-pr-description --init
# ou
create-test-card --init
```

O wizard guia a configuração de:

- Providers de IA (OpenRouter, Groq, Gemini, Ollama)
- API keys com validação automática
- Azure DevOps PAT
- Reviewers padrão para criação automática de PR
- Defaults de AreaPath e responsável para Test Cases

## Arquivo de configuração

As configurações são salvas em `~/.config/pr-tools/.env`:

```bash
# Providers ativos (ordem define prioridade do fallback)
PR_PROVIDERS="openrouter,groq,gemini,ollama"

# API Keys
OPENROUTER_API_KEY="sk-or-..."
GROQ_API_KEY="gsk_..."
GEMINI_API_KEY="..."
OLLAMA_API_KEY="oa-..."

# Azure DevOps
AZURE_PAT="your-pat-token"

# Modelos (opcional — usa o modelo padrão gratuito se não definir)
# OPENROUTER_MODEL="meta-llama/llama-3.3-70b-instruct:free"
# GROQ_MODEL="llama-3.3-70b-versatile"
# GEMINI_MODEL="gemini-3.1-flash-lite-preview"
# OLLAMA_MODEL="qwen3.5:cloud"

# Reviewers padrão para criação automática de PRs
# PR_REVIEWER_DEV="email@exemplo.com"
# PR_REVIEWER_SPRINT="email@exemplo.com"

# Defaults para Test Cases
# TEST_CARD_AREA_PATH="AGROTRACE\\Devops"
# TEST_CARD_ASSIGNED_TO="nome@exemplo.com"
```

Para editar manualmente:

```bash
vi ~/.config/pr-tools/.env
```

## Precedência de configuração

Quando a mesma variável é definida em múltiplos lugares, a ordem de prioridade é:

1. **Flags CLI** — `--work-item 1234`, `--target dev`
2. **Variáveis de ambiente** — `GROQ_MODEL=qwen/qwen3-32b create-pr-description`
3. **Arquivo `.env`** — `~/.config/pr-tools/.env`
4. **Defaults internos** — valores padrão hardcoded nos scripts

Exemplo: sobrescrever o modelo via variável de ambiente sem alterar o `.env`:

```bash
OPENROUTER_MODEL="qwen/qwen3-4b:free" create-pr-description
```

## Salvar modelo via flag

Para salvar um modelo permanentemente no `.env` sem editar o arquivo manualmente:

```bash
create-pr-description --set-openrouter-model qwen/qwen3-4b:free
create-pr-description --set-groq-model qwen/qwen3-32b
create-pr-description --set-gemini-model gemini-2.0-flash
create-pr-description --set-ollama-model llama3.2:latest
```

## Reviewers padrão

Os reviewers são usados na criação automática de PRs no Azure DevOps. Configure-os no wizard ou diretamente no `.env`:

```bash
PR_REVIEWER_DEV="dev-reviewer@empresa.com"
PR_REVIEWER_SPRINT="sprint-reviewer@empresa.com"
```

<Callout type="info">
  Os reviewers são identificados pelo email. O script resolve automaticamente o ID do usuário no Azure DevOps e cacheia o resultado em `~/.config/pr-tools/.cache`.
</Callout>

## Outros arquivos

| Arquivo | Descrição |
|---------|-----------|
| `~/.config/pr-tools/.env` | API keys e configuração |
| `~/.config/pr-tools/pr-template.md` | Template da descrição de PR (editável) |
| `~/.config/pr-tools/.cache` | Cache de repositoryId e IDs de reviewers |

## Próximos passos

- [Configurando o Azure DevOps](/docs/guides/azure-devops) — como criar o PAT e as permissões necessárias
- [Escolhendo providers de IA](/docs/guides/ai-providers) — comparação entre providers e modelos
```

- [ ] **Step 2: Commit**

```bash
git add apps/docs/content/docs/getting-started/configuration.mdx
git commit -m "docs: write configuration page"
```

---

### Task 5: create-pr-description.mdx

**Files:**
- Modify: `apps/docs/content/docs/commands/create-pr-description.mdx`

- [ ] **Step 1: Substituir conteúdo do arquivo**

```mdx
---
title: create-pr-description
description: Gera descrições de PR via LLM a partir do git diff
---

# create-pr-description

Gera a descrição de um Pull Request a partir do `git diff` e `git log` da branch atual contra a branch base (sprint ou dev), usando um LLM configurado.

## Uso

```bash
create-pr-description [opções]
```

## Flags

| Flag | Argumento | Descrição |
|------|-----------|-----------|
| `--init` | — | Inicializa ou atualiza a configuração interativamente |
| `--source <branch>` | nome da branch | Branch de origem do PR (padrão: branch atual) |
| `--target <branch>` | `dev` ou `sprint` | Branch alvo. Pode ser usada mais de uma vez. Padrão: ambas |
| `--work-item <id>` | ID numérico | ID do work item do Azure DevOps a vincular ao PR |
| `--set-openrouter-model <modelo>` | nome do modelo | Salva o modelo do OpenRouter no `.env` |
| `--set-groq-model <modelo>` | nome do modelo | Salva o modelo do Groq no `.env` |
| `--set-gemini-model <modelo>` | nome do modelo | Salva o modelo do Google Gemini no `.env` |
| `--set-ollama-model <modelo>` | nome do modelo | Salva o modelo do Ollama no `.env` |
| `--dry-run` | — | Mostra o prompt que seria enviado ao LLM sem chamá-lo |
| `--raw` | — | Exibe a descrição como texto puro, sem renderização Markdown |
| `--update` | — | Atualiza o script para a versão mais recente |
| `--help` | — | Mostra a ajuda |
| `--version` | — | Mostra a versão instalada |

## Exemplos

```bash
# Gera PR para dev e sprint (padrão)
create-pr-description

# Apenas para dev
create-pr-description --target dev

# Apenas para sprint
create-pr-description --target sprint

# PR a partir de outra branch (sem precisar fazer checkout)
create-pr-description --source feature/1234-login

# Vincular work item manualmente
create-pr-description --work-item 11763

# Ver o prompt sem chamar o LLM
create-pr-description --dry-run

# Saída sem formatação Markdown
create-pr-description --raw

# Mudar modelo do Groq e gerar
create-pr-description --set-groq-model qwen/qwen3-32b

# Sobrescrever modelo via variável de ambiente (sem salvar)
OPENROUTER_MODEL="qwen/qwen3-4b:free" create-pr-description
```

## Como funciona

<Steps>
  <Step>
    **Coleta de contexto** — Executa `git diff` e `git log` da branch atual contra a branch base (sprint ou dev).
  </Step>
  <Step>
    **Detecção de sprint** — Identifica a sprint vigente pelo maior número em `origin/sprint/*`.
  </Step>
  <Step>
    **Extração do work item** — Lê o work item do nome da branch (ex: `feat/1234-descricao` → `1234`) ou da flag `--work-item`.
  </Step>
  <Step>
    **Parse do remote** — Extrai org, project e repo da URL do git remote (Azure DevOps).
  </Step>
  <Step>
    **Chamada ao LLM** — Envia o contexto para o provider configurado via API REST, com fallback automático entre providers.
  </Step>
  <Step>
    **Extração do resultado** — Parseia título e corpo da resposta do LLM. Remove blocos `<think>` de modelos de raciocínio (ex: qwen3).
  </Step>
  <Step>
    **Output** — Imprime a descrição formatada no terminal (com Markdown renderizado se `glow`/`bat` estiver disponível).
  </Step>
  <Step>
    **Clipboard** — Copia a descrição para o clipboard (`pbcopy` no macOS, `wl-copy`/`xclip`/`xsel` no Linux).
  </Step>
  <Step>
    **Criação do PR** — Pergunta se deseja criar o PR automaticamente no Azure DevOps com reviewers e work item vinculado.
  </Step>
</Steps>

## Output esperado

```
==========================================
PR Description - feature/1234-login → dev
==========================================
Provider: groq (llama-3.3-70b-versatile)
Work item: #1234

# Implementar autenticação JWT

## Contexto
...

## Mudanças
...

[OK] Descrição copiada para o clipboard

Criar PR automaticamente no Azure DevOps? [s/N]
```

## Configuração relacionada

- [Configuração completa](/docs/getting-started/configuration)
- [Variáveis de ambiente](/docs/reference/environment-variables)
- [Escolhendo providers de IA](/docs/guides/ai-providers)
```

- [ ] **Step 2: Commit**

```bash
git add apps/docs/content/docs/commands/create-pr-description.mdx
git commit -m "docs: write create-pr-description command page"
```

---

### Task 6: create-test-card.mdx

**Files:**
- Modify: `apps/docs/content/docs/commands/create-test-card.mdx`

- [ ] **Step 1: Substituir conteúdo do arquivo**

```mdx
---
title: create-test-card
description: Gera cards de teste a partir de PR e Work Item do Azure DevOps
---

# create-test-card

Gera um card de teste (Test Case) em Markdown a partir de um PR e seu work item pai no Azure DevOps, usando um LLM. Pode criar o Test Case filho automaticamente no Azure DevOps.

## Uso

```bash
create-test-card [opções]
```

## Flags

| Flag | Argumento | Descrição |
|------|-----------|-----------|
| `--pr <id>` | ID numérico | ID do PR no Azure DevOps (override da detecção automática) |
| `--work-item <id>` | ID numérico | ID do work item pai (override da detecção automática) |
| `--project <nome>` | nome do projeto | Projeto do Azure DevOps (override) |
| `--area-path <path>` | caminho | AreaPath para criação do Test Case (override) |
| `--assigned-to <valor>` | email | Responsável pelo Test Case (override) |
| `--no-create` | — | Gera o card em Markdown mas não tenta criar no Azure DevOps |
| `--dry-run` | — | Mostra prompts e preview sem chamar o LLM nem criar |
| `--debug` | — | Exibe detalhes de diagnóstico |
| `--init` | — | Inicializa ou atualiza a configuração interativamente |
| `--update` | — | Atualiza o script para a versão mais recente |
| `--help` | — | Mostra a ajuda |

## Exemplos

```bash
# Detecta PR e work item automaticamente da branch atual
create-test-card

# Para um PR específico
create-test-card --pr 10513

# PR e work item explícitos
create-test-card --pr 10513 --work-item 11796

# Gera o Markdown sem criar no Azure DevOps
create-test-card --no-create

# Ver prompts e payload sem chamar o LLM
create-test-card --dry-run --debug

# Sobrescrever AreaPath e responsável
create-test-card --area-path "MEU-PROJETO\\QA" --assigned-to "qa@empresa.com"
```

## Autodetecção de PR e work item

Quando executado sem flags, o comando tenta detectar automaticamente:

1. **Work item** — extrai do nome da branch atual (ex: `feat/1234-login` → `1234`)
2. **PR** — busca PR ativo no Azure DevOps associado à branch atual
3. **Work item pai** — lê o work item vinculado ao PR encontrado

Se a detecção falhar em algum ponto, o comando informa o que não foi encontrado e pede para informar manualmente via flag.

## Output esperado

```
==========================================
Test Card - PR #10513
==========================================
Provider: groq (qwen/qwen3-32b)
Work Item pai: #11796 - Novo tipo de pergunta: Anexo (upload de documentos)
AreaPath Teste: AGROTRACE\Devops
Responsável: qa@empresa.com

Titulo: Testar novo tipo de pergunta 'Anexo' no CMS e formulários

## Objetivo
Validar a implementação do novo tipo de pergunta "Anexo"...

## Cenario base
1. Cadastro no CMS...
2. Configuração da estrutura...
3. Resposta do produtor...

## Checklist de testes
- [ ] Opção Anexo aparece no CMS...
- [ ] Campo de tamanho máximo respeita valor padrão...
- [ ] Upload acima do limite exibe erro...

## Resultado esperado
O tipo de pergunta Anexo funciona corretamente...

Criar Test Case no Azure DevOps? [s/N]
[OK] Test Case criado com sucesso: #12345
https://dev.azure.com/org/project/_workitems/edit/12345
==========================================
```

## Fallback quando o Azure DevOps bloqueia

Alguns projetos no Azure DevOps têm regras de processo que impedem a criação de Test Cases via API. Quando isso ocorre:

- O script mantém o Markdown gerado visível no terminal
- Informa que a criação automática falhou com a mensagem de erro do Azure DevOps
- O card pode ser criado manualmente colando o conteúdo na interface web

<Callout type="info">
  No projeto AGROTRACE, os Test Cases são criados com `Priority = 2`, `Team = DevOps` e `Programas Agrotrace = Agrotrace` como defaults além do AreaPath e responsável configurados.
</Callout>

## Configuração relacionada

- [Configuração completa](/docs/getting-started/configuration)
- [Variáveis de ambiente](/docs/reference/environment-variables)
- [Configurando o Azure DevOps](/docs/guides/azure-devops)
```

- [ ] **Step 2: Commit**

```bash
git add apps/docs/content/docs/commands/create-test-card.mdx
git commit -m "docs: write create-test-card command page"
```

---

### Task 7: azure-devops.mdx

**Files:**
- Modify: `apps/docs/content/docs/guides/azure-devops.mdx`

- [ ] **Step 1: Substituir conteúdo do arquivo**

```mdx
---
title: Configurando o Azure DevOps
description: Como configurar o PAT e as permissões necessárias
---

# Configurando o Azure DevOps

A integração com o Azure DevOps é opcional para geração de descrições, mas necessária para:

- Criar PRs automaticamente
- Ler contexto de work items na geração de Test Cases
- Criar Test Cases filhos automaticamente

## Criando um Personal Access Token (PAT)

<Steps>
  <Step>
    Acesse seu Azure DevOps: `https://dev.azure.com/<sua-org>`
  </Step>
  <Step>
    Clique no ícone do usuário (canto superior direito) → **Personal access tokens**
  </Step>
  <Step>
    Clique em **New Token**
  </Step>
  <Step>
    Configure o token:
    - **Name:** `pr-tools` (ou qualquer nome)
    - **Organization:** selecione sua organização
    - **Expiration:** escolha conforme sua política de segurança
    - **Scopes:** selecione as permissões abaixo
  </Step>
  <Step>
    Copie o token gerado — ele **não será exibido novamente**
  </Step>
</Steps>

## Permissões necessárias

| Escopo | Permissão | Necessário para |
|--------|-----------|-----------------|
| Code | Read & Write | Criar PRs |
| Work Items | Read & Write | Ler work items, criar Test Cases |
| Pull Request Threads | Read & Write | Comentários em PRs |

<Callout type="warn">
  Se for usar apenas geração de descrição sem criar PR ou Test Case automaticamente, a permissão de **Work Items: Read** é suficiente.
</Callout>

## Configurar o PAT no pr-tools

Via wizard (recomendado):

```bash
create-pr-description --init
```

Ou diretamente no `.env`:

```bash
# ~/.config/pr-tools/.env
AZURE_PAT="seu-token-aqui"
```

## Como o pr-tools identifica seu repositório

O pr-tools extrai automaticamente a org, project e repo da URL do git remote. Para repositórios no Azure DevOps, o remote tem o formato:

```
https://dev.azure.com/<org>/<project>/_git/<repo>
# ou
git@ssh.dev.azure.com:v3/<org>/<project>/<repo>
```

Não é necessário configurar org/project/repo manualmente — o script detecta pelo `git remote get-url origin`.

## Criação automática de PR

Quando o `AZURE_PAT` está configurado e você confirma a criação ao final do `create-pr-description`, o script:

1. Resolve o `repositoryId` (e cacheia em `~/.config/pr-tools/.cache`)
2. Resolve os IDs dos reviewers pelos emails configurados (e cacheia)
3. Cria o PR via API REST com título, descrição, reviewers e work item vinculado
4. Imprime o link para abrir o PR no browser

## Criação automática de Test Case

Quando o `AZURE_PAT` está configurado e você confirma a criação ao final do `create-test-card`, o script:

1. Cria um Test Case filho do work item pai via API REST
2. Preenche os campos com o conteúdo gerado pelo LLM
3. Imprime o link para o Test Case criado

Se as regras de processo do projeto bloquearem a criação via API, o Markdown fica visível no terminal para criação manual.
```

- [ ] **Step 2: Commit**

```bash
git add apps/docs/content/docs/guides/azure-devops.mdx
git commit -m "docs: write azure-devops guide page"
```

---

### Task 8: ai-providers.mdx

**Files:**
- Modify: `apps/docs/content/docs/guides/ai-providers.mdx`

- [ ] **Step 1: Substituir conteúdo do arquivo**

```mdx
---
title: Escolhendo providers de IA
description: Comparação entre OpenRouter, Groq, Google Gemini e Ollama
---

# Escolhendo providers de IA

O pr-tools suporta quatro providers de IA. Você pode configurar um ou mais — quando há múltiplos, o fallback automático tenta o próximo se o atual falhar ou estiver sem cota.

## Comparação

| Provider | Plano gratuito | Velocidade | Modelo padrão |
|----------|----------------|------------|---------------|
| [OpenRouter](https://openrouter.ai) | Sim (modelos `:free`) | Variável por modelo | `meta-llama/llama-3.3-70b-instruct:free` |
| [Groq](https://console.groq.com) | Sim | Muito rápido | `llama-3.3-70b-versatile` |
| [Google Gemini](https://aistudio.google.com) | Sim | Rápido | `gemini-3.1-flash-lite-preview` |
| Ollama | Sim (local/cloud) | Depende do hardware | `qwen3.5:cloud` |

## OpenRouter

Agrega centenas de modelos de diferentes provedores em uma única API. Ideal para experimentar modelos diferentes sem múltiplas contas.

```bash
# Obter API key: https://openrouter.ai
OPENROUTER_API_KEY="sk-or-..."

# Mudar modelo
create-pr-description --set-openrouter-model qwen/qwen3-4b:free
```

Modelos gratuitos populares: `meta-llama/llama-3.3-70b-instruct:free`, `qwen/qwen3-4b:free`, `qwen/qwen3-32b:free`

## Groq

Inferência de alta velocidade. O mais rápido para modelos de tamanho médio. Plano gratuito com limite de requisições por minuto.

```bash
# Obter API key: https://console.groq.com
GROQ_API_KEY="gsk_..."

# Mudar modelo
create-pr-description --set-groq-model qwen/qwen3-32b
```

Modelos populares: `llama-3.3-70b-versatile`, `qwen/qwen3-32b`, `llama3-8b-8192`

## Google Gemini

API do Google. Plano gratuito generoso (requisições por dia). Boa qualidade para geração de texto em português.

```bash
# Obter API key: https://aistudio.google.com
GEMINI_API_KEY="..."

# Mudar modelo
create-pr-description --set-gemini-model gemini-2.0-flash
```

Modelos populares: `gemini-3.1-flash-lite-preview`, `gemini-2.0-flash`, `gemini-1.5-pro`

## Ollama

Executa modelos localmente ou via Ollama Cloud. Útil para uso offline ou privacidade de código.

```bash
# Obter API key para Ollama Cloud: https://ollama.com
OLLAMA_API_KEY="oa-..."

# Mudar modelo
create-pr-description --set-ollama-model llama3.2:latest
```

<Callout type="info">
  Para uso local com Ollama (sem API key), deixe `OLLAMA_API_KEY` em branco e certifique-se de que o servidor Ollama está rodando em `localhost:11434`.
</Callout>

## Fallback automático

Quando há múltiplos providers em `PR_PROVIDERS`, o script tenta na ordem definida:

```bash
# Tenta openrouter primeiro, depois groq, depois gemini
PR_PROVIDERS="openrouter,groq,gemini"
```

Se o provider atual falhar (erro de API, sem cota, timeout), o próximo é tentado automaticamente. O provider e modelo usados são exibidos no output.

## Configurar múltiplos providers

```bash
# ~/.config/pr-tools/.env
PR_PROVIDERS="openrouter,groq,gemini,ollama"

OPENROUTER_API_KEY="sk-or-..."
GROQ_API_KEY="gsk_..."
GEMINI_API_KEY="..."
OLLAMA_API_KEY="oa-..."
```

<Callout type="info">
  Configure pelo menos dois providers para ter fallback em caso de indisponibilidade ou limite de requisições.
</Callout>
```

- [ ] **Step 2: Commit**

```bash
git add apps/docs/content/docs/guides/ai-providers.mdx
git commit -m "docs: write ai-providers guide page"
```

---

### Task 9: markdown-rendering.mdx

**Files:**
- Modify: `apps/docs/content/docs/guides/markdown-rendering.mdx`

- [ ] **Step 1: Substituir conteúdo do arquivo**

```mdx
---
title: Renderizando Markdown no terminal
description: Como usar glow, bat ou texto puro para visualizar a saída
---

# Renderizando Markdown no terminal

O `create-pr-description` detecta automaticamente qual renderizador está disponível e usa o melhor encontrado. Sem nenhum renderizador instalado, exibe o texto puro normalmente.

## Prioridade de detecção

O script verifica na seguinte ordem:

1. `glow` — renderização com cores e formatação rica
2. `bat` — syntax highlight de arquivos, funciona bem com Markdown
3. `batcat` — alias do `bat` em algumas distribuições Linux
4. Texto puro — fallback sempre disponível

## glow (recomendado)

Renderizador Markdown feito para o terminal. Exibe headers, negrito, código e listas formatados.

**Instalação:**

<Tabs items={['macOS', 'Linux', 'Windows WSL']}>
  <Tab value="macOS">
    ```bash
    brew install glow
    ```
  </Tab>
  <Tab value="Linux">
    ```bash
    # Debian/Ubuntu
    sudo apt install glow

    # Arch
    sudo pacman -S glow

    # Ou via Go
    go install github.com/charmbracelet/glow@latest
    ```
  </Tab>
  <Tab value="Windows WSL">
    ```bash
    # Dentro do WSL
    sudo apt install glow
    ```
  </Tab>
</Tabs>

## bat

Alternativa que oferece syntax highlight. Funciona bem para Markdown, mas a formatação é menos rica que o `glow`.

**Instalação:**

<Tabs items={['macOS', 'Linux']}>
  <Tab value="macOS">
    ```bash
    brew install bat
    ```
  </Tab>
  <Tab value="Linux">
    ```bash
    # Debian/Ubuntu
    sudo apt install bat
    # Nota: pode instalar como "batcat" em vez de "bat"
    ```
  </Tab>
</Tabs>

## Forçar texto puro

Use a flag `--raw` para desabilitar qualquer renderizador e exibir o Markdown sem formatação:

```bash
create-pr-description --raw
```

Útil quando:

- Você vai copiar o texto para colar em outra ferramenta
- O renderizador está causando problemas no seu terminal
- Você quer ver o Markdown bruto

<Callout type="info">
  A flag `--raw` afeta apenas a exibição no terminal. A cópia para o clipboard sempre usa o Markdown bruto, independente do renderizador.
</Callout>
```

- [ ] **Step 2: Commit**

```bash
git add apps/docs/content/docs/guides/markdown-rendering.mdx
git commit -m "docs: write markdown-rendering guide page"
```

---

### Task 10: advanced-examples.mdx

**Files:**
- Modify: `apps/docs/content/docs/guides/advanced-examples.mdx`

- [ ] **Step 1: Substituir conteúdo do arquivo**

```mdx
---
title: Exemplos avançados
description: Casos de uso avançados com flags e variáveis de ambiente
---

# Exemplos avançados

## PR para branch específica sem fazer checkout

Use `--source` para gerar a descrição de uma branch diferente da atual, sem precisar fazer `git checkout`:

```bash
create-pr-description --source feature/1234-novo-relatorio
```

Combinado com `--target` para especificar o alvo:

```bash
create-pr-description --source feature/1234-novo-relatorio --target dev
```

## Vincular work item manualmente

Quando a branch não segue o padrão `feat/<id>-descricao`, use `--work-item`:

```bash
create-pr-description --work-item 11763
```

## Gerar Test Case sem criar no Azure DevOps

Útil para revisar o conteúdo antes de criar, ou quando não há conexão com o Azure DevOps:

```bash
create-test-card --no-create
```

## Inspecionar o prompt enviado ao LLM

Veja exatamente o que é enviado para o modelo antes de decidir se vai chamar a API:

```bash
# Mostra o prompt sem chamar o LLM
create-pr-description --dry-run

# Para o test card, com detalhes de diagnóstico
create-test-card --dry-run --debug
```

## Sobrescrever modelo sem alterar o .env

Use variável de ambiente para um único comando sem salvar permanentemente:

```bash
# Usar um modelo diferente só desta vez
GROQ_MODEL="qwen/qwen3-32b" create-pr-description

# Usar OpenRouter com modelo específico
OPENROUTER_MODEL="qwen/qwen3-4b:free" create-pr-description --target dev
```

## Forçar uso de um único provider

Defina `PR_PROVIDERS` com apenas um provider via variável de ambiente:

```bash
PR_PROVIDERS="gemini" create-pr-description
```

## PR apenas para dev (sem sprint)

```bash
create-pr-description --target dev
```

## Test card para PR e work item explícitos

Quando a branch não tem o work item no nome ou o PR não é detectado automaticamente:

```bash
create-test-card --pr 10513 --work-item 11796
```

Com AreaPath e responsável específicos:

```bash
create-test-card --pr 10513 --work-item 11796 \
  --area-path "MEU-PROJETO\\QA" \
  --assigned-to "qa@empresa.com"
```

## Saída sem formatação para pipelines CI

```bash
create-pr-description --raw --target dev
```

## Salvar novo modelo permanentemente

```bash
# Salva no .env e sai (sem gerar PR)
create-pr-description --set-groq-model qwen/qwen3-32b
```
```

- [ ] **Step 2: Commit**

```bash
git add apps/docs/content/docs/guides/advanced-examples.mdx
git commit -m "docs: write advanced-examples guide page"
```

---

### Task 11: environment-variables.mdx

**Files:**
- Modify: `apps/docs/content/docs/reference/environment-variables.mdx`

- [ ] **Step 1: Substituir conteúdo do arquivo**

```mdx
---
title: Variáveis de ambiente
description: Referência completa de todas as variáveis configuráveis
---

# Variáveis de ambiente

Todas as variáveis abaixo podem ser definidas no arquivo `~/.config/pr-tools/.env` ou como variáveis de ambiente no shell. Variáveis de ambiente do shell têm prioridade sobre o `.env`.

## Providers

| Variável | Descrição | Padrão |
|----------|-----------|--------|
| `PR_PROVIDERS` | Lista de providers ativos, separados por vírgula. Define a ordem de fallback. | `openrouter,groq,gemini,ollama` |

## API Keys

| Variável | Descrição | Exemplo |
|----------|-----------|---------|
| `OPENROUTER_API_KEY` | API key do OpenRouter | `sk-or-v1-...` |
| `GROQ_API_KEY` | API key do Groq | `gsk_...` |
| `GEMINI_API_KEY` | API key do Google Gemini | `AIza...` |
| `OLLAMA_API_KEY` | API key do Ollama Cloud (opcional para uso local) | `oa-...` |

## Modelos

| Variável | Descrição | Padrão |
|----------|-----------|--------|
| `OPENROUTER_MODEL` | Modelo usado pelo OpenRouter | `meta-llama/llama-3.3-70b-instruct:free` |
| `GROQ_MODEL` | Modelo usado pelo Groq | `llama-3.3-70b-versatile` |
| `GEMINI_MODEL` | Modelo usado pelo Google Gemini | `gemini-3.1-flash-lite-preview` |
| `OLLAMA_MODEL` | Modelo usado pelo Ollama | `qwen3.5:cloud` |

## Azure DevOps

| Variável | Descrição | Exemplo |
|----------|-----------|---------|
| `AZURE_PAT` | Personal Access Token do Azure DevOps | `your-token` |
| `PR_REVIEWER_DEV` | Email do reviewer padrão para PRs para dev | `dev-reviewer@empresa.com` |
| `PR_REVIEWER_SPRINT` | Email do reviewer padrão para PRs para sprint | `sprint-reviewer@empresa.com` |

## Test Cases

| Variável | Descrição | Exemplo |
|----------|-----------|---------|
| `TEST_CARD_AREA_PATH` | AreaPath padrão para criação de Test Cases | `MEU-PROJETO\\Devops` |
| `TEST_CARD_ASSIGNED_TO` | Responsável padrão para Test Cases | `qa@empresa.com` |

## Exemplo completo do .env

```bash
# ~/.config/pr-tools/.env

# Providers ativos
PR_PROVIDERS="openrouter,groq,gemini"

# API Keys
OPENROUTER_API_KEY="sk-or-..."
GROQ_API_KEY="gsk_..."
GEMINI_API_KEY="..."

# Azure DevOps
AZURE_PAT="seu-pat-token"

# Modelos (descomente para sobrescrever o padrão)
# OPENROUTER_MODEL="meta-llama/llama-3.3-70b-instruct:free"
# GROQ_MODEL="llama-3.3-70b-versatile"
# GEMINI_MODEL="gemini-3.1-flash-lite-preview"

# Reviewers padrão
# PR_REVIEWER_DEV="dev@empresa.com"
# PR_REVIEWER_SPRINT="sprint@empresa.com"

# Test Cases
# TEST_CARD_AREA_PATH="MEU-PROJETO\\Devops"
# TEST_CARD_ASSIGNED_TO="qa@empresa.com"
```

<Callout type="info">
  Variáveis de ambiente do shell sempre sobrescrevem o `.env`. Flags CLI sobrescrevem ambos. Veja [precedência de configuração](/docs/getting-started/configuration#precedência-de-configuração) para detalhes.
</Callout>
```

- [ ] **Step 2: Commit**

```bash
git add apps/docs/content/docs/reference/environment-variables.mdx
git commit -m "docs: write environment-variables reference page"
```

---

### Task 12: troubleshooting.mdx

**Files:**
- Modify: `apps/docs/content/docs/reference/troubleshooting.mdx`

- [ ] **Step 1: Substituir conteúdo do arquivo**

```mdx
---
title: Troubleshooting
description: Soluções para problemas comuns
---

# Troubleshooting

## Comando não encontrado após instalação

**Sintoma:** `command not found: create-pr-description` após rodar o script de instalação.

**Causa:** `~/.local/bin` não está no PATH.

**Solução:**

```bash
# Adicione ao ~/.bashrc, ~/.zshrc ou ~/.config/fish/config.fish
export PATH="$HOME/.local/bin:$PATH"

# Recarregue o shell
source ~/.bashrc
```

## API key inválida ou sem cota

**Sintoma:** Erro `401 Unauthorized` ou `429 Too Many Requests` ao chamar o LLM.

**Causa:** API key inválida, expirada ou limite de requisições atingido.

**Solução:**

1. Verifique se a API key está correta no `.env`: `cat ~/.config/pr-tools/.env`
2. Verifique se há cota disponível no painel do provider
3. Configure um provider alternativo como fallback:

```bash
# Adicionar Groq como fallback do OpenRouter
PR_PROVIDERS="openrouter,groq"
```

## Azure PAT sem permissão

**Sintoma:** Erro `401` ou `403` ao criar PR ou Test Case no Azure DevOps.

**Causa:** O PAT não tem as permissões necessárias ou expirou.

**Solução:**

1. Acesse `https://dev.azure.com/<sua-org>` → ícone do usuário → **Personal access tokens**
2. Crie um novo token com as permissões: **Code: Read & Write** e **Work Items: Read & Write**
3. Atualize o token no `.env`:

```bash
create-pr-description --init
```

## `jq` não instalado

**Sintoma:** `jq: command not found` ao executar qualquer comando.

**Causa:** `jq` é uma dependência obrigatória não instalada.

**Solução:**

```bash
# Debian/Ubuntu
sudo apt install jq

# macOS
brew install jq

# Fedora
sudo dnf install jq
```

## Clipboard não funciona

**Sintoma:** Mensagem de aviso sobre clipboard mas a descrição é exibida normalmente.

**Causa:** Nenhuma ferramenta de clipboard disponível (`pbcopy`, `wl-copy`, `xclip`, `xsel`).

**Solução:**

```bash
# Linux com Wayland
sudo apt install wl-clipboard

# Linux com X11
sudo apt install xclip
# ou
sudo apt install xsel
```

macOS usa `pbcopy` nativamente — não precisa instalar nada.

## Fallback de provider inesperado

**Sintoma:** O output mostra um provider diferente do configurado como principal.

**Causa:** O provider principal falhou (sem cota, timeout, erro de rede) e o fallback foi acionado.

**Comportamento esperado:** O script tenta os providers na ordem definida em `PR_PROVIDERS` e usa o primeiro que responder com sucesso. O provider e modelo usados são sempre exibidos no output.

**Para forçar um provider específico:**

```bash
PR_PROVIDERS="groq" create-pr-description
```

## Bash version incompatível (macOS)

**Sintoma:** Erros de sintaxe como `syntax error near unexpected token` no macOS.

**Causa:** O macOS vem com Bash 3.x por padrão, incompatível com os scripts que requerem Bash 4+.

**Solução:**

```bash
brew install bash
# O script de instalação detecta e usa o Bash correto automaticamente
```

## Bibliotecas não encontradas

**Sintoma:** `[ERRO] Falha ao baixar` ao executar o comando.

**Causa:** As bibliotecas compartilhadas foram removidas ou corrompidas.

**Solução:** Reinstale o pr-tools:

```bash
curl -fsSL https://raw.githubusercontent.com/nitoba/pr-tools/main/install.sh | bash
```
```

- [ ] **Step 2: Commit**

```bash
git add apps/docs/content/docs/reference/troubleshooting.mdx
git commit -m "docs: write troubleshooting reference page"
```

---

### Task 13: changelog.mdx

**Files:**
- Modify: `apps/docs/content/docs/reference/changelog.mdx`

- [ ] **Step 1: Substituir conteúdo do arquivo**

```mdx
---
title: Changelog
description: Histórico de versões do pr-tools
---

# Changelog

O histórico completo de versões está disponível no repositório:

- [CHANGELOG.md no GitHub](https://github.com/nitoba/pr-tools/blob/main/CHANGELOG.md) — todas as versões com detalhes de cada mudança
- [Releases no GitHub](https://github.com/nitoba/pr-tools/releases) — versões com assets para download

## Versão atual

Para verificar a versão instalada:

```bash
create-pr-description --version
```
```

- [ ] **Step 2: Commit**

```bash
git add apps/docs/content/docs/reference/changelog.mdx
git commit -m "docs: write changelog placeholder page"
```
