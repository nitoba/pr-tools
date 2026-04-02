# PR Description Generator - Design Spec

**Data:** 2026-03-18
**Status:** Draft

## Objetivo

Criar uma ferramenta de linha de comando que gera automaticamente descricoes de Pull Request a partir do contexto do git (diff, log, branch name), usando APIs REST de LLM (OpenRouter e Groq) com prioridade configuravel.

## Problema

O processo manual de escrever descricoes de PR e repetitivo e consome tempo. O desenvolvedor precisa:

1. Analisar o diff manualmente
2. Escrever a descrição seguindo um formato padrao
3. Repetir para dois PRs (dev e sprint/N)

## Solucao

Um script shell + template externo + configuracao de providers que automatiza a geracao da descrição. Suporta multiplos providers de LLM (OpenRouter, Groq) com fallback configuravel.

## Requisitos

### Funcionais

- Coletar contexto do git: `git diff`, `git log`, branch name
- Detectar automaticamente a sprint vigente (branch `sprint/*` com maior numero no remote)
- Gerar descrição de PR em portugues brasileiro
- Seguir template fixo com secoes: Descrição, Alteracoes, Correcoes, Tipo de mudanca
- Imprimir output no terminal e copiar para clipboard (`pbcopy`)
- Informar as target branches: `dev` e `sprint/N`
- Gerar links clickaveis para abertura de PR no Azure DevOps (um para dev, um para sprint/N)
- Obter repositoryId do Azure DevOps via API REST (com PAT token) e cachear localmente

### Nao-funcionais

- Executar em menos de 30 segundos (depende da API do provider)
- Funcionar em macOS e Linux
- Usar modelos gratuitos por padrao (OpenRouter e Groq oferecem modelos free)

## Arquitetura

### Arquivos

```
~/.local/bin/create-pr-description    # Script principal (executavel, no PATH)
~/.config/pr-tools/pr-template.md     # Template da descrição do PR
~/.config/pr-tools/.env               # API keys e configuracao de providers
~/.config/pr-tools/.cache             # Cache de dados (repositoryId, etc.)
```

### Pre-requisitos

- `git` disponivel no PATH
- `curl` e `jq` disponiveis no PATH (para chamadas REST e parsing JSON)
- Clipboard: `pbcopy` (macOS), `xclip` ou `xsel` (Linux). Opcional — se nenhum disponivel, apenas imprime no terminal
- Pelo menos uma API key de LLM configurada (OpenRouter ou Groq)
- PAT token do Azure DevOps configurado (para obter repositoryId e gerar links de PR)

## Providers de LLM

O script suporta multiplos providers com fallback configuravel via lista de prioridade.

### Providers suportados

| Provider   | API Base URL                                    | Var de API Key     | Modelo padrao (gratuito)               |
| ---------- | ----------------------------------------------- | ------------------ | -------------------------------------- |
| OpenRouter | https://openrouter.ai/api/v1/chat/completions   | OPENROUTER_API_KEY | meta-llama/llama-3.3-70b-instruct:free |
| Groq       | https://api.groq.com/openai/v1/chat/completions | GROQ_API_KEY       | llama-3.3-70b-versatile                |

### Configuracao

Arquivo `~/.config/pr-tools/.env`:

```bash
# Providers em ordem de prioridade (tenta o primeiro, se falhar vai pro proximo)
PR_PROVIDERS="openrouter,groq"

# API Keys
OPENROUTER_API_KEY="sk-or-..."
GROQ_API_KEY="gsk_..."

# Modelos (opcional - usa padrao gratuito se nao definir)
OPENROUTER_MODEL="meta-llama/llama-3.3-70b-instruct:free"
GROQ_MODEL="llama-3.3-70b-versatile"

# Azure DevOps (para gerar links de PR)
AZURE_PAT="your-personal-access-token"
```

Variaveis de ambiente sobrescrevem o arquivo `.env` (precedencia: env var > .env > padrao).

### Logica de fallback

```
1. Le a lista PR_PROVIDERS (padrao: "openrouter,groq")
2. Para cada provider na lista:
   a. Verifica se a API key esta configurada (pula se nao)
   b. Faz a chamada REST
   c. Se sucesso (HTTP 200 + resposta valida): usa o resultado, para
   d. Se falha (erro HTTP, timeout, rate limit): loga aviso, tenta proximo
3. Se todos falharem: erro fatal
```

## Fluxo de Execucao

```
1.  Validar que esta num repo git
2.  Validar que curl e jq estao instalados
3.  Carregar configuracao (.env + variaveis de ambiente)
4.  Validar que pelo menos uma API key de LLM esta configurada
5.  Validar que nao esta em branch dev/main (nao faz sentido gerar PR da base)
6.  Coletar: branch name via git branch --show-current
7.  Parsear remote origin para extrair org/project/repo do Azure DevOps
8.  Obter repositoryId (do cache ou via API com PAT)
9.  Determinar base branch (dev) e gerar git diff dev...HEAD e git log dev...HEAD
10. git fetch --prune origin (com tratamento de falha de rede)
11. Detectar sprint vigente: branch sprint/* com maior numero no remote
12. Montar links de PR para dev e sprint/N
13. Ler template de ~/.config/pr-tools/pr-template.md
14. Montar prompt = template + contexto git
15. Iterar pelos providers (PR_PROVIDERS) ate obter resposta:
    a. Montar payload JSON para a API do provider
    b. Fazer chamada REST via curl
    c. Parsear resposta com jq
    d. Se falha: logar aviso, tentar proximo provider
16. Imprimir cabecalho com info da branch, targets e provider usado
17. Imprimir descrição gerada
18. Imprimir links de abertura de PR (clickaveis no terminal)
19. Copiar descrição para clipboard (pbcopy)
20. Mostrar confirmacao de copia
```

## Links de Abertura de PR (Azure DevOps)

O script gera links clickaveis que abrem diretamente a tela de criacao de PR no Azure DevOps.

### Formato do link

```
https://dev.azure.com/{org}/{project}/_git/{repo}/pullrequestcreate
  ?sourceRef={branch_name}
  &targetRef={target_branch}
  &sourceRepositoryId={repo_id}
  &targetRepositoryId={repo_id}
```

### Obtencao dos parametros

| Parametro     | Como obter                                                   |
| ------------- | ------------------------------------------------------------ |
| org           | Extrair da URL do remote origin: `git remote get-url origin` |
| project       | Extrair da URL do remote origin                              |
| repo          | Extrair da URL do remote origin                              |
| branch_name   | `git branch --show-current`                                  |
| target_branch | `dev` e `sprint/N` (dois links separados)                    |
| repo_id       | API REST do Azure DevOps (cacheado)                          |

### Parsing da URL do remote

O remote origin pode ter dois formatos:

```bash
# Formato HTTPS
https://dev.azure.com/{org}/{project}/_git/{repo}
# ou
https://{org}@dev.azure.com/{org}/{project}/_git/{repo}

# Formato SSH
git@ssh.dev.azure.com:v3/{org}/{project}/{repo}
```

O script faz parsing de ambos para extrair org, project e repo.

### Obtencao do repositoryId via API

Na primeira execucao, o script busca o repositoryId via API REST do Azure DevOps:

```bash
repo_id=$(curl -s -u ":$AZURE_PAT" \
  "https://dev.azure.com/$org/$project/_apis/git/repositories/$repo?api-version=7.0" \
  | jq -r '.id')
```

O resultado e cacheado em `~/.config/pr-tools/.cache`:

```bash
# Formato: remote_url=repo_id
https://dev.azure.com/ibsbiosistemico/AGROTRACE/_git/agrotrace-v3=a7b263e4-20cb-43d1-a7c9-381a70cd6ff5
```

Isso permite que o script funcione com multiplos repos sem pedir o ID novamente.

### Fallback sem PAT

Se AZURE_PAT nao estiver configurado:

- Gera os links SEM os parametros repositoryId (pode funcionar, depende do Azure DevOps)
- Exibe aviso: "Links gerados sem repositoryId. Configure AZURE_PAT para links completos."

## Deteccao da Sprint Vigente

```bash
git fetch --prune origin
sprint_number=$(git branch -r | grep 'origin/sprint/' | \
  sed 's|.*origin/sprint/||' | sort -n | tail -1)
```

- Se nenhuma branch `sprint/*` for encontrada, avisa o usuario e usa apenas `dev` como target
- O numero da sprint e exibido no cabecalho do output

## Limites de Contexto

Para evitar estourar o contexto do modelo:

- Diff limitado a **8000 linhas** (truncado com nota informativa)
- Log limitado a **50 commits**
- Se truncado, o prompt inclui: `[diff truncado, mostrando primeiras 8000 linhas]`

## Template do PR

O template e armazenado em `~/.config/pr-tools/pr-template.md` e funciona como instrucao para o LLM. Conteudo:

```markdown
Analise o diff e log do git fornecidos e gere uma descrição de PR em portugues
brasileiro seguindo EXATAMENTE este formato:

---

## Descrição

<Resumo conciso em 1-2 frases do que a mudanca faz e por que>

## Alteracoes

### Componentes atualizados

<Para cada componente/arquivo modificado significativamente, liste:>

- **nome-do-componente**: <Descrição das mudancas neste componente, focando no
  que mudou funcionalmente, nao linha por linha>

### Correcoes / Melhorias tecnicas

<Se houver correcoes de bugs, refatoracoes ou melhorias tecnicas, liste aqui.
Se nao houver, omita esta secao.>

## Tipo de mudanca

<Marque com [x] os tipos que se aplicam, baseado na analise do diff:>

- [ ] Bug fix
- [ ] Nova feature
- [ ] Breaking change
- [ ] Refactoring

---

## Regras:

- Escreva em portugues brasileiro
- Seja tecnico mas conciso
- Foque no "o que" e "por que", nao no "como"
- Use nomes reais de componentes/arquivos do diff
- Se o diff for muito grande, agrupe mudancas relacionadas
- Nao invente mudancas que nao estao no diff
```

## Chamada REST ao Provider

Ambos os providers (OpenRouter e Groq) usam o formato compativel com a API do OpenAI.

### Payload JSON

```json
{
  "model": "$model",
  "messages": [
    {
      "role": "system",
      "content": "$template_content"
    },
    {
      "role": "user",
      "content": "## Contexto Git\n\n**Branch:** $branch_name\n**Base branches alvo:** dev, sprint/$sprint_number\n\n### Git Log:\n$git_log\n\n### Git Diff:\n$git_diff"
    }
  ],
  "temperature": 0.3
}
```

### Chamada curl

```bash
response=$(curl -s -w "\n%{http_code}" \
  --max-time 60 \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $api_key" \
  -d "$payload" \
  "$api_url")

http_code=$(echo "$response" | tail -1)
body=$(echo "$response" | sed '$d')

# Extrair conteudo da resposta
description=$(echo "$body" | jq -r '.choices[0].message.content')
```

### Headers especificos por provider

- **OpenRouter**: adiciona `HTTP-Referer` e `X-Title` opcionais (identificacao do app)
- **Groq**: nenhum header adicional necessario

## Tratamento de Erros

| Condicao                         | Acao                                                                                                                              |
| -------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| Nao esta num repo git            | Erro: "Nao e um repositório git" (exit 1)                                                                                         |
| `curl` ou `jq` nao instalados    | Erro: "Dependencias nao encontradas: curl e jq sao necessarios." (exit 1)                                                         |
| Nenhuma API key configurada      | Erro: "Nenhuma API key configurada. Execute 'create-pr-description --init' e configure o .env." (exit 1)                          |
| Branch e dev/main/master         | Erro: "Voce esta na branch base. Mude para uma feature branch." (exit 1)                                                          |
| Nenhuma branch sprint/\*         | Aviso: "Nenhuma branch sprint encontrada. Usando apenas dev como target." (exit 0)                                                |
| Diff vazio                       | Erro: "Nenhuma alteracao encontrada em relacao a dev." (exit 1)                                                                   |
| Provider retorna erro HTTP       | Aviso: "Provider [X] falhou (HTTP [code]). Tentando proximo..." Tenta fallback.                                                   |
| Todos os providers falharam      | Erro: "Todos os providers falharam. Verifique suas API keys e conexao." (exit 1)                                                  |
| Timeout na chamada (>60s)        | Aviso: "Timeout no provider [X]. Tentando proximo..." Tenta fallback.                                                             |
| Rate limit (HTTP 429)            | Aviso: "Rate limit no provider [X]. Tentando proximo..." Tenta fallback.                                                          |
| Clipboard nao disponivel         | Aviso: "Nenhum comando de clipboard encontrado (pbcopy/xclip/xsel). Descrição exibida apenas no terminal." (exit 0)               |
| Template nao existe              | Erro: "Template nao encontrado em ~/.config/pr-tools/pr-template.md. Execute 'create-pr-description --init' para criar." (exit 1) |
| git fetch falha (offline/rede)   | Aviso: "Falha ao fazer fetch do remote. Usando dados locais." Continua com branches locais. (exit 0)                              |
| AZURE_PAT nao configurado        | Aviso: "Links gerados sem repositoryId. Configure AZURE_PAT para links completos." (exit 0)                                       |
| API do Azure DevOps falha        | Aviso: "Falha ao obter repositoryId. Links gerados sem repositoryId." (exit 0)                                                    |
| Remote origin nao e Azure DevOps | Aviso: "Remote nao e Azure DevOps. Links de PR nao serao gerados." (exit 0)                                                       |

## Output Esperado

```
==========================================
PR Description - feature/dark-mode
Target branches: dev, sprint/97
Provider: openrouter (meta-llama/llama-3.3-70b-instruct:free)
==========================================

## Descrição

Adiciona suporte ao tema escuro (dark mode) em multiplos componentes
da home page e do dashboard...

## Alteracoes

### Componentes atualizados
...

## Tipo de mudanca

- [ ] Bug fix
- [x] Nova feature
...

==========================================
Abrir PR:

  -> dev:
     https://dev.azure.com/ibsbiosistemico/AGROTRACE/_git/agrotrace-v3/pullrequestcreate?sourceRef=feature/dark-mode&targetRef=dev&sourceRepositoryId=a7b263e4-...&targetRepositoryId=a7b263e4-...

  -> sprint/97:
     https://dev.azure.com/ibsbiosistemico/AGROTRACE/_git/agrotrace-v3/pullrequestcreate?sourceRef=feature/dark-mode&targetRef=sprint/97&sourceRepositoryId=a7b263e4-...&targetRepositoryId=a7b263e4-...

Descrição copiada para o clipboard!
==========================================
```

Os links sao clickaveis em terminais modernos (iTerm2, Terminal.app, Windows Terminal, etc.) e abrem diretamente no navegador.

## Flags do CLI

### --init

- Cria o diretorio `~/.config/pr-tools/` se nao existir
- Escreve o template padrao em `~/.config/pr-tools/pr-template.md`
- Cria o arquivo `.env` com estrutura padrao (API keys vazias para preencher, incluindo AZURE_PAT)
- Cria o arquivo `.cache` vazio
- Se os arquivos ja existirem, pergunta se deseja sobrescrever
- Util para setup inicial ou para restaurar configuracao padrao

### --target \<branch\>

Controla para quais branches os links de PR sao gerados. Pode ser usado multiplas vezes.

```bash
# Padrao: ambas (dev + sprint)
create-pr-description

# Apenas para dev
create-pr-description --target dev

# Apenas para sprint
create-pr-description --target sprint

# Explicito ambas (mesmo que o padrao)
create-pr-description --target dev --target sprint
```

Valores aceitos:

- `dev` — gera link para a branch `dev`
- `sprint` — gera link para a branch `sprint/N` (detectada automaticamente)

Se `--target sprint` for usado mas nenhuma branch `sprint/*` for encontrada, exibe erro e nao gera o link.

### Resumo de uso

```
create-pr-description [opcoes]

Opcoes:
  --init               Inicializa arquivos de configuracao
  --target <branch>    Target do PR: dev, sprint (pode repetir; padrao: ambos)
  --help               Mostra ajuda
```

## Fora de Escopo

- Criacao automatica do PR no Azure DevOps (gera descrição + links, mas nao cria o PR)
- Suporte a multiplos templates por projeto
- Configuracao de base branch diferente de `dev`
- Streaming da resposta (aguarda resposta completa)
- Providers alem de OpenRouter e Groq (extensivel no futuro)
