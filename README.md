# pr-tools

Ferramentas de produtividade para Pull Requests. Gera descricoes de PR automaticamente usando IA, com links para Azure DevOps.

## Instalacao

```bash
curl -fsSL https://raw.githubusercontent.com/nitoba/pr-tools/main/install.sh | bash
```

### Requisitos

- `git`, `curl`, `jq`
- Bash 4+ (macOS, Linux, Windows WSL/Git Bash)
- API key de pelo menos um provider: [OpenRouter](https://openrouter.ai) ou [Groq](https://console.groq.com)

### Atualizacao

Rode o mesmo comando de instalacao. Ele sobrescreve o script com a versao mais recente.

## Configuracao

Na primeira instalacao, um **wizard interativo** guia a configuracao:

- Escolha de providers (OpenRouter, Groq ou ambos)
- API keys (com validacao automatica)
- Azure DevOps PAT (opcional, para links com repositoryId)

Para reconfigurar a qualquer momento:

```bash
create-pr-description --init
```

### Configuracao manual

Tambem e possivel editar diretamente `~/.config/pr-tools/.env`:

```bash
vi ~/.config/pr-tools/.env
```

```bash
PR_PROVIDERS="openrouter,groq"
OPENROUTER_API_KEY="sk-or-..."
GROQ_API_KEY="gsk_..."
AZURE_PAT="your-pat-token"
```

Variaveis de ambiente sobrescrevem o `.env`:

```bash
OPENROUTER_MODEL="qwen/qwen3-4b:free" create-pr-description
```

## Uso

De dentro de um repositorio git, em uma feature branch:

```bash
# Gera PR para dev + sprint (padrao)
create-pr-description

# Apenas para dev
create-pr-description --target dev

# Apenas para sprint
create-pr-description --target sprint
```

### Output

```
==========================================
PR Description - feat/dark-mode
Target branches: dev, sprint/97
Provider: openrouter (meta-llama/llama-3.3-70b-instruct:free)
==========================================

## Descricao
Adiciona suporte ao tema escuro em multiplos componentes...

## Alteracoes
### Componentes atualizados
- **home-padrao**: Skeletons de loading adaptados para dark mode...

## Tipo de mudanca
- [ ] Bug fix
- [x] Nova feature
- [ ] Breaking change
- [ ] Refactoring

==========================================
Abrir PR:

  -> dev:
     https://dev.azure.com/org/project/_git/repo/pullrequestcreate?sourceRef=feat/dark-mode&targetRef=dev&...

  -> sprint/97:
     https://dev.azure.com/org/project/_git/repo/pullrequestcreate?sourceRef=feat/dark-mode&targetRef=sprint/97&...

Descricao copiada para o clipboard!
==========================================
```

A descricao e copiada automaticamente para o clipboard. Os links sao clickaveis no terminal.

## Funcionalidades

- Gera descricoes de PR em portugues brasileiro via LLM
- Suporta OpenRouter e Groq com fallback automatico
- Detecta sprint vigente automaticamente (`sprint/*` branches)
- Gera links clickaveis para abrir PR no Azure DevOps
- Cacheia `repositoryId` localmente (busca via API uma vez)
- Copia descricao para clipboard (pbcopy/xclip/xsel)
- Funciona em macOS, Linux e Windows (WSL/Git Bash)

## Comandos

```
create-pr-description [opcoes]

Opcoes:
  --init               Inicializa arquivos de configuracao
  --target <branch>    Target do PR: dev, sprint (pode repetir; padrao: ambos)
  --help               Mostra ajuda
  --version            Mostra a versao
```

## Como funciona

1. Coleta `git diff` e `git log` da branch atual vs `dev`
2. Detecta a sprint vigente (maior numero em `origin/sprint/*`)
3. Parseia o remote para extrair org/project/repo do Azure DevOps
4. Envia o contexto para um LLM via API REST
5. Imprime a descricao formatada + links de PR
6. Copia para o clipboard

## Providers suportados

| Provider | Modelo padrao (gratuito) |
|---|---|
| [OpenRouter](https://openrouter.ai) | `meta-llama/llama-3.3-70b-instruct:free` |
| [Groq](https://console.groq.com) | `llama-3.3-70b-versatile` |

Voce pode trocar o modelo via `.env` ou variavel de ambiente:

```bash
OPENROUTER_MODEL="qwen/qwen3-4b:free" create-pr-description
```

## Estrutura de arquivos

```
~/.local/bin/create-pr-description    # Script principal
~/.config/pr-tools/pr-template.md     # Template da descricao (editavel)
~/.config/pr-tools/.env               # API keys e configuracao
~/.config/pr-tools/.cache             # Cache de repositoryId
```

## Licenca

MIT
